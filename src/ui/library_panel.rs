use adw::prelude::*;
use std::cell::Cell;
use std::rc::Rc;

use super::artwork::ArtworkLoader;
use crate::discovery_service::{Episode, Show};
use crate::playback::PlaybackController;

pub(super) fn panel(
    playback: Rc<PlaybackController>,
    artwork: Rc<ArtworkLoader>,
    toast: Rc<dyn Fn(&str)>,
    close: Rc<dyn Fn()>,
) -> gtk::Widget {
    let stack = adw::ViewStack::new();
    let queue_content = gtk::Box::new(gtk::Orientation::Vertical, 8);
    queue_content.set_margin_top(12);
    queue_content.set_margin_bottom(20);
    queue_content.set_margin_start(12);
    queue_content.set_margin_end(12);
    let history_content = gtk::Box::new(gtk::Orientation::Vertical, 8);
    history_content.set_margin_top(12);
    history_content.set_margin_bottom(20);
    history_content.set_margin_start(12);
    history_content.set_margin_end(12);
    let queue_scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .child(&queue_content)
        .build();
    let history_scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .child(&history_content)
        .build();
    stack.add_titled_with_icon(&queue_scroll, Some("queue"), "Queue", "view-list-symbolic");
    stack.add_titled_with_icon(&history_scroll, Some("history"), "History", "document-open-recent-symbolic");

    let switcher = adw::ViewSwitcher::new();
    switcher.set_policy(adw::ViewSwitcherPolicy::Wide);
    switcher.set_stack(Some(&stack));
    let header = adw::HeaderBar::new();
    header.set_show_start_title_buttons(false);
    header.set_show_end_title_buttons(false);
    header.set_title_widget(Some(&switcher));
    let close_button = gtk::Button::from_icon_name("window-close-symbolic");
    close_button.add_css_class("circular");
    close_button.set_size_request(32, 32);
    close_button.set_tooltip_text(Some("Close Queue and History"));
    close_button.connect_clicked(move |_| close());
    header.pack_end(&close_button);

    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.set_content(Some(&stack));

    let seen_revision = Rc::new(Cell::new(u64::MAX));
    let weak_queue = queue_content.downgrade();
    let weak_history = history_content.downgrade();
    let weak_playback = Rc::downgrade(&playback);
    let update: Rc<dyn Fn()> = Rc::new(move || {
        let (Some(queue_content), Some(history_content), Some(playback)) = (
            weak_queue.upgrade(), weak_history.upgrade(), weak_playback.upgrade(),
        ) else {
            return;
        };
        if seen_revision.get() == playback.library_revision() {
            return;
        }
        seen_revision.set(playback.library_revision());
        render_queue(&queue_content, &playback, &artwork, &toast);
        render_history(&history_content, &playback, &artwork, &toast);
    });
    update();
    let update_on_change = update.clone();
    playback.watch(move || {
        update_on_change();
        true
    });
    toolbar.upcast()
}

fn clear(content: &gtk::Box) {
    while let Some(child) = content.first_child() {
        content.remove(&child);
    }
}

fn render_queue(
    content: &gtk::Box,
    playback: &Rc<PlaybackController>,
    artwork: &Rc<ArtworkLoader>,
    toast: &Rc<dyn Fn(&str)>,
) {
    clear(content);
    let items = playback.queue();
    if items.is_empty() {
        content.append(&status("Your queue is empty", "Right-click an episode and choose Queue Up."));
        return;
    }
    let clear_button = gtk::Button::with_label("Clear Queue");
    clear_button.add_css_class("flat");
    clear_button.set_halign(gtk::Align::End);
    let playback_for_clear = playback.clone();
    clear_button.connect_clicked(move |_| playback_for_clear.clear_queue());
    content.append(&clear_button);
    let list = gtk::ListBox::new();
    list.add_css_class("boxed-list");
    list.set_selection_mode(gtk::SelectionMode::None);
    for (index, item) in items.iter().enumerate() {
        let row = entry_row(&item.show, &item.episode, artwork, None);
        let play = gtk::Button::from_icon_name("media-playback-start-symbolic");
        style_icon_action(&play);
        play.set_tooltip_text(Some("Play now"));
        let playback_for_play = playback.clone();
        let toast_for_play = toast.clone();
        play.connect_clicked(move |_| {
            if let Err(error) = playback_for_play.play_queued(index) {
                toast_for_play(&error);
            }
        });
        row.append(&play);
        let remove = gtk::Button::from_icon_name("list-remove-symbolic");
        style_icon_action(&remove);
        remove.set_tooltip_text(Some("Remove from queue"));
        let playback_for_remove = playback.clone();
        remove.connect_clicked(move |_| playback_for_remove.remove_queued(index));
        row.append(&remove);
        list.append(&row);
    }
    content.append(&list);
}

