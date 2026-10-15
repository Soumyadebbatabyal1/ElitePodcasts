mod artwork;
mod discovery;

use adw::prelude::*;
use gtk::glib::value::ToValue;
use std::rc::Rc;

pub fn window_content() -> gtk::Widget {
    install_styles();

    let split = adw::NavigationSplitView::new();
    split.set_min_sidebar_width(220.0);
    split.set_max_sidebar_width(260.0);
    split.set_show_content(true);

    let sidebar = gtk::Box::new(gtk::Orientation::Vertical, 0);
    sidebar.add_css_class("sidebar-pane");
    let sidebar_header = adw::HeaderBar::new();
    sidebar_header.add_css_class("shell-header");
    sidebar_header.set_show_start_title_buttons(false);
    sidebar_header.set_show_end_title_buttons(false);
    sidebar_header.set_title_widget(Some(&gtk::Label::new(Some("Elite Podcasts"))));

    let search_button = gtk::ToggleButton::builder()
        .icon_name("system-search-symbolic")
        .tooltip_text("Search podcasts")
        .build();
    sidebar_header.pack_start(&search_button);
    sidebar_header.pack_end(&menu_button());
    sidebar.append(&sidebar_header);

    let navigation = gtk::ListBox::new();
    navigation.add_css_class("navigation-sidebar");
    navigation.set_selection_mode(gtk::SelectionMode::Single);
    navigation.set_margin_top(12);
    navigation.set_margin_start(8);
    navigation.set_margin_end(8);
    let discover_row = navigation_row("find-location-symbolic", "Discover");
    let following_row = navigation_row("folder-music-symbolic", "Following");
    let episodes_row = navigation_row("view-refresh-symbolic", "New Episodes");
    navigation.append(&discover_row);
    navigation.append(&following_row);
    navigation.append(&episodes_row);
    navigation.select_row(Some(&discover_row));
    sidebar.append(&navigation);
    split.set_sidebar(Some(&adw::NavigationPage::new(&sidebar, "Elite Podcasts")));

    let main = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let main_header = adw::HeaderBar::new();
    main_header.add_css_class("shell-header");
    main_header.set_show_start_title_buttons(false);
    let show_sidebar = gtk::Button::from_icon_name("go-previous-symbolic");
    show_sidebar.set_tooltip_text(Some("Show navigation"));
    show_sidebar.set_visible(false);
    let split_for_back = split.clone();
    show_sidebar.connect_clicked(move |_| split_for_back.set_show_content(false));
    main_header.pack_start(&show_sidebar);

    let title_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    title_box.set_halign(gtk::Align::Center);
    let spinner = gtk::Spinner::new();
    spinner.set_visible(false);
    title_box.append(&spinner);
    let title = gtk::Label::new(Some("Discover"));
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    title.set_max_width_chars(34);
    title.add_css_class("heading");
    title_box.append(&title);
    main_header.set_title_widget(Some(&title_box));
    main.append(&main_header);

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
    main.append(&search_revealer);
    let revealer = search_revealer.clone();
    let search_entry = entry.clone();
    let split_for_search = split.clone();
    search_button.connect_toggled(move |button| {
        revealer.set_reveal_child(button.is_active());
        if button.is_active() {
            split_for_search.set_show_content(true);
            search_entry.grab_focus();
        } else {
            search_entry.set_text("");
        }
    });

    let pages = gtk::Stack::new();
    pages.set_vexpand(true);
    let pages_for_search = pages.clone();
    let navigation_for_search = navigation.clone();
    let discover_for_search = discover_row.clone();
    entry.connect_activate(move |_| {
        pages_for_search.set_visible_child_name("discover");
        navigation_for_search.select_row(Some(&discover_for_search));
    });

    let title_for_status = title.clone();
    let spinner_for_status = spinner.clone();
    let pages_for_status = pages.clone();
    let status: Rc<dyn Fn(&str, bool)> = Rc::new(move |name, loading| {
        if pages_for_status.visible_child_name().as_deref() != Some("discover") {
            return;
        }
        title_for_status.set_text(name);
        spinner_for_status.set_visible(loading);
        if loading {
            spinner_for_status.start();
        } else {
            spinner_for_status.stop();
        }
    });
    let discovery = discovery::page(&entry, status);
    pages.add_named(&discovery.widget, Some("discover"));
    pages.add_named(&empty_state("Your followed podcasts will appear here.", "starred-symbolic"), Some("following"));
    pages.add_named(&empty_state("New episodes from your shows will appear here.", "media-playlist-repeat-symbolic"), Some("new-episodes"));
    pages.set_visible_child_name("discover");
    main.append(&pages);
    split.set_content(Some(&adw::NavigationPage::new(&main, "Discover")));

    let pages_for_navigation = pages.clone();
    let title_for_navigation = title.clone();
    let spinner_for_navigation = spinner.clone();
    let split_for_navigation = split.clone();
    let reset_discovery = discovery.reset.clone();
    let search_for_navigation = search_button.clone();
    navigation.connect_row_activated(move |list, row| {
        list.select_row(Some(row));
        match row.index() {
            0 => {
                pages_for_navigation.set_visible_child_name("discover");
                reset_discovery();
                title_for_navigation.set_text("Discover");
            }
            1 => {
                pages_for_navigation.set_visible_child_name("following");
                search_for_navigation.set_active(false);
                title_for_navigation.set_text("Following");
            }
            _ => {
                pages_for_navigation.set_visible_child_name("new-episodes");
                search_for_navigation.set_active(false);
                title_for_navigation.set_text("New Episodes");
            }
        }
        spinner_for_navigation.stop();
        spinner_for_navigation.set_visible(false);
        split_for_navigation.set_show_content(true);
    });

    let sidebar_header_for_breakpoint = sidebar_header.clone();
    let show_sidebar_for_breakpoint = show_sidebar.clone();
    split.connect_collapsed_notify(move |split| {
        sidebar_header_for_breakpoint.set_show_end_title_buttons(split.is_collapsed());
        show_sidebar_for_breakpoint.set_visible(split.is_collapsed());
    });

    let adaptive = adw::BreakpointBin::new();
    adaptive.set_size_request(320, 400);
    adaptive.set_child(Some(&split));
    let compact = adw::Breakpoint::new(adw::BreakpointCondition::new_length(
        adw::BreakpointConditionLengthType::MaxWidth,
        720.0,
        adw::LengthUnit::Sp,
    ));
    compact.add_setter(&split, "collapsed", Some(&true.to_value()));
    adaptive.add_breakpoint(compact);
    adaptive.upcast()
}

