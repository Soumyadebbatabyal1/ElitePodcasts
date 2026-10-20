use adw::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

use crate::following::FollowingStore;
use crate::playback::PlaybackController;
use super::download_manager::{ActiveDownload, DownloadManager};

pub(super) struct DownloadedPage {
    pub widget: gtk::Widget,
    pub refresh: Rc<dyn Fn()>,
}

pub(super) fn page(
    store: Rc<FollowingStore>,
    playback: Rc<PlaybackController>,
    downloads: Rc<DownloadManager>,
    toast: Rc<dyn Fn(&str)>,
) -> DownloadedPage {
    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content.set_margin_top(20);
    content.set_margin_bottom(28);
    let clamp = adw::Clamp::builder().maximum_size(850).build();
    clamp.set_margin_start(16);
    clamp.set_margin_end(16);
    clamp.set_child(Some(&content));
    let scroller = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .child(&clamp)
        .build();

    let weak_content = content.downgrade();
    let store_for_render = Rc::downgrade(&store);
    let playback_for_render = Rc::downgrade(&playback);
    let downloads_for_render = Rc::downgrade(&downloads);
    let toast_for_render = toast.clone();
    let shown_active = Rc::new(RefCell::new(Vec::<(String, String)>::new()));
    let shown_for_render = shown_active.clone();
    let refresh: Rc<dyn Fn()> = Rc::new(move || {
        let (Some(content), Some(store), Some(playback), Some(downloads)) = (
            weak_content.upgrade(), store_for_render.upgrade(), playback_for_render.upgrade(),
            downloads_for_render.upgrade(),
        ) else { return };
        while let Some(child) = content.first_child() { content.remove(&child); }
        let active = downloads.active();
        *shown_for_render.borrow_mut() = active.iter()
            .map(|entry| (entry.show.feed_url.clone(), entry.episode.id.clone()))
            .collect();
        let entries = match store.downloaded() {
            Ok(entries) => entries,
            Err(error) => {
                content.append(&status("Could not load Downloads", &error));
                return;
            }
        };
        if active.is_empty() && entries.is_empty() {
            content.append(&status(
                "No downloaded episodes",
                "Use an episode’s menu and choose Download to save it for offline listening.",
            ));
            return;
        }
        if !active.is_empty() {
            let heading = gtk::Label::new(Some("Downloading"));
            heading.set_xalign(0.0);
            heading.add_css_class("title-3");
            heading.set_margin_bottom(8);
            content.append(&heading);
            let list = gtk::ListBox::new();
            list.add_css_class("boxed-list");
            list.set_selection_mode(gtk::SelectionMode::None);
            for entry in active { list.append(&active_row(&downloads, entry)); }
            content.append(&list);
        }
        if entries.is_empty() { return; }
        if content.first_child().is_some() {
            let heading = gtk::Label::new(Some("Downloaded"));
            heading.set_xalign(0.0);
            heading.add_css_class("title-3");
            heading.set_margin_top(24);
            heading.set_margin_bottom(8);
            content.append(&heading);
        }
        let list = gtk::ListBox::new();
        list.add_css_class("boxed-list");
        list.set_selection_mode(gtk::SelectionMode::None);
        for entry in entries {
            let mut episode = entry.episode;
            let audio_uri = gtk::glib::filename_to_uri(&entry.path, None)
                .ok().map(|uri| uri.to_string());
            episode.audio_url = audio_uri;
            let (row, over_play, controls) = super::episode_row::create(
                &entry.show, &episode, &store, &playback, &downloads, &toast_for_render, None,
            );
            controls.set_played(store.is_played(&entry.show.feed_url, &episode.id), &playback);
            let show = entry.show;
            let playback = playback.clone();
            let toast = toast_for_render.clone();
            row.connect_activate(move |row| {
                if !over_play.get() {
                    super::episode::show_dialog(row, &show, &episode, playback.clone(), toast.clone());
                }
            });
            list.append(&row);
        }
        content.append(&list);
    });
    refresh();
    let weak = content.downgrade();
    let refresh_for_watch = refresh.clone();
    store.watch(move || {
        if weak.upgrade().is_some() {
            refresh_for_watch();
            true
        } else {
            false
        }
    });
    let weak = content.downgrade();
    let weak_downloads = Rc::downgrade(&downloads);
    let shown_for_watch = shown_active.clone();
    let refresh_for_watch = refresh.clone();
    downloads.watch(move || {
        let (Some(_), Some(downloads)) = (weak.upgrade(), weak_downloads.upgrade()) else { return false };
        let active: Vec<_> = downloads.active().into_iter()
            .map(|entry| (entry.show.feed_url, entry.episode.id))
            .collect();
        let changed = active != *shown_for_watch.borrow();
        if changed { refresh_for_watch(); }
        true
    });
    DownloadedPage { widget: scroller.upcast(), refresh }
}