fn render_history(
    content: &gtk::Box,
    playback: &Rc<PlaybackController>,
    artwork: &Rc<ArtworkLoader>,
    toast: &Rc<dyn Fn(&str)>,
) {
    clear(content);
    let items = match playback.history() {
        Ok(items) => items,
        Err(error) => {
            content.append(&status("Could not load history", &error));
            return;
        }
    };
    if items.is_empty() {
        content.append(&status("No completed episodes", "Episodes appear here after they finish playing."));
        return;
    }
    let clear_button = gtk::Button::with_label("Clear History");
    clear_button.add_css_class("flat");
    clear_button.set_halign(gtk::Align::End);
    let playback_for_clear = playback.clone();
    let toast_for_clear = toast.clone();
    clear_button.connect_clicked(move |_| {
        match playback_for_clear.clear_history() {
            Ok(()) => toast_for_clear("History cleared"),
            Err(error) => toast_for_clear(&error),
        }
    });
    content.append(&clear_button);
    let list = gtk::ListBox::new();
    list.add_css_class("boxed-list");
    list.set_selection_mode(gtk::SelectionMode::None);
    for item in items {
        let completed = gtk::glib::DateTime::from_unix_local(item.completed_at)
            .ok().and_then(|date| date.format("%e %b %Y").ok())
            .map(|date| date.trim().to_string());
        let row = entry_row(&item.show, &item.episode, artwork, completed.as_deref());
        let play = gtk::Button::from_icon_name("media-playback-start-symbolic");
        style_icon_action(&play);
        play.set_tooltip_text(Some("Play again"));
        let show = item.show.clone();
        let episode = item.episode.clone();
        let playback_for_play = playback.clone();
        let toast_for_play = toast.clone();
        play.connect_clicked(move |_| {
            if let Err(error) = playback_for_play.play_episode(&show, &episode) {
                toast_for_play(&error);
            }
        });
        row.append(&play);
        list.append(&row);
    }
    content.append(&list);
}

fn status(title: &str, description: &str) -> adw::StatusPage {
    adw::StatusPage::builder()
        .icon_name("view-list-symbolic")
        .title(title)
        .description(description)
        .build()
}

fn style_icon_action(button: &gtk::Button) {
    button.add_css_class("flat");
    button.add_css_class("circular");
    button.add_css_class("library-icon-action");
    button.set_size_request(34, 34);
    button.set_valign(gtk::Align::Center);
}

fn entry_row(show: &Show, episode: &Episode, artwork: &ArtworkLoader, date: Option<&str>) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    row.set_margin_top(8);
    row.set_margin_bottom(8);
    row.set_margin_start(8);
    row.set_margin_end(8);
    let mut candidates = Vec::new();
    if let Some(url) = &episode.artwork_url {
        candidates.push(url.clone());
    }
    candidates.extend(show.artwork_urls.iter().cloned());
    row.append(&artwork.cached_widget(&candidates, 42));
    let labels = gtk::Box::new(gtk::Orientation::Vertical, 2);
    labels.set_hexpand(true);
    labels.set_valign(gtk::Align::Center);
    let title = gtk::Label::new(Some(&episode.title));
    title.set_xalign(0.0);
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    title.set_tooltip_text(Some(&episode.title));
    labels.append(&title);
    let subtitle_text = date.map(|date| format!("{} · {date}", show.title))
        .unwrap_or_else(|| show.title.clone());
    let subtitle = gtk::Label::new(Some(&subtitle_text));
    subtitle.set_xalign(0.0);
    subtitle.set_ellipsize(gtk::pango::EllipsizeMode::End);
    subtitle.add_css_class("caption");
    subtitle.add_css_class("dim-label");
    labels.append(&subtitle);
    row.append(&labels);
    row
}
