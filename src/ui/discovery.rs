use adw::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::mpsc;
use std::time::Duration;

use crate::data::CATEGORIES;
use crate::discovery_service::{self, FeedDetails, Show};
use super::artwork::ArtworkLoader;

#[derive(Clone)]
enum Request {
    Search(String),
    Category(String),
}

pub(super) struct DiscoveryPage {
    pub widget: gtk::Widget,
    pub reset: Rc<dyn Fn()>,
}

pub(super) fn page(entry: &gtk::SearchEntry, back: &gtk::Button, status: Rc<dyn Fn(&str, bool)>) -> DiscoveryPage {
    let view = gtk::Stack::new();
    view.set_vexpand(true);
    let artwork = ArtworkLoader::new();

    let results = gtk::Box::new(gtk::Orientation::Vertical, 0);
    results.set_margin_top(24);
    results.set_margin_bottom(40);
    let listing = gtk::Box::new(gtk::Orientation::Vertical, 0);
    listing.set_visible(false);
    results.append(&listing);
    let results_clamp = adw::Clamp::builder().maximum_size(660).build();
    results_clamp.set_margin_start(24);
    results_clamp.set_margin_end(24);
    results_clamp.set_child(Some(&results));
    let results_scroller = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .child(&results_clamp)
        .build();
    view.add_named(&results_scroller, Some("results"));

    let feed_content = gtk::Box::new(gtk::Orientation::Vertical, 18);
    feed_content.set_margin_top(52);
    feed_content.set_margin_bottom(40);
    feed_content.set_visible(false);
    let feed_artwork = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    feed_artwork.set_halign(gtk::Align::Center);
    feed_artwork.set_margin_bottom(12);
    feed_content.append(&feed_artwork);
    let feed_description = gtk::Label::new(None);
    feed_description.set_halign(gtk::Align::Center);
    feed_description.set_justify(gtk::Justification::Center);
    feed_description.set_wrap(true);
    feed_description.set_lines(4);
    feed_description.set_ellipsize(gtk::pango::EllipsizeMode::End);
    feed_description.set_visible(false);
    feed_content.append(&feed_description);
    let read_more = gtk::Button::with_label("Read More");
    read_more.set_halign(gtk::Align::Center);
    read_more.set_visible(false);
    feed_content.append(&read_more);
    let description_expanded = Rc::new(Cell::new(false));
    let description_for_button = feed_description.clone();
    let expanded_for_button = description_expanded.clone();
    read_more.connect_clicked(move |button| {
        let expanded = !expanded_for_button.get();
        expanded_for_button.set(expanded);
        description_for_button.set_lines(if expanded { -1 } else { 4 });
        description_for_button.set_ellipsize(if expanded {
            gtk::pango::EllipsizeMode::None
        } else {
            gtk::pango::EllipsizeMode::End
        });
        button.set_label(if expanded { "Show Less" } else { "Read More" });
    });
    let episodes = gtk::Box::new(gtk::Orientation::Vertical, 0);
    episodes.set_margin_top(16);
    feed_content.append(&episodes);
    let feed_clamp = adw::Clamp::builder().maximum_size(580).build();
    feed_clamp.set_margin_start(24);
    feed_clamp.set_margin_end(24);
    feed_clamp.set_child(Some(&feed_content));
    let feed_scroller = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .child(&feed_clamp)
        .build();
    view.add_named(&feed_scroller, Some("feed"));

    let generation = Rc::new(Cell::new(0u64));
    let result_title = Rc::new(RefCell::new(String::from("Search results")));
    let view_for_back = view.clone();
    let generation_for_back = generation.clone();
    let status_for_back = status.clone();
    let result_title_for_back = result_title.clone();
    let back_for_click = back.clone();
    back.connect_clicked(move |_| {
        generation_for_back.set(generation_for_back.get().wrapping_add(1));
        view_for_back.set_transition_type(gtk::StackTransitionType::SlideRight);
        if view_for_back.visible_child_name().as_deref() == Some("feed") {
            view_for_back.set_visible_child_name("results");
            status_for_back(&result_title_for_back.borrow(), false);
            back_for_click.set_tooltip_text(Some("Back to Discover"));
        } else {
            view_for_back.set_visible_child_name("home");
            status_for_back("Discover", false);
            back_for_click.set_visible(false);
        }
    });
    let open_feed: Rc<dyn Fn(Show)> = {
        let view = view.clone();
        let generation = generation.clone();
        let status = status.clone();
        let artwork = artwork.clone();
        let back = back.clone();
        let feed_content = feed_content.clone();
        let read_more = read_more.clone();
        let description_expanded = description_expanded.clone();
        Rc::new(move |show| {
            status(&show.title, true);
            feed_content.set_visible(false);
            clear(&feed_artwork);
            feed_description.set_text("");
            feed_description.set_visible(false);
            feed_description.set_lines(4);
            feed_description.set_ellipsize(gtk::pango::EllipsizeMode::End);
            description_expanded.set(false);
            read_more.set_label("Read More");
            read_more.set_visible(false);
            clear(&episodes);
            view.set_transition_type(gtk::StackTransitionType::SlideLeft);
            view.set_visible_child_name("feed");
            back.set_visible(true);
            back.set_tooltip_text(Some("Back to results"));

            let id = generation.get().wrapping_add(1);
            generation.set(id);
            let (tx, rx) = mpsc::channel();
            let fallback_title = show.title.clone();
            let fallback_artwork = show.artwork_urls.clone();
            std::thread::spawn(move || {
                let _ = tx.send(discovery_service::feed(&show.feed_url));
            });
            let generation = generation.clone();
            let episodes = episodes.clone();
            let feed_description = feed_description.clone();
            let feed_artwork = feed_artwork.clone();
            let feed_content = feed_content.clone();
            let read_more = read_more.clone();
            let status = status.clone();
            let artwork = artwork.clone();
            gtk::glib::timeout_add_local(Duration::from_millis(100), move || {
                if generation.get() != id {
                    return gtk::glib::ControlFlow::Break;
                }
                match rx.try_recv() {
                    Ok(answer) => {
                        match answer {
                            Ok(feed) => {
                                let title = if feed.title.is_empty() {
                                    fallback_title.clone()
                                } else {
                                    feed.title.clone()
                                };
                                let mut covers: Vec<String> = feed
                                    .artwork_url
                                    .clone()
                                    .filter(|url| !url.trim().is_empty())
                                    .into_iter()
                                    .collect();
                                covers.extend(fallback_artwork.clone());
                                let generation = generation.clone();
                                let feed_artwork = feed_artwork.clone();
                                let feed_description = feed_description.clone();
                                let feed_content = feed_content.clone();
                                let episodes = episodes.clone();
                                let read_more = read_more.clone();
                                let artwork_for_feed = artwork.clone();
                                let status = status.clone();
                                artwork.preload(vec![covers.clone()], move || {
                                    if generation.get() != id {
                                        return;
                                    }
                                    feed_artwork.append(&artwork_for_feed.cached_widget(&covers, 220));
                                    show_feed(&feed_description, &read_more, &episodes, feed);
                                    feed_content.set_visible(true);
                                    status(&title, false);
                                });
                            }
                            Err(message) => {
                                status(&fallback_title, false);
                                show_message(&episodes, "Could not load RSS feed", &message);
                                feed_content.set_visible(true);
                            }
                        }
                        gtk::glib::ControlFlow::Break
                    }
                    Err(mpsc::TryRecvError::Empty) => gtk::glib::ControlFlow::Continue,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        status(&fallback_title, false);
                        show_message(&episodes, "Could not load RSS feed", "The request ended unexpectedly.");
                        feed_content.set_visible(true);
                        gtk::glib::ControlFlow::Break
                    }
                }
            });
        })
    };
    let request: Rc<dyn Fn(Request)> = {
        let view = view.clone();
        let listing = listing.clone();
        let generation = generation.clone();
        let open_feed = open_feed.clone();
        let artwork = artwork.clone();
        let status = status.clone();
        let result_title = result_title.clone();
        let back = back.clone();
        Rc::new(move |request| {
            let cue_first = matches!(&request, Request::Search(_));
            let title = match &request {
                Request::Search(_) => "Search results".to_string(),
                Request::Category(name) => name.clone(),
            };
            *result_title.borrow_mut() = title.clone();
            status(&title, true);
            clear(&listing);
            listing.set_visible(false);
            view.set_transition_type(gtk::StackTransitionType::SlideLeft);
            view.set_visible_child_name("results");
            back.set_visible(true);
            back.set_tooltip_text(Some("Back to Discover"));

            let id = generation.get().wrapping_add(1);
            generation.set(id);
            let (tx, rx) = mpsc::channel();
            std::thread::spawn(move || {
                let answer = match request {
                    Request::Search(term) => discovery_service::search(&term),
                    Request::Category(name) => discovery_service::category(&name),
                };
                let _ = tx.send(answer);
            });
            let listing = listing.clone();
            let generation = generation.clone();
            let open_feed = open_feed.clone();
            let artwork = artwork.clone();
            let status = status.clone();
            gtk::glib::timeout_add_local(Duration::from_millis(100), move || {
                if generation.get() != id {
                    return gtk::glib::ControlFlow::Break;
                }
                match rx.try_recv() {
                    Ok(answer) => {
                        match answer {
                            Ok(shows) => {
                                let covers = shows.iter().map(|show| show.artwork_urls.clone()).collect();
                                let listing = listing.clone();
                                let artwork_for_results = artwork.clone();
                                let open_feed = open_feed.clone();
                                let status = status.clone();
                                let generation = generation.clone();
                                let title = title.clone();
                                artwork.preload(covers, move || {
                                    if generation.get() != id {
                                        return;
                                    }
                                    clear(&listing);
                                    show_results(&listing, shows, open_feed, &artwork_for_results, cue_first);
                                    listing.set_visible(true);
                                    status(&title, false);
                                });
                            }
                            Err(message) => {
                                clear(&listing);
                                status(&title, false);
                                show_message(&listing, "Could not load podcasts", &message);
                                listing.set_visible(true);
                            }
                        }
                        gtk::glib::ControlFlow::Break
                    }
                    Err(mpsc::TryRecvError::Empty) => gtk::glib::ControlFlow::Continue,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        clear(&listing);
                        status(&title, false);
                        show_message(&listing, "Could not load podcasts", "The request ended unexpectedly.");
                        listing.set_visible(true);
                        gtk::glib::ControlFlow::Break
                    }
                }
            });
        })
    };

    let home = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .build();
    let content = gtk::Box::new(gtk::Orientation::Vertical, 30);
    content.set_margin_top(24);
    content.set_margin_bottom(40);
    content.append(&categories(request.clone()));

    let clamp = adw::Clamp::builder().maximum_size(930).build();
    clamp.set_margin_start(24);
    clamp.set_margin_end(24);
    clamp.set_child(Some(&content));
    home.set_child(Some(&clamp));
    view.add_named(&home, Some("home"));
    view.set_visible_child_name("home");

    let search_request = request.clone();
    entry.connect_activate(move |entry| {
        let term = entry.text().trim().to_string();
        if !term.is_empty() {
            search_request(Request::Search(term));
        }
    });
    let view_for_clear = view.clone();
    let generation_for_clear = generation.clone();
    let status_for_clear = status.clone();
    let back_for_clear = back.clone();
    entry.connect_search_changed(move |entry| {
        if entry.text().trim().is_empty() {
            generation_for_clear.set(generation_for_clear.get().wrapping_add(1));
            view_for_clear.set_transition_type(gtk::StackTransitionType::SlideRight);
            view_for_clear.set_visible_child_name("home");
            status_for_clear("Discover", false);
            back_for_clear.set_visible(false);
        }
    });
    let reset_view = view.clone();
    let reset_generation = generation.clone();
    let reset_status = status.clone();
    let reset_back = back.clone();
    let reset: Rc<dyn Fn()> = Rc::new(move || {
        reset_generation.set(reset_generation.get().wrapping_add(1));
        reset_view.set_transition_type(gtk::StackTransitionType::SlideRight);
        reset_view.set_visible_child_name("home");
        reset_status("Discover", false);
        reset_back.set_visible(false);
    });
    DiscoveryPage { widget: view.upcast(), reset }
}

