use adw::prelude::*;
use crate::data::{Podcast, CATEGORIES, FEATURED, TRENDING};

pub(super) fn page() -> gtk::Widget {
    let scroller = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .build();
    let content = gtk::Box::new(gtk::Orientation::Vertical, 32);
    content.set_margin_top(24);
    content.set_margin_bottom(40);
    content.set_margin_start(24);
    content.set_margin_end(24);
    content.set_halign(gtk::Align::Center);
    content.set_hexpand(true);
    content.append(&featured());
    content.append(&categories());
    content.append(&podcast_section("Trending Podcasts", &TRENDING));
    content.append(&podcast_section("Top Shows", &FEATURED));
    content.append(&podcast_section("Recently Updated", &TRENDING));
    scroller.set_child(Some(&content));
    scroller.upcast()
}

fn featured() -> gtk::Widget {
    let index = std::rc::Rc::new(std::cell::Cell::new(0usize));
    let frame = gtk::Overlay::new();
    frame.set_overflow(gtk::Overflow::Hidden);
    frame.add_css_class("card");

    let panel = gtk::Box::new(gtk::Orientation::Vertical, 0);
    panel.add_css_class("featured-panel");
    let image = gtk::Picture::for_filename(FEATURED[0].artwork);
    image.set_content_fit(gtk::ContentFit::Cover);
    image.set_size_request(-1, 220);
    image.set_can_shrink(true);
    panel.append(&image);
    let details = gtk::Box::new(gtk::Orientation::Vertical, 12);
    details.set_valign(gtk::Align::Center);
    details.set_margin_start(24);
    details.set_margin_end(24);
    details.set_margin_top(20);
    details.set_margin_bottom(32);
    details.set_hexpand(true);
    let eyebrow = gtk::Label::new(Some("FEATURED PODCAST"));
    eyebrow.set_halign(gtk::Align::Start);
    eyebrow.add_css_class("dim-label");
    eyebrow.add_css_class("caption");
    let title = gtk::Label::new(Some(FEATURED[0].title));
    title.set_halign(gtk::Align::Start);
    title.set_wrap(true);
    title.add_css_class("title-1");
    let description = gtk::Label::new(Some(FEATURED[0].description));
    description.set_halign(gtk::Align::Start);
    description.set_wrap(true);
    description.add_css_class("dim-label");
    details.append(&eyebrow);
    details.append(&title);
    details.append(&description);
    details.append(&gtk::Button::with_label("Explore podcast"));
    panel.append(&details);
    frame.set_child(Some(&panel));

    let previous = gtk::Button::from_icon_name("go-previous-symbolic");
    previous.add_css_class("circular");
    previous.set_halign(gtk::Align::Start);
    previous.set_valign(gtk::Align::Center);
    previous.set_margin_start(10);
    frame.add_overlay(&previous);
    let next = gtk::Button::from_icon_name("go-next-symbolic");
    next.add_css_class("circular");
    next.set_halign(gtk::Align::End);
    next.set_valign(gtk::Align::Center);
    next.set_margin_end(10);
    frame.add_overlay(&next);

    let dots = gtk::Label::new(Some("●  ○  ○"));
    dots.add_css_class("dim-label");
    dots.set_halign(gtk::Align::Center);
    dots.set_valign(gtk::Align::End);
    dots.set_margin_bottom(10);
    frame.add_overlay(&dots);

    for (button, direction) in [(previous, -1isize), (next, 1isize)] {
        let pos = index.clone();
        let img = image.clone();
        let heading = title.clone();
        let summary = description.clone();
        let pagination = dots.clone();
        button.connect_clicked(move |_| {
            let updated = (pos.get() as isize + direction + FEATURED.len() as isize) as usize % FEATURED.len();
            pos.set(updated);
            img.set_filename(Some(FEATURED[updated].artwork));
            heading.set_text(FEATURED[updated].title);
            summary.set_text(FEATURED[updated].description);
            pagination.set_text(&(0..FEATURED.len()).map(|i| if i == updated { "●" } else { "○" }).collect::<Vec<_>>().join("  "));
        });
    }
    frame.upcast()
}

fn categories() -> gtk::Widget {
    let section = gtk::Box::new(gtk::Orientation::Vertical, 14);
    section.append(&section_title("Browse Categories"));
    let grid = gtk::Grid::builder().column_spacing(12).row_spacing(12).column_homogeneous(true).build();
    for (i, (icon, name, color)) in CATEGORIES.iter().enumerate() {
        let tile = gtk::Button::new();
        tile.add_css_class("category-tile");
        tile.add_css_class(&format!("category-{color}"));
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        row.set_margin_top(12);
        row.set_margin_bottom(12);
        row.set_margin_start(14);
        row.set_margin_end(14);
        let image = gtk::Image::from_icon_name(icon);
        image.set_pixel_size(20);
        row.append(&image);
        let label = gtk::Label::new(Some(name));
        label.set_halign(gtk::Align::Start);
        label.set_ellipsize(gtk::pango::EllipsizeMode::End);
        row.append(&label);
        tile.set_child(Some(&row));
        grid.attach(&tile, (i % 2) as i32, (i / 2) as i32, 1, 1);
    }
    section.append(&grid);
    section.upcast()
}

fn podcast_section(title: &str, podcasts: &[Podcast]) -> gtk::Widget {
    let section = gtk::Box::new(gtk::Orientation::Vertical, 14);
    section.append(&section_title(title));
    let flow = gtk::FlowBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .max_children_per_line(4)
        .min_children_per_line(1)
        .column_spacing(14)
        .row_spacing(14)
        .homogeneous(true)
        .build();
    for podcast in podcasts {
        flow.insert(&podcast_card(podcast), -1);
    }
    section.append(&flow);
    section.upcast()
}

fn podcast_card(podcast: &Podcast) -> gtk::Widget {
    let card = gtk::Box::new(gtk::Orientation::Vertical, 8);
    card.add_css_class("podcast-card");
    card.set_size_request(140, -1);
    let artwork = gtk::Picture::for_filename(podcast.artwork);
    artwork.set_content_fit(gtk::ContentFit::Cover);
    artwork.set_size_request(160, 150);
    artwork.set_can_shrink(true);
    card.append(&artwork);
    let title = gtk::Label::new(Some(podcast.title));
    title.set_halign(gtk::Align::Start);
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    title.add_css_class("heading");
    card.append(&title);
    let publisher = gtk::Label::new(Some(podcast.publisher));
    publisher.set_halign(gtk::Align::Start);
    publisher.add_css_class("dim-label");
    publisher.add_css_class("caption");
    card.append(&publisher);
    let description = gtk::Label::new(Some(podcast.description));
    description.set_halign(gtk::Align::Start);
    description.set_wrap(true);
    description.set_lines(2);
    description.add_css_class("dim-label");
    card.append(&description);
    card.upcast()
}

fn section_title(title: &str) -> gtk::Widget {
    let label = gtk::Label::new(Some(title));
    label.set_halign(gtk::Align::Start);
    label.add_css_class("title-2");
    label.upcast()
}