fn navigation_row(icon: &str, label: &str) -> gtk::ListBoxRow {
    let row = gtk::ListBoxRow::new();
    row.add_css_class("sidebar-row");
    let content = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    content.set_margin_start(12);
    content.set_margin_end(12);
    content.set_margin_top(10);
    content.set_margin_bottom(10);
    content.append(&gtk::Image::from_icon_name(icon));
    let text = gtk::Label::new(Some(label));
    text.set_halign(gtk::Align::Start);
    content.append(&text);
    row.set_child(Some(&content));
    row
}

fn menu_button() -> gtk::MenuButton {
    let menu = gtk::MenuButton::builder()
        .icon_name("open-menu-symbolic")
        .tooltip_text("Main menu")
        .build();
    let popover = gtk::Popover::new();
    let content = gtk::Box::new(gtk::Orientation::Vertical, 6);
    content.set_margin_top(8);
    content.set_margin_bottom(8);
    content.set_margin_start(8);
    content.set_margin_end(8);
    for label in ["About Elite Podcasts", "Keyboard Shortcuts"] {
        let row = gtk::Button::with_label(label);
        row.add_css_class("flat");
        content.append(&row);
    }
    popover.set_child(Some(&content));
    menu.set_popover(Some(&popover));
    menu
}

fn install_styles() {
    let provider = gtk::CssProvider::new();
    provider.load_from_data(include_str!("../../assets/style.css"));
    let accent = gtk::CssProvider::new();
    let manager = adw::StyleManager::default();
    accent.load_from_data(if manager.is_dark() {
        include_str!("../../assets/accent-dark.css")
    } else {
        include_str!("../../assets/accent-light.css")
    });
    if let Some(display) = gtk::gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
        gtk::style_context_add_provider_for_display(
            &display,
            &accent,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION + 1,
        );
    }
    manager.connect_dark_notify(move |manager| {
        accent.load_from_data(if manager.is_dark() {
            include_str!("../../assets/accent-dark.css")
        } else {
            include_str!("../../assets/accent-light.css")
        });
    });
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