fn clear(container: &gtk::Box) {
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }
}

fn show_message(container: &gtk::Box, title: &str, message: &str) {
    let content = gtk::Box::new(gtk::Orientation::Vertical, 8);
    content.set_halign(gtk::Align::Center);
    content.set_margin_top(28);
    let heading = gtk::Label::new(Some(title));
    heading.add_css_class("heading");
    content.append(&heading);
    let detail = gtk::Label::new(Some(message));
    detail.set_justify(gtk::Justification::Center);
    detail.set_wrap(true);
    detail.set_max_width_chars(56);
    detail.add_css_class("dim-label");
    content.append(&detail);
    container.append(&content);
}

fn show_results(
    container: &gtk::Box,
    shows: Vec<Show>,
    open_feed: Rc<dyn Fn(Show)>,
    artwork: &ArtworkLoader,
    cue_first: bool,
) {
    if shows.is_empty() {
        show_message(container, "No podcasts found", "No matching podcasts were found.");
        return;
    }
    let list = gtk::ListBox::new();
    list.add_css_class("boxed-list");
    list.set_selection_mode(gtk::SelectionMode::None);
    list.set_activate_on_single_click(true);
    let mut add_areas = Vec::with_capacity(shows.len());
    for (index, show) in shows.iter().enumerate() {
        let row = gtk::ListBoxRow::new();
        row.set_activatable(true);
        let over_add = Rc::new(Cell::new(false));
        add_areas.push(over_add.clone());
        let content = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        content.set_margin_start(12);
        content.set_margin_end(12);
        content.set_margin_top(10);
        content.set_margin_bottom(10);
        let layout = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        layout.set_hexpand(true);
        let cover = artwork.cached_widget(&show.artwork_urls, 64);
        cover.add_css_class("search-artwork");
        layout.append(&cover);
        let text = gtk::Box::new(gtk::Orientation::Vertical, 1);
        text.set_hexpand(true);
        text.set_size_request(0, -1);
        text.set_valign(gtk::Align::Center);
        if let Some(date) = show.updated.as_deref().and_then(format_search_date) {
            let updated = gtk::Label::new(Some(&date));
            updated.set_halign(gtk::Align::Start);
            updated.add_css_class("caption");
            updated.add_css_class("dim-label");
            text.append(&updated);
        }
        let title = gtk::Label::new(Some(&show.title));
        title.set_halign(gtk::Align::Start);
        title.set_xalign(0.0);
        title.set_ellipsize(gtk::pango::EllipsizeMode::End);
        text.append(&title);
        if !show.author.is_empty() {
            let author = gtk::Label::new(Some(&show.author));
            author.set_halign(gtk::Align::Start);
            author.set_xalign(0.0);
            author.set_ellipsize(gtk::pango::EllipsizeMode::End);
            author.add_css_class("dim-label");
            text.append(&author);
        }
        layout.append(&text);
        content.append(&layout);

        let add = gtk::Button::new();
        let add_content = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        add_content.append(&gtk::Image::from_icon_name("list-add-symbolic"));
        add_content.append(&gtk::Label::new(Some("Add")));
        add.set_child(Some(&add_content));
        add.set_valign(gtk::Align::Center);
        add.set_tooltip_text(Some("Add to Following (coming soon)"));
        add.set_sensitive(false);
        let add_area = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        let add_motion = gtk::EventControllerMotion::new();
        let over_add_on_enter = over_add.clone();
        add_motion.connect_enter(move |_, _, _| over_add_on_enter.set(true));
        add_motion.connect_leave(move |_| over_add.set(false));
        add_area.add_controller(add_motion);
        add_area.append(&add);
        content.append(&add_area);
        row.set_child(Some(&content));
        list.append(&row);
        if cue_first && index == 0 {
            row.add_css_class("first-result-cue");
            gtk::glib::timeout_add_local_once(Duration::from_millis(1500), move || {
                row.remove_css_class("first-result-cue");
            });
        }
    }
    list.connect_row_activated(move |_, row| {
        let index = row.index() as usize;
        if let Some(show) = shows.get(index) {
            if !add_areas[index].get() {
                open_feed(show.clone());
            }
        }
    });
    container.append(&list);
}

