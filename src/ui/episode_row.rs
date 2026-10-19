use adw::prelude::*;
use std::cell::Cell;
use std::rc::Rc;

use super::episode::{format_date, plain_description};
use crate::discovery_service::{Episode, Show};
use crate::playback::PlaybackController;

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
    playback: &Rc<PlaybackController>,
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
    attach_context_menu(&row, show, episode, playback, toast, go_to_show);
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
    playback: &Rc<PlaybackController>,
    toast: &Rc<dyn Fn(&str)>,
    go_to_show: Option<Rc<dyn Fn(Show)>>,
) {
    let model = gtk::gio::Menu::new();
    model.append(Some("Play Now"), Some("episode.play-now"));
    model.append(Some("Queue Up"), Some("episode.queue-up"));
    if go_to_show.is_some() {
        let more = gtk::gio::Menu::new();
        more.append(Some("Go to Show"), Some("episode.go-to-show"));
        model.append_section(None, &more);
    }
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
    let right_click = gtk::GestureClick::new();
    right_click.set_button(3);
    right_click.set_propagation_phase(gtk::PropagationPhase::Capture);
    right_click.connect_pressed(move |gesture, _, x, y| {
        if playback_for_menu.is_current_active(&feed_url, &episode_id) {
            gesture.set_state(gtk::EventSequenceState::Claimed);
            return;
        }
        popover.set_pointing_to(Some(&gtk::gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
        popover.popup();
        gesture.set_state(gtk::EventSequenceState::Claimed);
    });
    row.add_controller(right_click);
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
