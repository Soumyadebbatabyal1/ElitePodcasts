use adw::prelude::*;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::time::Duration;

use crate::discovery_service::{Episode, Show};
use crate::following::FollowingStore;

type DownloadKey = (String, String);

#[derive(Clone)]
pub(super) struct ActiveDownload {
    pub show: Show,
    pub episode: Episode,
    pub bytes: u64,
    pub total: Option<u64>,
    pub cancelling: bool,
    order: u64,
}

struct DownloadTask {
    state: ActiveDownload,
    cancelled: Arc<AtomicBool>,
}

enum WorkerMessage {
    Progress(u64, Option<u64>),
    Finished(Result<std::path::PathBuf, String>),
}

pub(super) struct DownloadManager {
    store: Rc<FollowingStore>,
    toast: Rc<dyn Fn(&str)>,
    overlay: gtk::glib::WeakRef<adw::ToastOverlay>,
    tasks: RefCell<HashMap<DownloadKey, DownloadTask>>,
    listeners: RefCell<Vec<Box<dyn Fn() -> bool>>>,
    next_order: Cell<u64>,
    progress_toast: RefCell<Option<(adw::Toast, gtk::DrawingArea, gtk::Label)>>,
    ring_phase: Cell<f64>,
}

impl DownloadManager {
    pub fn new(
        store: Rc<FollowingStore>,
        toast: Rc<dyn Fn(&str)>,
        overlay: &adw::ToastOverlay,
    ) -> Rc<Self> {
        Rc::new(Self {
            store, toast, overlay: overlay.downgrade(),
            tasks: RefCell::new(HashMap::new()),
            listeners: RefCell::new(Vec::new()),
            next_order: Cell::new(0),
            progress_toast: RefCell::new(None),
            ring_phase: Cell::new(0.0),
        })
    }