fn active_row(downloads: &Rc<DownloadManager>, entry: ActiveDownload) -> gtk::ListBoxRow {
    let row = gtk::ListBoxRow::new();
    row.set_activatable(false);
    let layout = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    layout.set_margin_top(12);
    layout.set_margin_bottom(12);
    layout.set_margin_start(14);
    layout.set_margin_end(12);
    let text = gtk::Box::new(gtk::Orientation::Vertical, 5);
    text.set_hexpand(true);
    let title = gtk::Label::new(Some(&entry.episode.title));
    title.set_xalign(0.0);
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    title.add_css_class("heading");
    text.append(&title);
    let podcast = gtk::Label::new(Some(&entry.show.title));
    podcast.set_xalign(0.0);
    podcast.add_css_class("dim-label");
    text.append(&podcast);
    let progress_line = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    let bar = gtk::ProgressBar::new();
    bar.set_hexpand(true);
    bar.set_valign(gtk::Align::Center);
    progress_line.append(&bar);
    let progress_text = gtk::Label::new(None);
    progress_text.set_width_chars(12);
    progress_text.set_xalign(1.0);
    progress_text.add_css_class("dim-label");
    progress_line.append(&progress_text);
    text.append(&progress_line);
    layout.append(&text);
    let cancel = gtk::Button::from_icon_name("process-stop-symbolic");
    cancel.add_css_class("flat");
    cancel.set_valign(gtk::Align::Center);
    cancel.set_tooltip_text(Some("Cancel download"));
    let feed_url = entry.show.feed_url.clone();
    let episode_id = entry.episode.id.clone();
    let manager_for_cancel = downloads.clone();
    let feed_for_cancel = feed_url.clone();
    let episode_for_cancel = episode_id.clone();
    cancel.connect_clicked(move |_| manager_for_cancel.cancel(&feed_for_cancel, &episode_for_cancel));
    layout.append(&cancel);
    row.set_child(Some(&layout));
    update_active_row(&bar, &progress_text, &cancel, &entry);
    let weak_bar = bar.downgrade();
    let weak_text = progress_text.downgrade();
    let weak_cancel = cancel.downgrade();
    let weak_downloads = Rc::downgrade(downloads);
    downloads.watch(move || {
        let (Some(bar), Some(text), Some(cancel), Some(downloads)) = (
            weak_bar.upgrade(), weak_text.upgrade(), weak_cancel.upgrade(), weak_downloads.upgrade(),
        ) else { return false };
        let Some(entry) = downloads.state(&feed_url, &episode_id) else { return false };
        update_active_row(&bar, &text, &cancel, &entry);
        true
    });
    row
}

fn update_active_row(
    bar: &gtk::ProgressBar,
    text: &gtk::Label,
    cancel: &gtk::Button,
    entry: &ActiveDownload,
) {
    cancel.set_sensitive(!entry.cancelling);
    if entry.cancelling {
        text.set_text("Canceling…");
    } else if let Some(total) = entry.total {
        let fraction = (entry.bytes as f64 / total as f64).clamp(0.0, 1.0);
        bar.set_fraction(fraction);
        text.set_text(&format!("{}%", (fraction * 100.0).round() as u32));
    } else {
        bar.pulse();
        text.set_text(&format!("{:.1} MB", entry.bytes as f64 / (1024.0 * 1024.0)));
    }
}

fn status(title: &str, description: &str) -> adw::StatusPage {
    adw::StatusPage::builder()
        .icon_name("folder-download-symbolic")
        .title(title)
        .description(description)
        .vexpand(true)
        .build()
}
