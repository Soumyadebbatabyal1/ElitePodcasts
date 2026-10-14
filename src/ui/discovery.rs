use adw::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::mpsc;
use std::time::Duration;

use crate::data::CATEGORIES;
use crate::discovery_service::{self, FeedDetails, Show};

#[derive(Clone)]
enum Request {
    Search(String),
    Category(String),
}

pub(super) struct DiscoveryPage {
    pub widget: gtk::Widget,
    pub reset: Rc<dyn Fn()>,
}

pub(super) fn page(entry: &gtk::SearchEntry, status: Rc<dyn Fn(&str, bool)>) -> DiscoveryPage {
    let view = gtk::Stack::new();
    view.set_vexpand(true);

    let results = gtk::Box::new(gtk::Orientation::Vertical, 18);
    results.set_margin_top(24);
    results.set_margin_bottom(40);
    let back = gtk::Button::from_icon_name("go-previous-symbolic");
    back.add_css_class("flat");
    back.set_tooltip_text(Some("Back to Discover"));
    back.set_halign(gtk::Align::Start);
    results.append(&back);
    let listing = gtk::Box::new(gtk::Orientation::Vertical, 8);
    results.append(&listing);
    let results_clamp = adw::Clamp::builder().maximum_size(930).build();
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
    feed_content.set_margin_top(24);
    feed_content.set_margin_bottom(40);
    let feed_back = gtk::Button::from_icon_name("go-previous-symbolic");
    feed_back.add_css_class("flat");
    feed_back.set_tooltip_text(Some("Back to results"));
    feed_back.set_halign(gtk::Align::Start);
    feed_content.append(&feed_back);
    let feed_description = gtk::Label::new(None);
    feed_description.set_halign(gtk::Align::Start);
    feed_description.set_wrap(true);
    feed_description.add_css_class("dim-label");
    feed_content.append(&feed_description);
    let episodes = gtk::Box::new(gtk::Orientation::Vertical, 8);
    feed_content.append(&episodes);
    let feed_clamp = adw::Clamp::builder().maximum_size(930).build();
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
    let result_title = Rc::new(RefCell::new(String::from("Search Results")));
    let view_for_feed_back = view.clone();
    let generation_for_feed_back = generation.clone();
    let status_for_feed_back = status.clone();
    let result_title_for_back = result_title.clone();
    feed_back.connect_clicked(move |_| {
        generation_for_feed_back.set(generation_for_feed_back.get().wrapping_add(1));
        view_for_feed_back.set_visible_child_name("results");
        status_for_feed_back(&result_title_for_back.borrow(), false);
    });
    let open_feed: Rc<dyn Fn(Show)> = {
        let view = view.clone();
        let generation = generation.clone();
        let status = status.clone();
        Rc::new(move |show| {
            status(&show.title, true);
            feed_description.set_text("");
            clear(&episodes);
            let spinner = gtk::Spinner::new();
            spinner.start();
            spinner.set_halign(gtk::Align::Center);
            spinner.set_margin_top(32);
            episodes.append(&spinner);
            view.set_visible_child_name("feed");

            let id = generation.get().wrapping_add(1);
            generation.set(id);
            let (tx, rx) = mpsc::channel();
            let fallback_title = show.title.clone();
            std::thread::spawn(move || {
                let _ = tx.send(discovery_service::feed(&show.feed_url));
            });
            let generation = generation.clone();
            let episodes = episodes.clone();
            let feed_description = feed_description.clone();
            let status = status.clone();
            gtk::glib::timeout_add_local(Duration::from_millis(100), move || {
                if generation.get() != id {
                    return gtk::glib::ControlFlow::Break;
                }
                match rx.try_recv() {
                    Ok(answer) => {
                        clear(&episodes);
                        match answer {
                            Ok(feed) => {
                                status(if feed.title.is_empty() { &fallback_title } else { &feed.title }, false);
                                show_feed(&feed_description, &episodes, feed);
                            }
                            Err(message) => {
                                status(&fallback_title, false);
                                show_message(&episodes, "Could not load RSS feed", &message);
                            }
                        }
                        gtk::glib::ControlFlow::Break
                    }
                    Err(mpsc::TryRecvError::Empty) => gtk::glib::ControlFlow::Continue,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        clear(&episodes);
                        status(&fallback_title, false);
                        show_message(&episodes, "Could not load RSS feed", "The request ended unexpectedly.");
                        gtk::glib::ControlFlow::Break
                    }
                }
            });
        })
    };
    let view_for_back = view.clone();
    let generation_for_back = generation.clone();
    let entry_for_back = entry.clone();
    let status_for_back = status.clone();
    back.connect_clicked(move |_| {
        generation_for_back.set(generation_for_back.get().wrapping_add(1));
        entry_for_back.set_text("");
        view_for_back.set_visible_child_name("home");
        status_for_back("Discover", false);
    });
    let request: Rc<dyn Fn(Request)> = {
        let view = view.clone();
        let listing = listing.clone();
        let generation = generation.clone();
        let open_feed = open_feed.clone();
        let status = status.clone();
        let result_title = result_title.clone();
        Rc::new(move |request| {
            let title = match &request {
                Request::Search(_) => "Search Results".to_string(),
                Request::Category(name) => name.clone(),
            };
            *result_title.borrow_mut() = title.clone();
            status(&title, true);
            clear(&listing);
            let spinner = gtk::Spinner::new();
            spinner.start();
            spinner.set_halign(gtk::Align::Center);
            spinner.set_margin_top(32);
            listing.append(&spinner);
            view.set_visible_child_name("results");

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
            let status = status.clone();
            gtk::glib::timeout_add_local(Duration::from_millis(100), move || {
                if generation.get() != id {
                    return gtk::glib::ControlFlow::Break;
                }
                match rx.try_recv() {
                    Ok(answer) => {
                        clear(&listing);
                        status(&title, false);
                        match answer {
                            Ok(shows) => show_results(&listing, shows, open_feed.clone()),
                            Err(message) => show_message(&listing, "Could not load podcasts", &message),
                        }
                        gtk::glib::ControlFlow::Break
                    }
                    Err(mpsc::TryRecvError::Empty) => gtk::glib::ControlFlow::Continue,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        clear(&listing);
                        status(&title, false);
                        show_message(&listing, "Could not load podcasts", "The request ended unexpectedly.");
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
    entry.connect_search_changed(move |entry| {
        if entry.text().trim().is_empty() {
            generation_for_clear.set(generation_for_clear.get().wrapping_add(1));
            view_for_clear.set_visible_child_name("home");
            status_for_clear("Discover", false);
        }
    });
    let reset_view = view.clone();
    let reset_generation = generation.clone();
    let reset_entry = entry.clone();
    let reset_status = status.clone();
    let reset: Rc<dyn Fn()> = Rc::new(move || {
        reset_generation.set(reset_generation.get().wrapping_add(1));
        reset_entry.set_text("");
        reset_view.set_visible_child_name("home");
        reset_status("Discover", false);
    });
    DiscoveryPage { widget: view.upcast(), reset }
}

fn clear(container: &gtk::Box) {
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }
}

fn show_message(container: &gtk::Box, title: &str, message: &str) {
    container.append(&adw::StatusPage::builder()
        .icon_name("dialog-information-symbolic")
        .title(title)
        .description(message)
        .build());
}

fn show_results(container: &gtk::Box, shows: Vec<Show>, open_feed: Rc<dyn Fn(Show)>) {
    if shows.is_empty() {
        show_message(container, "No podcasts found", "No matching podcasts were found.");
        return;
    }
    for show in shows {
        let card = gtk::Button::new();
        card.add_css_class("discovery-result");
        let layout = gtk::Box::new(gtk::Orientation::Horizontal, 16);
        let icon = gtk::Image::from_icon_name("audio-x-generic-symbolic");
        icon.set_pixel_size(48);
        icon.add_css_class("dim-label");
        layout.append(&icon);
        let text = gtk::Box::new(gtk::Orientation::Vertical, 4);
        text.set_hexpand(true);
        let title = gtk::Label::new(Some(&show.title));
        title.set_halign(gtk::Align::Start);
        title.set_wrap(true);
        title.add_css_class("heading");
        text.append(&title);
        if !show.author.is_empty() {
            let author = gtk::Label::new(Some(&show.author));
            author.set_halign(gtk::Align::Start);
            author.add_css_class("dim-label");
            text.append(&author);
        }
        if !show.description.is_empty() {
            let description = gtk::Label::new(Some(&show.description));
            description.set_halign(gtk::Align::Start);
            description.set_wrap(true);
            description.set_lines(2);
            description.set_ellipsize(gtk::pango::EllipsizeMode::End);
            text.append(&description);
        }
        layout.append(&text);
        card.set_child(Some(&layout));
        card.set_tooltip_text(Some(&show.feed_url));
        let open_feed = open_feed.clone();
        card.connect_clicked(move |_| open_feed(show.clone()));
        container.append(&card);
    }
}

fn show_feed(description: &gtk::Label, episodes: &gtk::Box, feed: FeedDetails) {
    if !feed.description.contains('<') {
        description.set_text(&feed.description);
    }
    if feed.episodes.is_empty() {
        show_message(episodes, "No episodes found", "This RSS feed has no recent episodes to show.");
        return;
    }
    let heading = gtk::Label::new(Some("Recent Episodes"));
    heading.add_css_class("title-2");
    heading.set_halign(gtk::Align::Start);
    episodes.append(&heading);
    for episode in feed.episodes {
        let row = gtk::Box::new(gtk::Orientation::Vertical, 4);
        row.add_css_class("discovery-result");
        let label = gtk::Label::new(Some(&episode.title));
        label.set_halign(gtk::Align::Start);
        label.set_wrap(true);
        label.add_css_class("heading");
        row.append(&label);
        if !episode.date.is_empty() {
            let date = gtk::Label::new(Some(&episode.date));
            date.set_halign(gtk::Align::Start);
            date.add_css_class("dim-label");
            row.append(&date);
        }
        episodes.append(&row);
    }
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
