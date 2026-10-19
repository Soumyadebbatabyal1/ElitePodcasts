use adw::prelude::*;
use std::rc::Rc;

use crate::discovery_service::{Episode, Show};
use crate::playback::PlaybackController;

pub(super) fn show_dialog(
    parent: &gtk::ListBoxRow,
    show: &Show,
    episode: &Episode,
    playback: Rc<PlaybackController>,
    toast: Rc<dyn Fn(&str)>,
) {
    let body = gtk::Box::new(gtk::Orientation::Vertical, 16);
    body.set_margin_top(20);
    body.set_margin_bottom(24);
    body.set_margin_start(24);
    body.set_margin_end(24);
    let heading_text = gtk::Box::new(gtk::Orientation::Vertical, 6);
    let title = gtk::Label::new(Some(&episode.title));
    title.set_xalign(0.0);
    title.set_wrap(true);
    title.add_css_class("title-3");
    heading_text.append(&title);
    let mut details = format_date(&episode.date);
    if let Some(seconds) = episode.duration_seconds {
        details.push_str(&format!(" · {} min", (seconds + 30) / 60));
    }
    let subtitle = gtk::Label::new(Some(&details));
    subtitle.set_xalign(0.0);
    subtitle.add_css_class("dim-label");
    heading_text.append(&subtitle);
    body.append(&heading_text);
    let description = plain_description(&episode.description);
    let summary = gtk::Label::new(Some(if description.is_empty() {
        "No episode description is available."
    } else {
        &description
    }));
    summary.set_xalign(0.0);
    summary.set_yalign(0.0);
    summary.set_wrap(true);
    body.append(&summary);
    let play = gtk::Button::with_label("Play episode");
    play.add_css_class("episode-play-button");
    play.set_halign(gtk::Align::Start);
    play.set_sensitive(episode.audio_url.is_some());
    body.append(&play);
    let scroller = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .child(&body)
        .build();
    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&adw::HeaderBar::new());
    toolbar.set_content(Some(&scroller));
    let dialog = adw::Dialog::builder()
        .title("Episode details")
        .content_width(500)
        .content_height(420)
        .child(&toolbar)
        .build();
    let show = show.clone();
    let episode = episode.clone();
    let dialog_for_play = dialog.clone();
    play.connect_clicked(move |_| {
        match playback.play_or_toggle(&show, &episode) {
            Ok(()) => {
                dialog_for_play.close();
            }
            Err(error) => toast(&error),
        }
    });
    dialog.present(Some(parent));
}

pub(super) fn parse_date(raw: &str) -> Option<gtk::glib::DateTime> {
    let raw = raw.trim();
    if let Ok(date) = gtk::glib::DateTime::from_iso8601(raw, None) {
        return date.to_local().ok();
    }
    let parts: Vec<&str> = raw
        .trim_start_matches(|character: char| character.is_ascii_alphabetic() || character == ',')
        .split_whitespace()
        .collect();
    if parts.len() < 4 {
        return None;
    }
    let month = match parts[1].to_ascii_lowercase().as_str() {
        "jan" => 1, "feb" => 2, "mar" => 3, "apr" => 4, "may" => 5, "jun" => 6,
        "jul" => 7, "aug" => 8, "sep" => 9, "oct" => 10, "nov" => 11, "dec" => 12,
        _ => return None,
    };
    let day = parts[0].parse::<u8>().ok()?;
    let year = parts[2].parse::<i32>().ok()?;
    let zone = parts.get(4).copied().unwrap_or("+0000");
    let offset = if zone.len() == 5 && (zone.starts_with('+') || zone.starts_with('-')) {
        format!("{}:{}", &zone[..3], &zone[3..])
    } else {
        "+00:00".to_string()
    };
    let iso = format!("{year:04}-{month:02}-{day:02}T{}{offset}", parts[3]);
    gtk::glib::DateTime::from_iso8601(&iso, None).ok()?.to_local().ok()
}

pub(super) fn short_date(published: &gtk::glib::DateTime, now: &gtk::glib::DateTime) -> String {
    let pattern = if published.year() == now.year() { "%e %b" } else { "%e %b %Y" };
    published.format(pattern).map(|value| value.trim().to_string()).unwrap_or_default()
}

pub(super) fn format_date(raw: &str) -> String {
    let Some(published) = parse_date(raw) else {
        return raw.trim().to_string();
    };
    let Some(now) = gtk::glib::DateTime::now_local().ok() else {
        return raw.trim().to_string();
    };
    let age = now.to_unix() - published.to_unix();
    if (0..3600).contains(&age) {
        format!("{} min ago", (age / 60).max(1))
    } else if (0..86400).contains(&age) {
        format!("{} hr ago", age / 3600)
    } else if (86400..172800).contains(&age) {
        "Yesterday".to_string()
    } else if (0..604800).contains(&age) {
        format!("{} days ago", age / 86400)
    } else {
        short_date(&published, &now)
    }
}

pub(super) fn plain_description(raw: &str) -> String {
    let decoded = raw
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'");
    let mut text = String::new();
    let mut in_tag = false;
    for character in decoded.chars() {
        match character {
            '<' => {
                in_tag = true;
                text.push(' ');
            }
            '>' if in_tag => in_tag = false,
            _ if !in_tag => text.push(character),
            _ => {}
        }
    }
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}
