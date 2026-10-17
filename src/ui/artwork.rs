use adw::prelude::*;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::rc::Rc;
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

use crate::cache;
use crate::discovery_service::USER_AGENT;

const MAX_IMAGE_BYTES: u64 = 3 * 1024 * 1024;
const MAX_CACHED_IMAGES: usize = 128;

struct DecodedImage {
    pixels: Vec<u8>,
    width: i32,
    height: i32,
    rowstride: i32,
    has_alpha: bool,
}

struct ArtworkBinding {
    picture: gtk::glib::WeakRef<gtk::Picture>,
    candidates: Vec<String>,
    displayed_url: Option<String>,
}

pub struct ArtworkLoader {
    jobs: mpsc::Sender<String>,
    pending: RefCell<HashSet<String>>,
    cache: RefCell<HashMap<String, gtk::gdk::Texture>>,
    failed: RefCell<HashSet<String>>,
    bindings: RefCell<Vec<ArtworkBinding>>,
}

impl ArtworkLoader {
    pub fn new() -> Rc<Self> {
        let (jobs_tx, jobs_rx) = mpsc::channel::<String>();
        let jobs_rx = Arc::new(Mutex::new(jobs_rx));
        let (done_tx, done_rx) = mpsc::channel::<(String, Option<DecodedImage>)>();
        for _ in 0..4 {
            let jobs_rx = jobs_rx.clone();
            let done_tx = done_tx.clone();
            std::thread::spawn(move || {
                loop {
                    let job = jobs_rx.lock().expect("artwork queue mutex poisoned").recv();
                    let Ok(url) = job else { break };
                    let decoded = fetch(&url).and_then(|bytes| decode(&bytes));
                    if done_tx.send((url, decoded)).is_err() {
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
            bindings: RefCell::new(Vec::new()),
        });
        let weak = Rc::downgrade(&loader);
        gtk::glib::timeout_add_local(Duration::from_millis(100), move || {
            let Some(loader) = weak.upgrade() else {
                return gtk::glib::ControlFlow::Break;
            };
            let mut changed = false;
            for _ in 0..4 {
                let Ok((url, decoded)) = done_rx.try_recv() else { break };
                loader.pending.borrow_mut().remove(&url);
                if let Some(decoded) = decoded {
                    let pixels = gtk::glib::Bytes::from_owned(decoded.pixels);
                    let pixbuf = gtk::gdk_pixbuf::Pixbuf::from_bytes(
                        &pixels,
                        gtk::gdk_pixbuf::Colorspace::Rgb,
                        decoded.has_alpha,
                        8,
                        decoded.width,
                        decoded.height,
                        decoded.rowstride,
                    );
                    let texture = gtk::gdk::Texture::for_pixbuf(&pixbuf);
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
                changed = true;
            }
            if changed {
                loader.refresh_bindings();
            }
            gtk::glib::ControlFlow::Continue
        });
        loader
    }

    pub fn cached_widget(&self, candidates: &[String], size: i32) -> gtk::Widget {
        let (widget, picture) = self.make_widget(size);
        let mut displayed_url = None;
        self.advance(candidates);
        if let Some((url, texture)) = self.best_texture(candidates) {
            picture.set_paintable(Some(&texture));
            displayed_url = Some(url);
        }
        self.bindings.borrow_mut().push(ArtworkBinding {
            picture: picture.downgrade(),
            candidates: candidates.to_vec(),
            displayed_url,
        });
        widget
    }

    fn make_widget(&self, size: i32) -> (gtk::Widget, gtk::Picture) {
        let overlay = gtk::Overlay::new();
        overlay.set_size_request(size, size);
        overlay.set_overflow(gtk::Overflow::Hidden);
        overlay.add_css_class("podcast-artwork");
        let background = gtk::Box::new(gtk::Orientation::Vertical, 0);
        background.add_css_class("artwork-placeholder");
        overlay.set_child(Some(&background));
        let icon = gtk::Image::from_icon_name("image-x-generic-symbolic");
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
        (overlay.upcast(), picture)
    }

    fn best_texture(&self, candidates: &[String]) -> Option<(String, gtk::gdk::Texture)> {
        let cache = self.cache.borrow();
        candidates.iter()
            .find_map(|url| cache.get(url).map(|pixbuf| (url, pixbuf)))
            .map(|(url, pixbuf)| (url.clone(), pixbuf.clone()))
    }

    fn refresh_bindings(&self) {
        self.bindings.borrow_mut().retain(|binding| binding.picture.upgrade().is_some());
        for binding in self.bindings.borrow_mut().iter_mut() {
            if let Some(picture) = binding.picture.upgrade() {
                self.advance(&binding.candidates);
                if let Some((url, texture)) = self.best_texture(&binding.candidates) {
                    if binding.displayed_url.as_deref() != Some(&url) {
                        picture.set_paintable(Some(&texture));
                        binding.displayed_url = Some(url);
                    }
                }
            }
        }
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
    if let Some(bytes) = cache::read_bytes("artwork-v1", url, None) {
        if !bytes.is_empty() && bytes.len() as u64 <= MAX_IMAGE_BYTES {
            return Some(bytes);
        }
    }
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
        cache::write_bytes("artwork-v1", url, &bytes);
        Some(bytes)
    }
}

fn decode(bytes: &[u8]) -> Option<DecodedImage> {
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
    let pixbuf = loader.pixbuf()?;
    Some(DecodedImage {
        pixels: pixbuf.read_pixel_bytes().as_ref().to_vec(),
        width: pixbuf.width(),
        height: pixbuf.height(),
        rowstride: pixbuf.rowstride(),
        has_alpha: pixbuf.has_alpha(),
    })
}