    pub fn start(self: &Rc<Self>, show: &Show, episode: &Episode) -> Result<bool, String> {
        let key = (show.feed_url.clone(), episode.id.clone());
        if self.store.is_downloaded(&key.0, &key.1) || self.tasks.borrow().contains_key(&key) {
            return Ok(false);
        }
        if episode.audio_url.is_none() {
            return Err("This episode has no downloadable audio source".to_owned());
        }
        let cancelled = Arc::new(AtomicBool::new(false));
        let order = self.next_order.get();
        self.next_order.set(order.wrapping_add(1));
        self.tasks.borrow_mut().insert(key.clone(), DownloadTask {
            state: ActiveDownload {
                show: show.clone(), episode: episode.clone(), bytes: 0, total: None,
                cancelling: false, order,
            },
            cancelled: cancelled.clone(),
        });
        self.notify();
        self.show_progress_toast();

        let (sender, receiver) = mpsc::channel();
        let show_for_worker = show.clone();
        let episode_for_worker = episode.clone();
        std::thread::spawn(move || {
            let progress_sender = sender.clone();
            let result = crate::downloads::download_episode(
                &show_for_worker, &episode_for_worker, &cancelled,
                move |bytes, total| {
                    let _ = progress_sender.send(WorkerMessage::Progress(bytes, total));
                },
            );
            let _ = sender.send(WorkerMessage::Finished(result));
        });

        let manager = self.clone();
        gtk::glib::timeout_add_local(Duration::from_millis(100), move || {
            let mut changed = false;
            loop {
                match receiver.try_recv() {
                    Ok(WorkerMessage::Progress(bytes, total)) => {
                        if let Some(task) = manager.tasks.borrow_mut().get_mut(&key) {
                            task.state.bytes = bytes;
                            task.state.total = total;
                            changed = true;
                        }
                    }
                    Ok(WorkerMessage::Finished(result)) => {
                        manager.finish(&key, result);
                        return gtk::glib::ControlFlow::Break;
                    }
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        manager.finish(&key, Err("Episode download did not finish".to_owned()));
                        return gtk::glib::ControlFlow::Break;
                    }
                }
            }
            if changed { manager.notify(); }
            gtk::glib::ControlFlow::Continue
        });
        Ok(true)
    }

    pub fn cancel(&self, feed_url: &str, episode_id: &str) {
        let changed = {
            let mut tasks = self.tasks.borrow_mut();
            if let Some(task) = tasks.get_mut(&(feed_url.to_owned(), episode_id.to_owned())) {
                task.cancelled.store(true, Ordering::Relaxed);
                task.state.cancelling = true;
                true
            } else { false }
        };
        if changed { self.notify(); }
    }

    pub fn is_active(&self, feed_url: &str, episode_id: &str) -> bool {
        self.tasks.borrow().contains_key(&(feed_url.to_owned(), episode_id.to_owned()))
    }

    pub fn active(&self) -> Vec<ActiveDownload> {
        let mut entries: Vec<_> = self.tasks.borrow().values().map(|task| task.state.clone()).collect();
        entries.sort_by_key(|entry| entry.order);
        entries
    }

    pub fn state(&self, feed_url: &str, episode_id: &str) -> Option<ActiveDownload> {
        self.tasks.borrow().get(&(feed_url.to_owned(), episode_id.to_owned()))
            .map(|task| task.state.clone())
    }

    pub fn watch(&self, listener: impl Fn() -> bool + 'static) {
        self.listeners.borrow_mut().push(Box::new(listener));
    }

    fn finish(&self, key: &DownloadKey, result: Result<std::path::PathBuf, String>) {
        let Some(task) = self.tasks.borrow_mut().remove(key) else { return };
        if task.cancelled.load(Ordering::Relaxed) {
            if let Ok(path) = result { let _ = std::fs::remove_file(path); }
            (self.toast)("Download canceled");
        } else {
            match result {
                Ok(path) => match self.store.save_download(&task.state.show, &task.state.episode, &path) {
                    Ok(()) => (self.toast)("Episode downloaded"),
                    Err(error) => {
                        let _ = std::fs::remove_file(path);
                        (self.toast)(&error);
                    }
                },
                Err(error) => (self.toast)(&error),
            }
        }
        self.notify();
    }

    fn overall_progress(&self) -> Option<f64> {
        let tasks = self.tasks.borrow();
        if tasks.is_empty() { return Some(1.0); }
        let mut received = 0_u128;
        let mut total = 0_u128;
        for task in tasks.values() {
            let size = task.state.total?;
            received += task.state.bytes as u128;
            total += size as u128;
        }
        if total == 0 { None } else { Some((received as f64 / total as f64).clamp(0.0, 1.0)) }
    }

    fn show_progress_toast(self: &Rc<Self>) {
        let current = self.progress_toast.borrow().as_ref()
            .map(|(toast, ring, label)| (toast.clone(), ring.clone(), label.clone()));
        if let Some((toast, ring, label)) = current {
            label.set_text(&self.progress_title());
            ring.queue_draw();
            if let Some(overlay) = self.overlay.upgrade() { overlay.add_toast(toast); }
            return;
        }
        let Some(overlay) = self.overlay.upgrade() else { return };
        let content = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let icon_area = gtk::Overlay::new();
        let ring = gtk::DrawingArea::new();
        ring.set_content_width(30);
        ring.set_content_height(30);
        icon_area.set_child(Some(&ring));
        let icon = gtk::Image::from_icon_name("folder-download-symbolic");
        icon.set_pixel_size(14);
        icon.set_halign(gtk::Align::Center);
        icon.set_valign(gtk::Align::Center);
        icon_area.add_overlay(&icon);
        content.append(&icon_area);
        let label = gtk::Label::new(Some(&self.progress_title()));
        content.append(&label);
        let weak_manager = Rc::downgrade(self);
        ring.set_draw_func(move |area, cr, width, height| {
            let Some(manager) = weak_manager.upgrade() else { return };
            let color = area.style_context().lookup_color("accent_color")
                .unwrap_or_else(|| area.style_context().color());
            let radius = (width.min(height) as f64 / 2.0) - 2.0;
            let center_x = width as f64 / 2.0;
            let center_y = height as f64 / 2.0;
            cr.set_line_width(2.5);
            cr.set_source_rgba(color.red() as f64, color.green() as f64, color.blue() as f64, 0.22);
            cr.arc(center_x, center_y, radius, 0.0, std::f64::consts::TAU);
            let _ = cr.stroke();
            cr.set_source_rgba(color.red() as f64, color.green() as f64, color.blue() as f64, 1.0);
            let progress = manager.overall_progress();
            let start = progress.map_or(manager.ring_phase.get(), |_| -std::f64::consts::FRAC_PI_2);
            let sweep = progress.map_or(std::f64::consts::FRAC_PI_2, |value| std::f64::consts::TAU * value);
            cr.arc(center_x, center_y, radius, start, start + sweep);
            let _ = cr.stroke();
        });
        let notice = adw::Toast::new("Downloading episodes");
        notice.set_timeout(3);
        notice.set_custom_title(Some(&content));
        let weak_manager = Rc::downgrade(self);
        notice.connect_dismissed(move |toast| {
            if let Some(manager) = weak_manager.upgrade() {
                let is_current = manager.progress_toast.borrow().as_ref()
                    .is_some_and(|(current, _, _)| current == toast);
                if is_current { manager.progress_toast.borrow_mut().take(); }
            }
        });
        *self.progress_toast.borrow_mut() = Some((notice.clone(), ring.clone(), label));
        overlay.add_toast(notice);
        let weak_manager = Rc::downgrade(self);
        gtk::glib::timeout_add_local(Duration::from_millis(80), move || {
            let Some(manager) = weak_manager.upgrade() else { return gtk::glib::ControlFlow::Break };
            let ring = manager.progress_toast.borrow().as_ref().map(|(_, ring, _)| ring.clone());
            let Some(ring) = ring else {
                return gtk::glib::ControlFlow::Break;
            };
            if manager.overall_progress().is_none() {
                manager.ring_phase.set(manager.ring_phase.get() + 0.12);
                ring.queue_draw();
            }
            gtk::glib::ControlFlow::Continue
        });
    }

    fn progress_title(&self) -> String {
        let count = self.tasks.borrow().len();
        if count == 1 { "Downloading 1 episode".to_owned() }
        else { format!("Downloading {count} episodes") }
    }

    fn notify(&self) {
        if let Some((_, ring, label)) = self.progress_toast.borrow().as_ref() {
            label.set_text(&self.progress_title());
            ring.queue_draw();
        }
        let mut listeners = std::mem::take(&mut *self.listeners.borrow_mut());
        listeners.retain(|listener| listener());
        self.listeners.borrow_mut().splice(0..0, listeners);
    }
}
