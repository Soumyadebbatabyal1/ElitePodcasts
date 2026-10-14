use adw::prelude::*;
use std::cell::Cell;
use std::rc::Rc;
use std::sync::mpsc;
use std::time::Duration;

use crate::data::{CATEGORIES, FEATURED};
use crate::discovery_service::{self, Show};

const HERO_COLORS: [&str; 3] = ["hero-plum", "hero-green", "hero-blue"];

#[derive(Clone)]
enum Request {
    Search(String),
    Category(String),
}

pub(super) fn page(entry: &gtk::SearchEntry) -> gtk::Widget {
    let view = gtk::Stack::new();
    view.set_vexpand(true);

    let results = gtk::Box::new(gtk::Orientation::Vertical, 18);
    results.set_margin_top(24);
    results.set_margin_bottom(40);
    let results_header = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let back = gtk::Button::from_icon_name("go-previous-symbolic");
    back.add_css_class("flat");
    back.set_tooltip_text(Some("Back to Discover"));
    results_header.append(&back);
    let heading = gtk::Label::new(None);
    heading.add_css_class("title-1");
    heading.set_halign(gtk::Align::Start);
    heading.set_wrap(true);
    results_header.append(&heading);
    results.append(&results_header);
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

    let generation = Rc::new(Cell::new(0u64));
    let view_for_back = view.clone();
    let generation_for_back = generation.clone();
    let entry_for_back = entry.clone();
    back.connect_clicked(move |_| {
        generation_for_back.set(generation_for_back.get().wrapping_add(1));
        entry_for_back.set_text("");
        view_for_back.set_visible_child_name("home");
    });
    let request: Rc<dyn Fn(Request)> = {
        let view = view.clone();
        let heading = heading.clone();
        let listing = listing.clone();
        let generation = generation.clone();
        Rc::new(move |request| {
            heading.set_text(&match &request {
                Request::Search(term) => format!("Search results for “{term}”"),
                Request::Category(name) => format!("{name} podcasts"),
            });
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
            gtk::glib::timeout_add_local(Duration::from_millis(100), move || {
                if generation.get() != id {
                    return gtk::glib::ControlFlow::Break;
                }
                match rx.try_recv() {
                    Ok(answer) => {
                        clear(&listing);
                        match answer {
                            Ok(shows) => show_results(&listing, shows),
                            Err(message) => show_message(&listing, "Could not load podcasts", &message),
                        }
                        gtk::glib::ControlFlow::Break
                    }
                    Err(mpsc::TryRecvError::Empty) => gtk::glib::ControlFlow::Continue,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        clear(&listing);
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
    content.append(&featured());
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
    entry.connect_search_changed(move |entry| {
        if entry.text().trim().is_empty() {
            generation_for_clear.set(generation_for_clear.get().wrapping_add(1));
            view_for_clear.set_visible_child_name("home");
        }
    });
    view.upcast()
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

fn show_results(container: &gtk::Box, shows: Vec<Show>) {
    if shows.is_empty() {
        show_message(container, "No podcasts found", "No matching podcasts were found.");
        return;
    }
    for show in shows {
        let card = gtk::Box::new(gtk::Orientation::Horizontal, 16);
        card.add_css_class("discovery-result");
        let icon = gtk::Image::from_icon_name("audio-x-generic-symbolic");
        icon.set_pixel_size(48);
        icon.add_css_class("dim-label");
        card.append(&icon);
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
        card.append(&text);
        card.set_tooltip_text(Some(&show.feed_url));
        container.append(&card);
    }
}

fn featured() -> gtk::Widget {
    let index = std::rc::Rc::new(std::cell::Cell::new(0usize));
    let carousel = gtk::Box::new(gtk::Orientation::Vertical, 12);
    let frame = gtk::Overlay::new();
    frame.set_overflow(gtk::Overflow::Hidden);
    frame.add_css_class("featured-frame");

    let panel = gtk::Box::new(gtk::Orientation::Vertical, 0);
    panel.set_height_request(320);
    panel.add_css_class("featured-panel");
    panel.add_css_class(HERO_COLORS[0]);

    let details = gtk::Box::new(gtk::Orientation::Vertical, 10);
    details.set_halign(gtk::Align::Center);
    details.set_valign(gtk::Align::Center);
    details.set_margin_start(54);
    details.set_margin_end(54);
    details.set_margin_top(32);
    details.set_margin_bottom(32);

    let image = gtk::Picture::for_filename(FEATURED[0].artwork);
    image.set_content_fit(gtk::ContentFit::Cover);
    image.set_size_request(132, 132);
    image.set_can_shrink(true);
    image.set_halign(gtk::Align::Center);
    image.set_overflow(gtk::Overflow::Hidden);
    image.add_css_class("featured-artwork");

    let title = gtk::Label::new(Some(FEATURED[0].title));
    title.set_halign(gtk::Align::Center);
    title.set_justify(gtk::Justification::Center);
    title.set_wrap(true);
    title.add_css_class("title-1");

    let description = gtk::Label::new(Some(FEATURED[0].description));
    description.set_halign(gtk::Align::Center);
    description.set_justify(gtk::Justification::Center);
    description.set_wrap(true);
    description.set_max_width_chars(58);
    description.add_css_class("hero-description");

    details.append(&image);
    details.append(&title);
    details.append(&description);
    panel.append(&details);
    frame.set_child(Some(&panel));

    let previous = gtk::Button::from_icon_name("go-previous-symbolic");
    previous.add_css_class("flat");
    previous.add_css_class("hero-arrow");
    previous.set_tooltip_text(Some("Previous featured podcast"));
    previous.set_halign(gtk::Align::Start);
    previous.set_valign(gtk::Align::Center);
    previous.set_margin_start(18);
    frame.add_overlay(&previous);

    let next = gtk::Button::from_icon_name("go-next-symbolic");
    next.add_css_class("flat");
    next.add_css_class("hero-arrow");
    next.set_tooltip_text(Some("Next featured podcast"));
    next.set_halign(gtk::Align::End);
    next.set_valign(gtk::Align::Center);
    next.set_margin_end(18);
    frame.add_overlay(&next);

    let dots = gtk::Label::new(Some("●  ○  ○"));
    dots.add_css_class("pagination-dots");
    dots.set_halign(gtk::Align::Center);

    for (button, direction) in [(previous, -1isize), (next, 1isize)] {
        let pos = index.clone();
        let hero = panel.clone();
        let artwork = image.clone();
        let heading = title.clone();
        let summary = description.clone();
        let pagination = dots.clone();
        button.connect_clicked(move |_| {
            let current = pos.get();
            let updated = (current as isize + direction + FEATURED.len() as isize) as usize
                % FEATURED.len();
            pos.set(updated);
            hero.remove_css_class(HERO_COLORS[current]);
            hero.add_css_class(HERO_COLORS[updated]);
            artwork.set_filename(Some(FEATURED[updated].artwork));
            heading.set_text(FEATURED[updated].title);
            summary.set_text(FEATURED[updated].description);
            pagination.set_text(
                &(0..FEATURED.len())
                    .map(|i| if i == updated { "●" } else { "○" })
                    .collect::<Vec<_>>()
                    .join("  "),
            );
        });
    }

    carousel.append(&frame);
    carousel.append(&dots);
    carousel.upcast()
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
        tile.set_size_request(270, 108);
        tile.set_overflow(gtk::Overflow::Hidden);
        tile.add_css_class("category-tile");
        tile.add_css_class(&format!("category-{color}"));

        let layout = gtk::Overlay::new();
        let surface = gtk::Box::new(gtk::Orientation::Vertical, 0);
        surface.set_size_request(-1, 108);
        layout.set_child(Some(&surface));

        let image = gtk::Image::from_icon_name(icon);
        image.set_pixel_size(110);
        image.set_halign(gtk::Align::End);
        image.set_valign(gtk::Align::End);
        image.set_margin_end(8);
        image.add_css_class("category-icon");
        layout.add_overlay(&image);

        let label = gtk::Label::new(Some(name));
        label.set_halign(gtk::Align::Start);
        label.set_valign(gtk::Align::End);
        label.set_margin_start(16);
        label.set_margin_bottom(14);
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