fn format_search_date(raw: &str) -> Option<String> {
    let updated = gtk::glib::DateTime::from_iso8601(raw, None).ok()?.to_local().ok()?;
    let now = gtk::glib::DateTime::now_local().ok()?;
    let same_day = |date: &gtk::glib::DateTime| {
        updated.year() == date.year()
            && updated.month() == date.month()
            && updated.day_of_month() == date.day_of_month()
    };
    if same_day(&now) {
        Some("Today".to_string())
    } else if same_day(&now.add_days(-1).ok()?) {
        Some("Yesterday".to_string())
    } else {
        let pattern = if updated.year() == now.year() { "%e %b" } else { "%e %b %Y" };
        Some(updated.format(pattern).ok()?.trim().to_string())
    }
}

fn show_feed(description: &gtk::Label, read_more: &gtk::Button, episodes: &gtk::Box, feed: FeedDetails) {
    let text = plain_description(&feed.description);
    description.set_text(&text);
    description.set_visible(!text.is_empty());
    read_more.set_visible(text.chars().count() > 180);
    if feed.episodes.is_empty() {
        show_message(episodes, "No episodes found", "This RSS feed has no recent episodes to show.");
        return;
    }
    let list = gtk::ListBox::new();
    list.add_css_class("boxed-list");
    list.set_selection_mode(gtk::SelectionMode::None);
    for episode in feed.episodes {
        let row = adw::ActionRow::new();
        row.set_title(&episode.title);
        row.set_title_lines(2);
        row.set_subtitle(&episode.date);
        row.set_subtitle_lines(1);
        list.append(&row);
    }
    episodes.append(&list);
}

