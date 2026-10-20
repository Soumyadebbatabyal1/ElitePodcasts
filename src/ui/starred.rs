use adw::prelude::*;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use super::artwork::ArtworkLoader;
use crate::discovery_service::Show;
use crate::following::FollowingStore;

pub(super) struct StarredPage {
    pub widget: gtk::Widget,
    pub refresh: Rc<dyn Fn()>,
}

pub(super) fn page(
    store: Rc<FollowingStore>,
    artwork: Rc<ArtworkLoader>,
    open_show: Rc<dyn Fn(Show)>,
) -> StarredPage {
    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content.set_margin_top(20);
    content.set_margin_bottom(24);
    let clamp = adw::Clamp::builder().maximum_size(1200).build();
    clamp.set_margin_start(16);
    clamp.set_margin_end(16);
    clamp.set_child(Some(&content));
    let scroller = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .child(&clamp)
        .build();
    let weak_content = content.downgrade();
    let weak_store = Rc::downgrade(&store);
    let refresh: Rc<dyn Fn()> = Rc::new(move || {
        let (Some(content), Some(store)) = (weak_content.upgrade(), weak_store.upgrade()) else { return };
        while let Some(child) = content.first_child() { content.remove(&child); }
        let items = match store.starred() {
            Ok(items) => items,
            Err(error) => {
                content.append(&adw::StatusPage::builder()
                    .icon_name("dialog-error-symbolic")
                    .title("Could not load Starred")
                    .description(&error)
                    .vexpand(true)
                    .build());
                return;
            }
        };
        if items.is_empty() {
            content.append(&adw::StatusPage::builder()
                .icon_name("starred-symbolic")
                .title("No starred episodes")
                .description("Right-click an episode and choose Star Episode.")
                .vexpand(true)
                .build());
            return;
        }
        let followed_artwork: HashMap<_, _> = store.list().unwrap_or_default().into_iter()
            .map(|show| (show.feed_url, show.artwork_urls))
            .collect();
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
        let mut seen = HashSet::new();
        for item in items {
            if !seen.insert(item.show.feed_url.clone()) { continue; }
            let mut show = item.show;
            if let Some(urls) = followed_artwork.get(&show.feed_url) {
                for url in urls.iter().rev() {
                    show.artwork_urls.retain(|candidate| candidate != url);
                    show.artwork_urls.insert(0, url.clone());
                }
            }
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
    refresh();
    let weak_scroller = scroller.downgrade();
    let refresh_for_watch = refresh.clone();
    store.watch(move || {
        if weak_scroller.upgrade().is_none() { return false; }
        let refresh = refresh_for_watch.clone();
        gtk::glib::idle_add_local_once(move || refresh());
        true
    });
    StarredPage { widget: scroller.upcast(), refresh }
}
