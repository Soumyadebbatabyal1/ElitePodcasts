mod discovery;

use adw::prelude::*;

pub fn window_content() -> gtk::Box {
    install_styles();
    let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let header = adw::HeaderBar::new();
    header.add_css_class("shell-header");

    let search_button = gtk::ToggleButton::builder()
        .icon_name("system-search-symbolic")
        .tooltip_text("Search podcasts")
        .build();
    header.pack_start(&search_button);

    let stack = adw::ViewStack::builder().hexpand(true).vexpand(true).build();
    let switcher = adw::ViewSwitcher::new();
    switcher.set_policy(adw::ViewSwitcherPolicy::Wide);
    switcher.set_stack(Some(&stack));
    header.set_title_widget(Some(&switcher));

    let menu = gtk::MenuButton::builder()
        .icon_name("open-menu-symbolic")
        .tooltip_text("Main menu")
        .build();
    let popover = gtk::Popover::new();
    let menu_content = gtk::Box::new(gtk::Orientation::Vertical, 6);
    menu_content.set_margin_top(8);
    menu_content.set_margin_bottom(8);
    menu_content.set_margin_start(8);
    menu_content.set_margin_end(8);
    for label in ["About Elite Podcasts", "Keyboard Shortcuts"] {
        let row = gtk::Button::with_label(label);
        row.add_css_class("flat");
        menu_content.append(&row);
    }
    popover.set_child(Some(&menu_content));
    menu.set_popover(Some(&popover));
    header.pack_end(&menu);
    root.append(&header);

    let search_revealer = gtk::Revealer::builder()
        .transition_type(gtk::RevealerTransitionType::SlideDown)
        .reveal_child(false)
        .build();
    let search_clamp = adw::Clamp::builder().maximum_size(500).build();
    search_clamp.set_margin_top(8);
    search_clamp.set_margin_bottom(12);
    search_clamp.set_margin_start(24);
    search_clamp.set_margin_end(24);
    let entry = gtk::SearchEntry::builder()
        .placeholder_text("Search podcasts")
        .hexpand(true)
        .build();
    search_clamp.set_child(Some(&entry));
    search_revealer.set_child(Some(&search_clamp));
    root.append(&search_revealer);
    let revealer = search_revealer.clone();
    let search_entry = entry.clone();
    search_button.connect_toggled(move |button| {
        revealer.set_reveal_child(button.is_active());
        if button.is_active() {
            search_entry.grab_focus();
        } else {
            search_entry.set_text("");
        }
    });

    stack.add_titled_with_icon(&discovery::page(&entry), Some("discover"), "Discover", "find-location-symbolic");
    let navigation = stack.clone();
    entry.connect_activate(move |_| navigation.set_visible_child_name("discover"));
    stack.add_titled_with_icon(&empty_state("Your followed podcasts will appear here.", "starred-symbolic"), Some("following"), "Following", "folder-music-symbolic");
    stack.add_titled_with_icon(&empty_state("New episodes from your shows will appear here.", "media-playlist-repeat-symbolic"), Some("new-episodes"), "New Episodes", "view-refresh-symbolic");
    root.append(&stack);
    root
}

fn install_styles() {
    let provider = gtk::CssProvider::new();
    provider.load_from_data(include_str!("../../assets/style.css"));
    if let Some(display) = gtk::gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}

fn empty_state(message: &str, icon: &str) -> gtk::Widget {
    adw::StatusPage::builder()
        .icon_name(icon)
        .title("Nothing here yet")
        .description(message)
        .vexpand(true)
        .build()
        .upcast()
}
