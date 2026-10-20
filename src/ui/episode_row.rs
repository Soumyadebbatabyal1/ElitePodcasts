use adw::prelude::*;
use std::cell::Cell;
use std::rc::Rc;

use super::episode::{format_date, plain_description};
use crate::discovery_service::{Episode, Show};
use crate::following::FollowingStore;
use crate::playback::PlaybackController;
use super::download_manager::DownloadManager;

pub(super) struct EpisodeRowControls {
    pub row: gtk::ListBoxRow,
    pub played: Rc<Cell<bool>>,
    badge: gtk::Image,
    play_icon: gtk::Image,
    play_button: gtk::Button,
    feed_url: String,
    episode_id: String,
}

impl EpisodeRowControls {
    pub fn set_played(&self, played: bool, playback: &PlaybackController) {
        self.played.set(played);
        self.badge.set_visible(played);
        update_play_icon(
            &self.play_icon, &self.play_button, playback,
            &self.feed_url, &self.episode_id, played,
        );
    }
}

pub(super) fn create(
    show: &Show,
    episode: &Episode,
    store: &Rc<FollowingStore>,
    playback: &Rc<PlaybackController>,
    downloads: &Rc<DownloadManager>,
    toast: &Rc<dyn Fn(&str)>,
    go_to_show: Option<Rc<dyn Fn(Show)>>,
) -> (gtk::ListBoxRow, Rc<Cell<bool>>, EpisodeRowControls) {
    let row = gtk::ListBoxRow::new();
    let content = gtk::Box::new(gtk::Orientation::Horizontal, 14);
    content.set_margin_top(13);
    content.set_margin_bottom(13);
    content.set_margin_start(12);
    content.set_margin_end(12);
    let text = gtk::Box::new(gtk::Orientation::Vertical, 3);
    text.set_hexpand(true);
    text.set_valign(gtk::Align::Center);
    let date = gtk::Label::new(Some(&format_date(&episode.date)));
    date.set_xalign(0.0);
    date.add_css_class("episode-secondary-text");
    let date_line = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let played_icon = gtk::Image::from_icon_name("object-select-symbolic");
    played_icon.set_pixel_size(16);
    played_icon.set_valign(gtk::Align::Center);
    played_icon.add_css_class("episode-played-icon");
    played_icon.set_visible(false);
    date_line.append(&played_icon);
    date_line.append(&date);
    let starred_icon = gtk::Image::from_icon_name("starred-symbolic");
    starred_icon.set_pixel_size(12);
    starred_icon.set_valign(gtk::Align::Center);
    starred_icon.set_margin_start(2);
    starred_icon.add_css_class("episode-starred-icon");
    starred_icon.set_tooltip_text(Some("Starred episode"));
    starred_icon.set_visible(store.is_starred(&show.feed_url, &episode.id));
    date_line.append(&starred_icon);
    let downloaded_icon = gtk::Image::from_icon_name("folder-download-symbolic");
    downloaded_icon.set_pixel_size(12);
    downloaded_icon.set_valign(gtk::Align::Center);
    downloaded_icon.set_tooltip_text(Some("Downloaded episode"));
    downloaded_icon.set_visible(store.is_downloaded(&show.feed_url, &episode.id));
    date_line.append(&downloaded_icon);
    let weak_starred_icon = starred_icon.downgrade();
    let weak_downloaded_icon = downloaded_icon.downgrade();
    let weak_store = Rc::downgrade(store);
    let starred_feed_url = show.feed_url.clone();
    let starred_episode_id = episode.id.clone();
    store.watch(move || {
        let (Some(icon), Some(downloaded_icon), Some(store)) = (
            weak_starred_icon.upgrade(), weak_downloaded_icon.upgrade(), weak_store.upgrade(),
        ) else {
            return false;
        };
        icon.set_visible(store.is_starred(&starred_feed_url, &starred_episode_id));
        downloaded_icon.set_visible(store.is_downloaded(&starred_feed_url, &starred_episode_id));
        true
    });
    text.append(&date_line);
    let title = gtk::Label::new(Some(&episode.title));
    title.set_xalign(0.0);
    title.set_wrap(true);
    title.set_lines(2);
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    title.add_css_class("heading");
    text.append(&title);
    let summary = plain_description(&episode.description);
    if !summary.is_empty() {
        let preview = gtk::Label::new(Some(&summary));
        preview.set_xalign(0.0);
        preview.set_wrap(true);
        preview.set_lines(if episode.title.chars().count() > 55 { 1 } else { 2 });
        preview.set_ellipsize(gtk::pango::EllipsizeMode::End);
        preview.add_css_class("episode-secondary-text");
        text.append(&preview);
    }
    content.append(&text);
    let play_button = gtk::Button::new();
    play_button.add_css_class("episode-play-button");
    play_button.set_valign(gtk::Align::Center);
    let play_content = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    let play_icon = gtk::Image::from_icon_name("media-playback-start-symbolic");
    play_content.append(&play_icon);
    if let Some(seconds) = episode.duration_seconds {
        play_content.append(&gtk::Label::new(Some(&format!("{} min", (seconds + 30) / 60))));
    }
    play_button.set_child(Some(&play_content));
    play_button.set_sensitive(episode.audio_url.is_some());
    let feed_url = show.feed_url.clone();
    let episode_id = episode.id.clone();
    let played = Rc::new(Cell::new(false));
    update_play_icon(&play_icon, &play_button, playback, &feed_url, &episode_id, false);
    let weak_icon = play_icon.downgrade();
    let weak_button = play_button.downgrade();
    let weak_playback = Rc::downgrade(playback);
    let played_for_watch = played.clone();
    playback.watch(move || {
        let (Some(icon), Some(button), Some(playback)) = (
            weak_icon.upgrade(), weak_button.upgrade(), weak_playback.upgrade(),
        ) else {
            return false;
        };
        update_play_icon(&icon, &button, &playback, &feed_url, &episode_id, played_for_watch.get());
        true
    });
    let over_play = Rc::new(Cell::new(false));
    let motion = gtk::EventControllerMotion::new();
    let entered = over_play.clone();
    motion.connect_enter(move |_, _, _| entered.set(true));
    let left = over_play.clone();
    motion.connect_leave(move |_| left.set(false));
    play_button.add_controller(motion);
    let show_for_play = show.clone();
    let episode_for_play = episode.clone();
    let playback_for_play = playback.clone();
    let toast_for_play = toast.clone();
    let played_for_play = played.clone();
    play_button.connect_clicked(move |_| {
        let active = playback_for_play.is_current(&show_for_play.feed_url, &episode_for_play.id)
            && playback_for_play.is_playing();
        let result = if played_for_play.get() && !active {
            playback_for_play.replay_episode(&show_for_play, &episode_for_play)
        } else {
            playback_for_play.play_or_toggle(&show_for_play, &episode_for_play)
        };
        if let Err(error) = result {
            toast_for_play(&error);
        }
    });
    content.append(&play_button);
    row.set_child(Some(&content));
    attach_context_menu(&row, show, episode, store, playback, downloads, toast, go_to_show);
    let controls = EpisodeRowControls {
        row: row.clone(), played, badge: played_icon,
        play_icon, play_button, feed_url: show.feed_url.clone(), episode_id: episode.id.clone(),
    };
    (row, over_play, controls)
}

