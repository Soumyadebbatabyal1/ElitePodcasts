use adw::prelude::*;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::rc::Rc;
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

use crate::cache;
use crate::discovery_service::USER_AGENT;

const MAX_IMAGE_BYTES: u64 = 8 * 1024 * 1024;
const MAX_CACHED_IMAGES: usize = 128;
const MAX_SOURCE_DIMENSION: i32 = 4096;

#[derive(Clone, Hash, Eq, PartialEq)]
struct ImageKey {
    url: String,
    size: i32,
    full_source: bool,
}

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
    decoded_size: i32,
    quality_target: i32,
    full_source: bool,
    displayed_url: Option<String>,
}

pub struct ArtworkLoader {
    jobs: mpsc::Sender<ImageKey>,
    pending: RefCell<HashSet<ImageKey>>,
    cache: RefCell<HashMap<ImageKey, gtk::gdk::Texture>>,
    failed: RefCell<HashSet<ImageKey>>,
    bindings: RefCell<Vec<ArtworkBinding>>,
}

impl ArtworkLoader {
    pub fn new() -> Rc<Self> {
        let (jobs_tx, jobs_rx) = mpsc::channel::<ImageKey>();
        let jobs_rx = Arc::new(Mutex::new(jobs_rx));
        let (done_tx, done_rx) = mpsc::channel::<(ImageKey, Option<DecodedImage>)>();
        for _ in 0..4 {
            let jobs_rx = jobs_rx.clone();
            let done_tx = done_tx.clone();
            std::thread::spawn(move || {
                loop {
                    let job = jobs_rx.lock().expect("artwork queue mutex poisoned").recv();
                    let Ok(key) = job else { break };
                    let decoded = fetch(&key.url)
                        .and_then(|bytes| decode(&bytes, key.size, key.full_source));
                    if done_tx.send((key, decoded)).is_err() {
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
                let Ok((key, decoded)) = done_rx.try_recv() else { break };
                loader.pending.borrow_mut().remove(&key);
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
                        if let Some(old_key) = cache.keys().next().cloned() {
                            cache.remove(&old_key);
                        }
                    }
                    cache.insert(key, texture);
                } else {
                    loader.failed.borrow_mut().insert(key);
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
        let decoded_size = (size.saturating_mul(2) as u32)
            .next_power_of_two()
            .clamp(128, 512) as i32;
        self.widget(candidates.to_vec(), size, decoded_size)
    }

    pub fn large_cover_widget(self: &Rc<Self>, candidates: &[String], size: i32) -> gtk::Widget {
        let (widget, picture) = self.make_widget(size, true);
        self.bindings.borrow_mut().push(ArtworkBinding {
            picture: picture.downgrade(),
            candidates: large_cover_candidates(candidates),
            decoded_size: 0,
            quality_target: 0,
            full_source: true,
            displayed_url: None,
        });
        let weak = Rc::downgrade(self);
        picture.connect_map(move |picture| {
            if let Some(loader) = weak.upgrade() {
                loader.update_large_cover(picture, size);
            }
        });
        let weak = Rc::downgrade(self);
        picture.connect_notify_local(Some("scale-factor"), move |picture, _| {
            if picture.is_mapped() {
                if let Some(loader) = weak.upgrade() {
                    loader.update_large_cover(picture, size);
                }
            }
        });
        widget
    }

    fn update_large_cover(&self, picture: &gtk::Picture, size: i32) {
        let target = size.saturating_mul(picture.scale_factor().max(1));
        let mut bindings = self.bindings.borrow_mut();
        if let Some(binding) = bindings.iter_mut().find(|binding| {
            binding.picture.upgrade().is_some_and(|bound| bound == *picture)
        }) {
            if binding.decoded_size == target {
                return;
            }
            binding.decoded_size = target;
            binding.quality_target = target;
            binding.displayed_url = None;
            picture.set_paintable(None::<&gtk::gdk::Texture>);
        }
        drop(bindings);
        self.refresh_bindings();
    }

    fn widget(&self, candidates: Vec<String>, size: i32, decoded_size: i32) -> gtk::Widget {
        let (widget, picture) = self.make_widget(size, false);
        let mut displayed_url = None;
        self.advance(&candidates, decoded_size, 0, false);
        if let Some((url, texture)) = self.best_texture(&candidates, decoded_size, false) {
            picture.set_paintable(Some(&texture));
            displayed_url = Some(url);
        }
        self.bindings.borrow_mut().push(ArtworkBinding {
            picture: picture.downgrade(),
            candidates,
            decoded_size,
            quality_target: 0,
            full_source: false,
            displayed_url,
        });
        widget
    }

    fn make_widget(&self, size: i32, large: bool) -> (gtk::Widget, gtk::Picture) {
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
        picture.set_content_fit(if large { gtk::ContentFit::ScaleDown } else { gtk::ContentFit::Cover });
        picture.set_can_shrink(true);
        picture.set_size_request(size, size);
        overlay.add_overlay(&picture);
        (overlay.upcast(), picture)
    }

    fn best_texture(&self, candidates: &[String], decoded_size: i32, full_source: bool) -> Option<(String, gtk::gdk::Texture)> {
        let cache = self.cache.borrow();
        candidates.iter()
            .filter_map(|url| cache.get(&ImageKey { url: url.clone(), size: decoded_size, full_source }).map(|texture| (url, texture)))
            .fold(None, |best: Option<(&String, &gtk::gdk::Texture)>, next| {
                match best {
                    Some(current) if current.1.width().min(current.1.height())
                        >= next.1.width().min(next.1.height()) => Some(current),
                    _ => Some(next),
                }
            })
            .map(|(url, texture)| (url.clone(), texture.clone()))
    }

    fn refresh_bindings(&self) {
        self.bindings.borrow_mut().retain(|binding| binding.picture.upgrade().is_some());
        for binding in self.bindings.borrow_mut().iter_mut() {
            if let Some(picture) = binding.picture.upgrade() {
                if binding.decoded_size == 0 { continue; }
                self.advance(&binding.candidates, binding.decoded_size, binding.quality_target, binding.full_source);
                if let Some((url, texture)) = self.best_texture(&binding.candidates, binding.decoded_size, binding.full_source) {
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

    fn is_ready(&self, key: &ImageKey) -> bool {
        !Self::valid_url(&key.url)
            || self.cache.borrow().contains_key(key)
            || self.failed.borrow().contains(key)
    }

    fn advance(&self, candidates: &[String], decoded_size: i32, quality_target: i32, full_source: bool) -> bool {
        for url in candidates {
            let key = ImageKey { url: url.clone(), size: decoded_size, full_source };
            if !Self::valid_url(url) || self.failed.borrow().contains(&key) {
                continue;
            }
            if let Some(texture) = self.cache.borrow().get(&key) {
                if texture.width().min(texture.height()) >= quality_target {
                    return true;
                }
                continue;
            }
            if !self.pending.borrow().contains(&key) {
                self.request(key);
            }
            return false;
        }
        true
    }

    fn request(&self, key: ImageKey) {
        if self.is_ready(&key) {
            return;
        }
        let mut pending = self.pending.borrow_mut();
        if pending.insert(key.clone()) {
            let _ = self.jobs.send(key);
        }
    }
}

fn large_cover_candidates(candidates: &[String]) -> Vec<String> {
    let mut result = Vec::with_capacity(candidates.len() * 2);
    for url in candidates {
        if let Some((base, suffix)) = url.rsplit_once('/') {
            let host = url.split('/').nth(2).unwrap_or_default();
            if url.starts_with("https://")
                && host.ends_with(".mzstatic.com")
                && matches!(suffix, "600x600bb.jpg" | "600x600bb.jpeg" | "100x100bb.jpg" | "100x100bb.jpeg")
            {
                let extension = suffix.rsplit_once('.').map(|(_, extension)| extension).unwrap_or("jpg");
                let larger = format!("{base}/1200x1200bb.{extension}");
                if !result.contains(&larger) {
                    result.push(larger);
                }
            }
        }
        if !result.contains(url) {
            result.push(url.clone());
        }
    }
    result
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

fn decode(bytes: &[u8], target_size: i32, full_source: bool) -> Option<DecodedImage> {
    let loader = gtk::gdk_pixbuf::PixbufLoader::new();
    // Large covers use the original image when possible and are resized once.
    // Bound unusually large sources before decoding to avoid excessive memory use.
    let decode_limit = if full_source {
        MAX_SOURCE_DIMENSION
    } else {
        target_size.saturating_mul(3).min(2048)
    };
    loader.connect_size_prepared(move |loader, width, height| {
        let largest = width.max(height);
        if largest > decode_limit {
            let scale = f64::from(decode_limit) / f64::from(largest);
            loader.set_size(
                (f64::from(width) * scale).round().max(1.0) as i32,
                (f64::from(height) * scale).round().max(1.0) as i32,
            );
        }
    });
    loader.write(bytes).ok()?;
    loader.close().ok()?;
    let pixbuf = loader.pixbuf()?;
    let largest = pixbuf.width().max(pixbuf.height());
    let pixbuf = if largest > target_size {
        let scale = f64::from(target_size) / f64::from(largest);
        pixbuf.scale_simple(
            (f64::from(pixbuf.width()) * scale).round().max(1.0) as i32,
            (f64::from(pixbuf.height()) * scale).round().max(1.0) as i32,
            gtk::gdk_pixbuf::InterpType::Hyper,
        )?
    } else {
        pixbuf
    };
    Some(DecodedImage {
        pixels: pixbuf.read_pixel_bytes().as_ref().to_vec(),
        width: pixbuf.width(),
        height: pixbuf.height(),
        rowstride: pixbuf.rowstride(),
        has_alpha: pixbuf.has_alpha(),
    })
}
