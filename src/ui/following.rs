use adw::prelude::*;
use std::cell::Cell;
use std::rc::Rc;
use std::sync::mpsc;
use std::time::Duration;

use super::artwork::ArtworkLoader;
use crate::discovery_service::{self, Show};
use crate::following::FollowingStore;

pub(super) fn page(
    store: Rc<FollowingStore>,
    artwork: Rc<ArtworkLoader>,
    open_show: Rc<dyn Fn(Show)>,
) -> gtk::Widget {
    let scroller = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .build();
    let clamp = adw::Clamp::builder().maximum_size(1200).build();
    clamp.set_margin_start(16);
    clamp.set_margin_end(16);
    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content.set_hexpand(true);
    content.set_margin_top(20);
    content.set_margin_bottom(24);
    clamp.set_child(Some(&content));
    scroller.set_child(Some(&clamp));

    let store_for_render = Rc::downgrade(&store);
    let content_for_render = content.downgrade();
    let render: Rc<dyn Fn()> = Rc::new(move || {
        let Some(content) = content_for_render.upgrade() else {
            return;
        };
        while let Some(child) = content.first_child() {
            content.remove(&child);
        }
        let Some(store_for_render) = store_for_render.upgrade() else {
            return;
        };
        let shows = match store_for_render.list() {
            Ok(shows) => shows,
            Err(message) => {
                let label = gtk::Label::new(Some(&message));
                label.set_wrap(true);
                content.append(&label);
                return;
            }
        };
        if shows.is_empty() {
            let empty = adw::StatusPage::builder()
                .icon_name("folder-music-symbolic")
                .title("No followed podcasts yet")
                .description("Add a podcast from search to see it here.")
                .vexpand(true)
                .build();
            content.append(&empty);
            return;
        }
        let grid = gtk::FlowBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .min_children_per_line(1)
            .max_children_per_line(8)
            .column_spacing(8)
            .row_spacing(8)
            .homogeneous(true)
            .halign(gtk::Align::Start)
            .hexpand(true)
            .build();
        for show in shows {
            let tile = gtk::Button::new();
            tile.add_css_class("flat");
            tile.add_css_class("following-cover");
            tile.set_halign(gtk::Align::Center);
            tile.set_tooltip_text(Some(&show.title));
            tile.set_child(Some(&artwork.large_cover_widget(&show.artwork_urls, 140)));
            let open_show = open_show.clone();
            tile.connect_clicked(move |_| open_show(show.clone()));
            grid.insert(&tile, -1);
        }
        content.append(&grid);
    });
    render();
    let weak = content.downgrade();
    store_watch(&render, &weak, &store);
    let artwork_refresh_started = Cell::new(false);
    content.connect_map(move |content| {
        if !artwork_refresh_started.replace(true) {
            refresh_saved_artwork(store.clone(), content);
        }
    });
    scroller.upcast()
}

fn refresh_saved_artwork(store: Rc<FollowingStore>, content: &gtk::Box) {
    let Ok(shows) = store.list() else { return };
    if shows.is_empty() { return }
    let (tx, rx) = mpsc::channel::<Vec<(String, String)>>();
    std::thread::spawn(move || {
        let mut missing = Vec::new();
        let mut cached = Vec::new();
        for show in shows {
            if let Some(feed) = discovery_service::cached_feed_current(&show.feed_url) {
                if let Some(url) = feed.artwork_url {
                    cached.push((show.feed_url, url));
                }
            } else {
                missing.push(show.feed_url);
            }
        }
        if !cached.is_empty() && tx.send(cached).is_err() { return }
        let mut fetched = Vec::new();
        for feed_url in missing {
            if let Ok(feed) = discovery_service::feed_fresh(&feed_url) {
                if let Some(url) = feed.artwork_url {
                    fetched.push((feed_url, url));
                }
            }
            if fetched.len() >= 3 && tx.send(std::mem::take(&mut fetched)).is_err() { return }
        }
        if !fetched.is_empty() { let _ = tx.send(fetched); }
    });
    let content = content.downgrade();
    gtk::glib::timeout_add_local(Duration::from_millis(100), move || {
        if content.upgrade().is_none() { return gtk::glib::ControlFlow::Break }
        loop {
            match rx.try_recv() {
                Ok(covers) => { let _ = store.prefer_feed_artworks(&covers); }
                Err(mpsc::TryRecvError::Empty) => return gtk::glib::ControlFlow::Continue,
                Err(mpsc::TryRecvError::Disconnected) => return gtk::glib::ControlFlow::Break,
            }
        }
    });
}

fn store_watch(
    render: &Rc<dyn Fn()>,
    weak: &gtk::glib::WeakRef<gtk::Box>,
    store: &Rc<FollowingStore>,
) {
    let render = render.clone();
    let weak = weak.clone();
    store.watch(move || {
        if weak.upgrade().is_some() {
            render();
            true
        } else {
            false
        }
    });
}
