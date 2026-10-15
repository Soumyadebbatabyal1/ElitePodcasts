use adw::prelude::*;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::rc::Rc;
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

use crate::discovery_service::USER_AGENT;

const MAX_IMAGE_BYTES: u64 = 3 * 1024 * 1024;
const MAX_CACHED_IMAGES: usize = 128;

pub struct ArtworkLoader {
    jobs: mpsc::Sender<String>,
    pending: RefCell<HashMap<String, Vec<gtk::glib::WeakRef<gtk::Picture>>>>,
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
            pending: RefCell::new(HashMap::new()),
            cache: RefCell::new(HashMap::new()),
            failed: RefCell::new(HashSet::new()),
        });
        let weak = Rc::downgrade(&loader);
        gtk::glib::timeout_add_local(Duration::from_millis(100), move || {
            let Some(loader) = weak.upgrade() else {
                return gtk::glib::ControlFlow::Break;
            };
            while let Ok((url, bytes)) = done_rx.try_recv() {
                let pictures = loader.pending.borrow_mut().remove(&url).unwrap_or_default();
                if let Some(texture) = bytes.and_then(|bytes| decode(&bytes)) {
                    let mut cache = loader.cache.borrow_mut();
                    if cache.len() >= MAX_CACHED_IMAGES {
                        if let Some(old_url) = cache.keys().next().cloned() {
                            cache.remove(&old_url);
                        }
                    }
                    cache.insert(url, texture.clone());
                    for picture in pictures {
                        if let Some(picture) = picture.upgrade() {
                            picture.set_paintable(Some(&texture));
                        }
                    }
                } else {
                    loader.failed.borrow_mut().insert(url);
                }
            }
            gtk::glib::ControlFlow::Continue
        });
        loader
    }

    pub fn widget(&self, url: Option<&str>, size: i32) -> gtk::Widget {
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
            self.load(url, picture);
        }
        overlay.upcast()
    }

    fn load(&self, url: &str, picture: gtk::Picture) {
        if url.len() > 2048 || !(url.starts_with("https://") || url.starts_with("http://")) {
            return;
        }
        if let Some(texture) = self.cache.borrow().get(url) {
            picture.set_paintable(Some(texture));
            return;
        }
        if self.failed.borrow().contains(url) {
            return;
        }
        let mut pending = self.pending.borrow_mut();
        if let Some(pictures) = pending.get_mut(url) {
            pictures.push(picture.downgrade());
        } else {
            pending.insert(url.to_string(), vec![picture.downgrade()]);
            let _ = self.jobs.send(url.to_string());
        }
    }
}

fn fetch(url: &str) -> Option<Vec<u8>> {
    let response = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(12))
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
        if largest > 256 {
            let scale = 256.0 / f64::from(largest);
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
