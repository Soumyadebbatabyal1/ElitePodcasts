use adw::prelude::*;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::rc::Rc;
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use crate::discovery_service::USER_AGENT;

const MAX_IMAGE_BYTES: u64 = 3 * 1024 * 1024;
const MAX_CACHED_IMAGES: usize = 128;
const BATCH_WAIT: Duration = Duration::from_secs(15);

pub struct ArtworkLoader {
    jobs: mpsc::Sender<String>,
    pending: RefCell<HashSet<String>>,
    cache: RefCell<HashMap<String, gtk::gdk::Texture>>,
    failed: RefCell<HashSet<String>>,
}

impl ArtworkLoader {
    pub fn new() -> Rc<Self> {
        let (jobs_tx, jobs_rx) = mpsc::channel::<String>();
        let jobs_rx = Arc::new(Mutex::new(jobs_rx));
        let (done_tx, done_rx) = mpsc::channel::<(String, Option<Vec<u8>>)>();
        for _ in 0..4 {
            let jobs_rx = jobs_rx.clone();
            let done_tx = done_tx.clone();
            std::thread::spawn(move || {
                loop {
                    let job = jobs_rx.lock().expect("artwork queue mutex poisoned").recv();
                    let Ok(url) = job else { break };
                    let bytes = fetch(&url);
                    if done_tx.send((url, bytes)).is_err() {
                        break;
                    }
                }
            });
        }
        let loader = Rc::new(Self {
            jobs: jobs_tx,
            pending: RefCell::new(HashSet::new()),
            cache: RefCell::new(HashMap::new()),
            failed: RefCell::new(HashSet::new()),
        });
        let weak = Rc::downgrade(&loader);
        gtk::glib::timeout_add_local(Duration::from_millis(100), move || {
            let Some(loader) = weak.upgrade() else {
                return gtk::glib::ControlFlow::Break;
            };
            while let Ok((url, bytes)) = done_rx.try_recv() {
                loader.pending.borrow_mut().remove(&url);
                if let Some(texture) = bytes.and_then(|bytes| decode(&bytes)) {
                    let mut cache = loader.cache.borrow_mut();
                    if cache.len() >= MAX_CACHED_IMAGES {
                        if let Some(old_url) = cache.keys().next().cloned() {
                            cache.remove(&old_url);
                        }
                    }
                    cache.insert(url, texture);
                } else {
                    loader.failed.borrow_mut().insert(url);
                }
            }
            gtk::glib::ControlFlow::Continue
        });
        loader
    }

    pub fn cached_widget(&self, candidates: &[String], size: i32) -> gtk::Widget {
        let selected = candidates
            .iter()
            .find(|url| self.cache.borrow().contains_key(url.as_str()));
        self.make_widget(selected.map(String::as_str), size)
    }

    pub fn preload(self: &Rc<Self>, groups: Vec<Vec<String>>, ready: impl FnOnce() + 'static) {
        for candidates in &groups {
            self.advance(candidates);
        }
        let loader = self.clone();
        let started = Instant::now();
        let mut ready = Some(ready);
        gtk::glib::timeout_add_local(Duration::from_millis(100), move || {
            if started.elapsed() >= BATCH_WAIT {
                if let Some(ready) = ready.take() {
                    ready();
                }
                return gtk::glib::ControlFlow::Break;
            }
            let finished = groups
                .iter()
                .map(|candidates| loader.advance(candidates))
                .fold(true, |all_ready, ready| all_ready && ready);
            if finished {
                if let Some(ready) = ready.take() {
                    ready();
                }
                gtk::glib::ControlFlow::Break
            } else {
                gtk::glib::ControlFlow::Continue
            }
        });
    }

    fn make_widget(&self, url: Option<&str>, size: i32) -> gtk::Widget {
        let overlay = gtk::Overlay::new();
        overlay.set_size_request(size, size);
        overlay.set_overflow(gtk::Overflow::Hidden);
        overlay.add_css_class("podcast-artwork");
        let background = gtk::Box::new(gtk::Orientation::Vertical, 0);
        background.add_css_class("artwork-placeholder");
        overlay.set_child(Some(&background));
        let icon = gtk::Image::from_icon_name("audio-x-generic-symbolic");
        icon.set_pixel_size(size / 2);
        icon.set_halign(gtk::Align::Center);
        icon.set_valign(gtk::Align::Center);
        icon.add_css_class("dim-label");
        overlay.add_overlay(&icon);
        let picture = gtk::Picture::new();
        picture.set_content_fit(gtk::ContentFit::Cover);
        picture.set_can_shrink(true);
        picture.set_size_request(size, size);
        overlay.add_overlay(&picture);
        if let Some(url) = url {
            if let Some(texture) = self.cache.borrow().get(url) {
                picture.set_paintable(Some(texture));
            }
        }
        overlay.upcast()
    }

    fn valid_url(url: &str) -> bool {
        url.len() <= 2048 && (url.starts_with("https://") || url.starts_with("http://"))
    }

    fn is_ready(&self, url: &str) -> bool {
        !Self::valid_url(url)
            || self.cache.borrow().contains_key(url)
            || self.failed.borrow().contains(url)
    }

    fn advance(&self, candidates: &[String]) -> bool {
        for url in candidates {
            if !Self::valid_url(url) || self.failed.borrow().contains(url) {
                continue;
            }
            if self.cache.borrow().contains_key(url) {
                return true;
            }
            if !self.pending.borrow().contains(url) {
                self.request(url);
            }
            return false;
        }
        true
    }

    fn request(&self, url: &str) {
        if !Self::valid_url(url) || self.is_ready(url) {
            return;
        }
        let mut pending = self.pending.borrow_mut();
        if pending.insert(url.to_string()) {
            let _ = self.jobs.send(url.to_string());
        }
    }
}

fn fetch(url: &str) -> Option<Vec<u8>> {
    let response = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(5))
        .user_agent(USER_AGENT)
        .build()
        .get(url)
        .call()
        .ok()?;
    let mut bytes = Vec::new();
    response
        .into_reader()
        .take(MAX_IMAGE_BYTES + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.is_empty() || bytes.len() as u64 > MAX_IMAGE_BYTES {
        None
    } else {
        Some(bytes)
    }
}

fn decode(bytes: &[u8]) -> Option<gtk::gdk::Texture> {
    let loader = gtk::gdk_pixbuf::PixbufLoader::new();
    loader.connect_size_prepared(|loader, width, height| {
        let largest = width.max(height);
        if largest > 512 {
            let scale = 512.0 / f64::from(largest);
            loader.set_size(
                (f64::from(width) * scale).round().max(1.0) as i32,
                (f64::from(height) * scale).round().max(1.0) as i32,
            );
        }
    });
    loader.write(bytes).ok()?;
    loader.close().ok()?;
    loader.pixbuf().map(|pixbuf| gtk::gdk::Texture::for_pixbuf(&pixbuf))
}