fn plain_description(raw: &str) -> String {
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

fn categories(request: Rc<dyn Fn(Request)>) -> gtk::Widget {
    let flow = gtk::FlowBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .max_children_per_line(3)
        .min_children_per_line(1)
        .column_spacing(12)
        .row_spacing(12)
        .homogeneous(true)
        .build();

    for (icon, name, color) in CATEGORIES.iter() {
        let tile = gtk::Button::new();
        tile.set_size_request(180, 82);
        tile.set_overflow(gtk::Overflow::Hidden);
        tile.add_css_class("category-tile");
        tile.add_css_class(&format!("category-{color}"));

        let layout = gtk::Overlay::new();
        let surface = gtk::Box::new(gtk::Orientation::Vertical, 0);
        surface.set_size_request(-1, 82);
        layout.set_child(Some(&surface));

        let image = gtk::Image::from_icon_name(icon);
        image.set_pixel_size(92);
        image.set_halign(gtk::Align::End);
        image.set_valign(gtk::Align::End);
        image.set_margin_end(8);
        image.add_css_class("category-icon");
        layout.add_overlay(&image);

        let label = gtk::Label::new(Some(name));
        label.set_halign(gtk::Align::Start);
        label.set_valign(gtk::Align::End);
        label.set_margin_start(16);
        label.set_margin_bottom(11);
        label.set_ellipsize(gtk::pango::EllipsizeMode::End);
        label.add_css_class("category-title");
        layout.add_overlay(&label);

        tile.set_child(Some(&layout));
        let request = request.clone();
        let category = (*name).to_string();
        tile.connect_clicked(move |_| request(Request::Category(category.clone())));
        flow.insert(&tile, -1);
    }

    flow.upcast()
}
