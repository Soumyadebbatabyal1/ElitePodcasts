use adw::prelude::*;
use std::cell::Cell;
use std::rc::Rc;

use crate::discovery_service::Show;
use crate::following::FollowingStore;
use crate::playback::PlaybackController;
use super::download_manager::DownloadManager;

pub(super) struct HistoryPage {
    pub widget: gtk::Widget,
    pub refresh: Rc<dyn Fn()>,
}

pub(super) fn page(
    store: Rc<FollowingStore>,
    playback: Rc<PlaybackController>,
    downloads: Rc<DownloadManager>,
    toast: Rc<dyn Fn(&str)>,
    open_show: Rc<dyn Fn(Show)>,
) -> HistoryPage {
    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content.set_margin_top(20);
    content.set_margin_bottom(28);
    let clamp = adw::Clamp::builder().maximum_size(700).build();
    clamp.set_margin_start(16);
    clamp.set_margin_end(16);
    clamp.set_child(Some(&content));
    let scroller = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .child(&clamp)
        .build();
    let weak_content = content.downgrade();
    let weak_playback = Rc::downgrade(&playback);
    let refresh: Rc<dyn Fn()> = Rc::new(move || {
        let (Some(content), Some(playback)) = (weak_content.upgrade(), weak_playback.upgrade()) else { return };
        while let Some(child) = content.first_child() { content.remove(&child); }
        let entries = match playback.history() {
            Ok(entries) => entries,
            Err(error) => {
                content.append(&status("Could not load History", &error));
                return;
            }
        };
        if entries.is_empty() {
            content.append(&status("No completed episodes", "Episodes appear here after they finish playing."));
            return;
        }
        let now = gtk::glib::DateTime::now_local().ok();
        let mut groups: [Vec<_>; 5] = std::array::from_fn(|_| Vec::new());
        for entry in entries {
            let completed = gtk::glib::DateTime::from_unix_local(entry.completed_at).ok();
            let index = super::new_episodes::date_group(completed.as_ref(), now.as_ref());
            groups[index].push(entry);
        }
        for (index, entries) in groups.into_iter().enumerate() {
            if entries.is_empty() { continue; }
            let heading = gtk::Label::new(Some(["Today", "Yesterday", "This week", "This month", "Older"][index]));
            heading.set_xalign(0.0);
            heading.add_css_class("title-3");
            heading.set_margin_top(if content.first_child().is_none() { 0 } else { 24 });
            heading.set_margin_bottom(8);
            content.append(&heading);
            let list = gtk::ListBox::new();
            list.add_css_class("boxed-list");
            list.set_selection_mode(gtk::SelectionMode::None);
            for entry in entries {
                let (row, over_play, controls) = super::episode_row::create(
                    &entry.show, &entry.episode, &store, &playback, &downloads, &toast,
                    Some(open_show.clone()),
                );
                controls.set_played(true, &playback);
                let show = entry.show;
                let episode = entry.episode;
                let playback = playback.clone();
                let toast = toast.clone();
                row.connect_activate(move |row| {
                    if !over_play.get() {
                        super::episode::show_dialog(row, &show, &episode, playback.clone(), toast.clone());
                    }
                });
                list.append(&row);
            }
            content.append(&list);
        }
    });
    refresh();
    let seen = Rc::new(Cell::new(playback.library_revision()));
    let weak_scroller = scroller.downgrade();
    let weak_playback = Rc::downgrade(&playback);
    let refresh_for_watch = refresh.clone();
    playback.watch(move || {
        let (Some(scroller), Some(playback)) = (weak_scroller.upgrade(), weak_playback.upgrade()) else { return false };
        if scroller.is_mapped() && seen.replace(playback.library_revision()) != playback.library_revision() {
            refresh_for_watch();
        }
        true
    });
    HistoryPage { widget: scroller.upcast(), refresh }
}

fn status(title: &str, description: &str) -> adw::StatusPage {
    adw::StatusPage::builder()
        .icon_name("document-open-recent-symbolic")
        .title(title)
        .description(description)
        .vexpand(true)
        .build()
}