fn attach_context_menu(
    row: &gtk::ListBoxRow,
    show: &Show,
    episode: &Episode,
    store: &Rc<FollowingStore>,
    playback: &Rc<PlaybackController>,
    downloads: &Rc<DownloadManager>,
    toast: &Rc<dyn Fn(&str)>,
    go_to_show: Option<Rc<dyn Fn(Show)>>,
) {
    let model = gtk::gio::Menu::new();
    let has_go_to_show = go_to_show.is_some();
    let actions = gtk::gio::SimpleActionGroup::new();
    let play_now = gtk::gio::SimpleAction::new("play-now", None);
    play_now.set_enabled(episode.audio_url.is_some());
    let show_for_play = show.clone();
    let episode_for_play = episode.clone();
    let playback_for_play = playback.clone();
    let toast_for_play = toast.clone();
    play_now.connect_activate(move |_, _| {
        if let Err(error) = playback_for_play.play_episode(&show_for_play, &episode_for_play) {
            toast_for_play(&error);
        }
    });
    actions.add_action(&play_now);
    let queue_up = gtk::gio::SimpleAction::new("queue-up", None);
    queue_up.set_enabled(episode.audio_url.is_some());
    let show_for_queue = show.clone();
    let episode_for_queue = episode.clone();
    let playback_for_queue = playback.clone();
    let toast_for_queue = toast.clone();
    queue_up.connect_activate(move |_, _| {
        match playback_for_queue.enqueue(&show_for_queue, &episode_for_queue) {
            Ok(()) => toast_for_queue("Episode added to queue"),
            Err(error) => toast_for_queue(&error),
        }
    });
    actions.add_action(&queue_up);
    let star = gtk::gio::SimpleAction::new("toggle-star", None);
    let show_for_star = show.clone();
    let episode_for_star = episode.clone();
    let store_for_star = store.clone();
    let toast_for_star = toast.clone();
    star.connect_activate(move |_, _| {
        match store_for_star.toggle_starred(&show_for_star, &episode_for_star) {
            Ok(true) => toast_for_star("Episode starred"),
            Ok(false) => toast_for_star("Episode removed from Starred"),
            Err(error) => toast_for_star(&error),
        }
    });
    actions.add_action(&star);
    let mark = gtk::gio::SimpleAction::new("mark-played", None);
    let show_for_mark = show.clone();
    let episode_for_mark = episode.clone();
    let store_for_mark = store.clone();
    let playback_for_mark = playback.clone();
    let toast_for_mark = toast.clone();
    mark.connect_activate(move |_, _| {
        if store_for_mark.is_played(&show_for_mark.feed_url, &episode_for_mark.id) {
            match store_for_mark.mark_unplayed(&show_for_mark.feed_url, &episode_for_mark.id) {
                Ok(true) => toast_for_mark("Episode marked as unplayed"),
                Ok(false) => {},
                Err(error) => toast_for_mark(&error),
            }
        } else {
            if playback_for_mark.is_current(&show_for_mark.feed_url, &episode_for_mark.id) {
                if let Err(error) = playback_for_mark.stop() {
                    toast_for_mark(&error);
                    return;
                }
            }
            match store_for_mark.mark_played_and_reset(&show_for_mark.feed_url, &[episode_for_mark.id.clone()]) {
                Ok(_) => toast_for_mark("Episode marked as played"),
                Err(error) => toast_for_mark(&error),
            }
        }
    });
    actions.add_action(&mark);

    let download = gtk::gio::SimpleAction::new("download", None);
    download.set_enabled(episode.audio_url.is_some());
    let show_for_download = show.clone();
    let episode_for_download = episode.clone();
    let downloads_for_action = downloads.clone();
    let toast_for_download = toast.clone();
    download.connect_activate(move |_, _| {
        if let Err(error) = downloads_for_action.start(&show_for_download, &episode_for_download) {
            toast_for_download(&error);
        }
    });
    actions.add_action(&download);
    let cancel_download = gtk::gio::SimpleAction::new("cancel-download", None);
    let downloads_for_cancel = downloads.clone();
    let feed_for_cancel = show.feed_url.clone();
    let episode_for_cancel = episode.id.clone();
    cancel_download.connect_activate(move |_, _| {
        downloads_for_cancel.cancel(&feed_for_cancel, &episode_for_cancel);
    });
    actions.add_action(&cancel_download);
    let remove_download = gtk::gio::SimpleAction::new("remove-download", None);
    let show_for_remove = show.clone();
    let episode_for_remove = episode.clone();
    let store_for_remove = store.clone();
    let toast_for_remove = toast.clone();
    remove_download.connect_activate(move |_, _| {
        match store_for_remove.remove_download(&show_for_remove.feed_url, &episode_for_remove.id) {
            Ok(Some(path)) => match std::fs::remove_file(path) {
                Ok(()) => toast_for_remove("Download removed"),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => toast_for_remove("Download removed"),
                Err(error) => toast_for_remove(&format!("Could not delete downloaded file: {error}")),
            },
            Ok(None) => {},
            Err(error) => toast_for_remove(&error),
        }
    });
    actions.add_action(&remove_download);
    if let Some(go_to_show) = go_to_show {
        let open_show = gtk::gio::SimpleAction::new("go-to-show", None);
        let show = show.clone();
        open_show.connect_activate(move |_, _| go_to_show(show.clone()));
        actions.add_action(&open_show);
    }
    row.insert_action_group("episode", Some(&actions));
    let popover = gtk::PopoverMenu::from_model(Some(&model));
    popover.set_parent(row);
    let playback_for_menu = playback.clone();
    let feed_url = show.feed_url.clone();
    let episode_id = episode.id.clone();
    let store_for_menu = store.clone();
    let downloads_for_menu = downloads.clone();
    let right_click = gtk::GestureClick::new();
    right_click.set_button(3);
    right_click.set_propagation_phase(gtk::PropagationPhase::Capture);
    right_click.connect_pressed(move |gesture, _, x, y| {
        let current = playback_for_menu.is_current_active(&feed_url, &episode_id);
        let starred = store_for_menu.is_starred(&feed_url, &episode_id);
        let played = store_for_menu.is_played(&feed_url, &episode_id);
        let downloaded = store_for_menu.is_downloaded(&feed_url, &episode_id);
        let active_download = downloads_for_menu.is_active(&feed_url, &episode_id);
        populate_menu(&model, starred, played, downloaded, active_download, has_go_to_show, current);
        popover.set_pointing_to(Some(&gtk::gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
        popover.popup();
        gesture.set_state(gtk::EventSequenceState::Claimed);
    });
    row.add_controller(right_click);
}

fn populate_menu(model: &gtk::gio::Menu, starred: bool, played: bool, downloaded: bool, active_download: bool, has_go_to_show: bool, current: bool) {
    model.remove_all();
    if !current {
        let playback = gtk::gio::Menu::new();
        append_icon_item(&playback, "Play Now", "episode.play-now", "media-playback-start-symbolic");
        append_icon_item(&playback, "Queue Up", "episode.queue-up", "view-list-symbolic");
        model.append_section(None, &playback);
    }
    let collection = gtk::gio::Menu::new();
    append_icon_item(
        &collection,
        if played { "Mark as Unplayed" } else { "Mark as Played" },
        "episode.mark-played",
        if played { "media-playback-start-symbolic" } else { "object-select-symbolic" },
    );
    append_icon_item(
        &collection,
        if starred { "Remove from Starred" } else { "Star Episode" },
        "episode.toggle-star",
        if starred { "starred-symbolic" } else { "non-starred-symbolic" },
    );
    append_icon_item(
        &collection,
        if active_download { "Cancel Download" } else if downloaded { "Remove Download" } else { "Download" },
        if active_download { "episode.cancel-download" } else if downloaded { "episode.remove-download" } else { "episode.download" },
        if active_download { "process-stop-symbolic" } else if downloaded { "user-trash-symbolic" } else { "folder-download-symbolic" },
    );
    model.append_section(None, &collection);
    if has_go_to_show {
        let show = gtk::gio::Menu::new();
        append_icon_item(&show, "Go to Show", "episode.go-to-show", "audio-input-microphone-symbolic");
        model.append_section(None, &show);
    }
}

fn append_icon_item(menu: &gtk::gio::Menu, label: &str, action: &str, icon: &str) {
    let item = gtk::gio::MenuItem::new(Some(label), Some(action));
    item.set_icon(&gtk::gio::ThemedIcon::new(icon));
    menu.append_item(&item);
}

fn update_play_icon(
    icon: &gtk::Image,
    button: &gtk::Button,
    playback: &PlaybackController,
    feed_url: &str,
    episode_id: &str,
    played: bool,
) {
    let active = playback.is_current(feed_url, episode_id) && playback.is_playing();
    icon.set_icon_name(Some(if active {
        "media-playback-pause-symbolic"
    } else if played {
        "view-refresh-symbolic"
    } else {
        "media-playback-start-symbolic"
    }));
    button.set_tooltip_text(Some(if active {
        "Pause episode"
    } else if played {
        "Replay episode from the beginning"
    } else {
        "Play episode"
    }));
}
