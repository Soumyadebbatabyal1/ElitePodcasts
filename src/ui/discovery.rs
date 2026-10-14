use adw::prelude::*;

use crate::data::{CATEGORIES, FEATURED};

const HERO_COLORS: [&str; 3] = ["hero-plum", "hero-green", "hero-blue"];

pub(super) fn page() -> gtk::Widget {
    let scroller = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .build();
    let content = gtk::Box::new(gtk::Orientation::Vertical, 30);
    content.set_margin_top(24);
    content.set_margin_bottom(40);
    content.append(&featured());
    content.append(&categories());

    let clamp = adw::Clamp::builder().maximum_size(930).build();
    clamp.set_margin_start(24);
    clamp.set_margin_end(24);
    clamp.set_child(Some(&content));
    scroller.set_child(Some(&clamp));
    scroller.upcast()
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

fn categories() -> gtk::Widget {
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
        flow.insert(&tile, -1);
    }

    flow.upcast()
}
