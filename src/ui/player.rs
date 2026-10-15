use adw::prelude::*;
use gtk::glib::value::ToValue;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use super::artwork::ArtworkLoader;
use crate::playback::PlaybackController;

pub(super) fn bar(
    playback: Rc<PlaybackController>,
    artwork: Rc<ArtworkLoader>,
    toast: Rc<dyn Fn(&str)>,
) -> gtk::Widget {
    let bar = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    bar.add_css_class("player-bar");
    bar.set_margin_start(12);
    bar.set_margin_end(12);
    bar.set_margin_top(10);
    bar.set_margin_bottom(10);
    bar.set_size_request(-1, 64);
    bar.set_visible(false);

    let rewind = gtk::Button::from_icon_name("skip-backwards-10-symbolic");
    rewind.add_css_class("flat");
    rewind.add_css_class("player-skip-button");
    rewind.set_tooltip_text(Some("Back 10 seconds"));
    rewind.set_size_request(34, 34);
    rewind.set_valign(gtk::Align::Center);
    let playback_for_rewind = playback.clone();
    let toast_for_rewind = toast.clone();
    rewind.connect_clicked(move |_| {
        if let Err(error) = playback_for_rewind.seek_relative(-10) {
            toast_for_rewind(&error);
        }
    });
    bar.append(&rewind);

    let play = gtk::Button::from_icon_name("media-playback-start-symbolic");
    play.add_css_class("player-play-button");
    play.set_tooltip_text(Some("Pause"));
    play.set_size_request(64, 38);
    play.set_valign(gtk::Align::Center);
    let playback_for_play = playback.clone();
    let toast_for_play = toast.clone();
    play.connect_clicked(move |_| {
        if let Err(error) = playback_for_play.toggle() {
            toast_for_play(&error);
        }
    });
    bar.append(&play);

    let forward = gtk::Button::from_icon_name("skip-forward-10-symbolic");
    forward.add_css_class("flat");
    forward.add_css_class("player-skip-button");
    forward.set_tooltip_text(Some("Forward 10 seconds"));
    forward.set_size_request(34, 34);
    forward.set_valign(gtk::Align::Center);
    let playback_for_forward = playback.clone();
    let toast_for_forward = toast.clone();
    forward.connect_clicked(move |_| {
        if let Err(error) = playback_for_forward.seek_relative(10) {
            toast_for_forward(&error);
        }
    });
    bar.append(&forward);

    let cover = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    cover.set_size_request(36, 36);
    cover.set_margin_start(6);
    cover.set_valign(gtk::Align::Center);
    bar.append(&cover);

    let metadata = gtk::Box::new(gtk::Orientation::Vertical, 0);
    metadata.set_valign(gtk::Align::Center);
    metadata.set_size_request(170, -1);
    let show_title = gtk::Label::new(None);
    show_title.set_xalign(0.0);
    show_title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    show_title.add_css_class("caption");
    show_title.add_css_class("dim-label");
    metadata.append(&show_title);
    let episode_title = gtk::Label::new(None);
    episode_title.set_xalign(0.0);
    episode_title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    episode_title.add_css_class("caption");
    episode_title.add_css_class("player-episode-title");
    metadata.append(&episode_title);
    bar.append(&metadata);

    let seek = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 1.0, 1.0);
    seek.set_draw_value(false);
    seek.set_hexpand(true);
    seek.set_size_request(120, -1);
    seek.set_tooltip_text(Some("Seek through episode"));
    let updating_seek = Rc::new(Cell::new(false));
    let playback_for_seek = playback.clone();
    let toast_for_seek = toast.clone();
    let updating_for_seek = updating_seek.clone();
    seek.connect_value_changed(move |scale| {
        if !updating_for_seek.get() {
            if let Err(error) = playback_for_seek.seek_to(scale.value().round() as u64) {
                toast_for_seek(&error);
            }
        }
    });
    bar.append(&seek);

    let time = gtk::Label::new(Some("00:00 / 00:00"));
    time.add_css_class("caption");
    time.add_css_class("dim-label");
    bar.append(&time);

    let speed = gtk::MenuButton::new();
    speed.add_css_class("player-speed-button");
    speed.set_valign(gtk::Align::Center);
    speed.set_tooltip_text(Some("Playback speed"));
    let speed_content = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    let speed_icon = gtk::Image::from_icon_name("speedometer-symbolic");
    speed_icon.set_pixel_size(12);
    speed_icon.add_css_class("player-speed-icon");
    speed_content.append(&speed_icon);
    let speed_label = gtk::Label::new(Some("1.00×"));
    speed_label.add_css_class("caption");
    speed_label.add_css_class("player-speed-label");
    speed_content.append(&speed_label);
    speed.set_child(Some(&speed_content));
    let speed_popover = gtk::Popover::new();
    speed_popover.set_position(gtk::PositionType::Top);
    let choices = gtk::Box::new(gtk::Orientation::Vertical, 2);
    choices.set_margin_top(6);
    choices.set_margin_bottom(6);
    choices.set_margin_start(6);
    choices.set_margin_end(6);
    for rate in [0.75, 1.0, 1.25, 1.5, 1.75, 2.0] {
        let choice = gtk::Button::with_label(&format!("{rate:.2}×"));
        choice.add_css_class("flat");
        let playback = playback.clone();
        let popover = speed_popover.clone();
        let toast = toast.clone();
        choice.connect_clicked(move |_| {
            if let Err(error) = playback.set_rate(rate) {
                toast(&error);
            }
            popover.popdown();
        });
        choices.append(&choice);
    }
    speed_popover.set_child(Some(&choices));
    speed.set_popover(Some(&speed_popover));
    speed.set_direction(gtk::ArrowType::Up);
    bar.append(&speed);

    let current_key = Rc::new(RefCell::new(String::new()));
    let weak_bar = bar.downgrade();
    let weak_cover = cover.downgrade();
    let weak_play = play.downgrade();
    let weak_show = show_title.downgrade();
    let weak_episode = episode_title.downgrade();
    let weak_seek = seek.downgrade();
    let weak_time = time.downgrade();
    let weak_speed = speed_label.downgrade();
    let playback_for_update = Rc::downgrade(&playback);
    playback.watch(move || {
        let Some(playback_for_update) = playback_for_update.upgrade() else {
            return false;
        };
        let (Some(bar), Some(cover), Some(play), Some(show_title), Some(episode_title),
            Some(seek), Some(time), Some(speed)) = (
            weak_bar.upgrade(), weak_cover.upgrade(), weak_play.upgrade(),
            weak_show.upgrade(), weak_episode.upgrade(), weak_seek.upgrade(),
            weak_time.upgrade(), weak_speed.upgrade(),
        ) else {
            return false;
        };
        let Some(item) = playback_for_update.current() else {
            bar.set_visible(false);
            return true;
        };
        bar.set_visible(true);
        let key = format!("{}\u{1f}{}", item.feed_url, item.episode.id);
        if *current_key.borrow() != key {
            *current_key.borrow_mut() = key;
            while let Some(child) = cover.first_child() {
                cover.remove(&child);
            }
            let cover_image = artwork.cached_widget(&item.artwork_urls, 36);
            cover_image.add_css_class("player-artwork");
            cover_image.set_valign(gtk::Align::Center);
            cover.append(&cover_image);
            show_title.set_text(&item.show_title);
            episode_title.set_text(&item.episode.title);
            episode_title.set_tooltip_text(Some(&item.episode.title));
        }
        play.set_icon_name(if playback_for_update.is_playing() {
            "media-playback-pause-symbolic"
        } else {
            "media-playback-start-symbolic"
        });
        play.set_tooltip_text(Some(if playback_for_update.is_playing() { "Pause" } else { "Play" }));
        let duration = playback_for_update.duration();
        let position = playback_for_update.position();
        updating_seek.set(true);
        seek.set_range(0.0, duration.max(1) as f64);
        seek.set_sensitive(duration > 0);
        seek.set_value(position.min(duration.max(1)) as f64);
        updating_seek.set(false);
        time.set_text(&format!("{} / {}", clock_text(position), clock_text(duration)));
        speed.set_text(&format!("{:.2}×", playback_for_update.rate()));
        true
    });

    let compact = adw::BreakpointBin::new();
    compact.set_child(Some(&bar));
    let narrow = adw::Breakpoint::new(adw::BreakpointCondition::new_length(
        adw::BreakpointConditionLengthType::MaxWidth,
        800.0,
        adw::LengthUnit::Sp,
    ));
    narrow.add_setter(&cover, "visible", Some(&false.to_value()));
    narrow.add_setter(&metadata, "visible", Some(&false.to_value()));
    compact.add_breakpoint(narrow);
    compact.upcast()
}

fn clock_text(seconds: u64) -> String {
    if seconds >= 3600 {
        format!("{}:{:02}:{:02}", seconds / 3600, seconds / 60 % 60, seconds % 60)
    } else {
        format!("{:02}:{:02}", seconds / 60, seconds % 60)
    }
}
