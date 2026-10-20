mod artwork;
mod discovery;
mod downloaded;
mod download_manager;
mod episode;
mod episode_row;
mod following;
mod history;
mod library_panel;
mod new_episodes;
mod opml_io;
mod player;
mod settings;
mod starred;

use adw::prelude::*;
use gtk::glib::value::ToValue;
use std::cell::RefCell;
use std::rc::Rc;
use crate::following::FollowingStore;
use artwork::ArtworkLoader;
use crate::playback::PlaybackController;

pub fn window_content(window: &adw::ApplicationWindow, app: &adw::Application, settings: Rc<crate::settings::AppSettings>) -> gtk::Widget {
    install_styles();
    let store = match FollowingStore::open() {
        Ok(store) => store,
        Err(message) => {
            return adw::StatusPage::builder()
                .icon_name("dialog-error-symbolic")
                .title("Could not open Elite Podcasts")
                .description(&message)
                .build()
                .upcast();
        }
    };
    let artwork = ArtworkLoader::new();
    let toast_overlay = adw::ToastOverlay::new();
    let toast: Rc<dyn Fn(&str)> = Rc::new({
        let overlay = toast_overlay.downgrade();
        move |message| {
            if let Some(overlay) = overlay.upgrade() {
                let notice = adw::Toast::new(message);
                notice.set_use_markup(false);
                overlay.add_toast(notice);
            }
        }
    });
    let active_follow_toast = Rc::new(RefCell::new(None::<adw::Toast>));
    let follow_toast: Rc<dyn Fn(&str)> = Rc::new({
        let overlay = toast_overlay.downgrade();
        let active = active_follow_toast.clone();
        move |message| {
            let Some(overlay) = overlay.upgrade() else { return };
            let current = active.borrow().clone();
            if let Some(current) = current {
                current.set_title(message);
                overlay.add_toast(current);
            } else {
                let notice = adw::Toast::new(message);
                notice.set_timeout(3);
                notice.set_use_markup(false);
                let weak_active = Rc::downgrade(&active);
                notice.connect_dismissed(move |_| {
                    if let Some(active) = weak_active.upgrade() {
                        active.borrow_mut().take();
                    }
                });
                *active.borrow_mut() = Some(notice.clone());
                overlay.add_toast(notice);
            }
        }
    });
    let downloads = download_manager::DownloadManager::new(store.clone(), toast.clone(), &toast_overlay);
    let playback = match PlaybackController::new(store.clone(), settings.clone(), toast.clone()) {
        Ok(playback) => playback,
        Err(message) => {
            return adw::StatusPage::builder()
                .icon_name("dialog-error-symbolic")
                .title("Could not start audio playback")
                .description(&message)
                .build()
                .upcast();
        }
    };
    let playback_on_shutdown = playback.clone();
    app.connect_shutdown(move |_| playback_on_shutdown.persist_on_exit());
    let mpris = crate::mpris::Mpris::start(app, &playback);
    app.connect_shutdown(move |_| mpris.stop());
    let overlay = adw::OverlaySplitView::new();
    overlay.set_collapsed(true);
    overlay.set_show_sidebar(false);
    overlay.set_sidebar_position(gtk::PackType::End);
    overlay.set_min_sidebar_width(300.0);
    overlay.set_max_sidebar_width(380.0);

    let split = adw::NavigationSplitView::new();
    split.set_min_sidebar_width(180.0);
    split.set_max_sidebar_width(205.0);
    split.set_show_content(true);

    let sidebar = gtk::Box::new(gtk::Orientation::Vertical, 0);
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
    let main_menu = menu_button();
    sidebar_header.pack_end(&main_menu);
    sidebar.append(&sidebar_header);

    let navigation = gtk::ListBox::new();
    navigation.add_css_class("navigation-sidebar");
    navigation.set_selection_mode(gtk::SelectionMode::Single);
    let discover_row = navigation_row("software-explore-symbolic", "Discover");
    let following_row = navigation_row("audio-input-microphone-symbolic", "Following");
    let episodes_row = navigation_row("application-rss+xml-symbolic", "New Episodes");
    navigation.append(&discover_row);
    navigation.append(&following_row);
    navigation.append(&episodes_row);
    navigation.select_row(Some(&discover_row));
    sidebar.append(&navigation);
    let divider = gtk::Separator::new(gtk::Orientation::Horizontal);
    divider.set_margin_top(3);
    divider.set_margin_bottom(3);
    divider.set_margin_start(12);
    divider.set_margin_end(12);
    sidebar.append(&divider);
    let library_navigation = gtk::ListBox::new();
    library_navigation.add_css_class("navigation-sidebar");
    library_navigation.set_selection_mode(gtk::SelectionMode::Single);
    let starred_row = navigation_row("starred-symbolic", "Starred");
    let downloaded_row = navigation_row("folder-download-symbolic", "Downloaded");
    let history_row = navigation_row("document-open-recent-symbolic", "History");
    library_navigation.append(&starred_row);
    library_navigation.append(&downloaded_row);
    library_navigation.append(&history_row);
    sidebar.append(&library_navigation);
    split.set_sidebar(Some(&adw::NavigationPage::new(&sidebar, "Elite Podcasts")));

    let main = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let main_header = adw::HeaderBar::new();
    main_header.add_css_class("shell-header");
    main_header.set_show_start_title_buttons(false);
    main_header.set_show_back_button(false);
    let page_back = gtk::Button::from_icon_name("go-previous-symbolic");
    page_back.set_tooltip_text(Some("Back to Discover"));
    page_back.set_visible(false);
    main_header.pack_start(&page_back);
    let show_sidebar = gtk::Button::from_icon_name("go-previous-symbolic");
    show_sidebar.set_tooltip_text(Some("Show navigation"));
    show_sidebar.set_visible(false);
    let split_for_back = split.clone();
    show_sidebar.connect_clicked(move |_| split_for_back.set_show_content(false));
    main_header.pack_start(&show_sidebar);
    let podcast_menu = gtk::MenuButton::builder()
        .icon_name("view-more-symbolic")
        .tooltip_text("Podcast options")
        .build();
    main_header.pack_end(&podcast_menu);
    let episodes_menu = gtk::MenuButton::builder()
        .icon_name("view-more-symbolic")
        .tooltip_text("New Episodes options")
        .build();
    episodes_menu.set_visible(false);
    main_header.pack_end(&episodes_menu);
    let idle_library = gtk::Button::from_icon_name("sidebar-show-right-symbolic");
    idle_library.add_css_class("flat");
    idle_library.set_tooltip_text(Some("Queue and History"));
    queue_button_with_indicator(&idle_library, &playback);
    idle_library.set_visible(playback.current().is_none());
    let weak_overlay = overlay.downgrade();
    idle_library.connect_clicked(move |_| {
        if let Some(overlay) = weak_overlay.upgrade() {
            overlay.set_show_sidebar(true);
        }
    });
    let weak_idle_library = idle_library.downgrade();
    let weak_playback = Rc::downgrade(&playback);
    playback.watch(move || {
        let (Some(button), Some(playback)) = (weak_idle_library.upgrade(), weak_playback.upgrade()) else {
            return false;
        };
        button.set_visible(playback.current().is_none());
        true
    });
    main_header.pack_end(&idle_library);

    let title_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    title_box.set_halign(gtk::Align::Center);
    let spinner = gtk::Spinner::new();
    spinner.set_size_request(20, 20);
    spinner.set_opacity(0.0);
    title_box.append(&spinner);
    let title = gtk::Label::new(Some("Discover"));
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    title.set_max_width_chars(34);
    title.add_css_class("heading");
    title_box.append(&title);
    let title_balance = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    title_balance.set_size_request(20, 20);
    title_box.append(&title_balance);
    main_header.set_title_widget(Some(&title_box));
    main.append(&main_header);

    let search_revealer = gtk::Revealer::builder()
        .transition_type(gtk::RevealerTransitionType::SlideDown)
        .reveal_child(false)
        .build();
    let search_clamp = adw::Clamp::builder().maximum_size(460).build();
    search_clamp.set_margin_top(6);
    search_clamp.set_margin_bottom(8);
    search_clamp.set_margin_start(16);
    search_clamp.set_margin_end(16);
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
        }
    });

    let pages = gtk::Stack::new();
    pages.set_vexpand(true);

    let title_for_status = title.clone();
    let spinner_for_status = spinner.clone();
    let pages_for_status = pages.clone();
    let status: Rc<dyn Fn(&str, bool)> = Rc::new(move |name, loading| {
        if pages_for_status.visible_child_name().as_deref() != Some("discover") {
            return;
        }
        title_for_status.set_text(name);
        if loading {
            spinner_for_status.start();
            spinner_for_status.set_opacity(1.0);
        } else {
            spinner_for_status.stop();
            spinner_for_status.set_opacity(0.0);
        }
    });
    let pages_for_following_back = pages.clone();
    let navigation_for_following_back = navigation.clone();
    let row_for_following_back = following_row.clone();
    let title_for_following_back = title.clone();
    let spinner_for_following_back = spinner.clone();
    let episodes_menu_for_following_back = episodes_menu.clone();
    let back_to_following: Rc<dyn Fn()> = Rc::new(move || {
        pages_for_following_back.set_transition_type(gtk::StackTransitionType::SlideRight);
        pages_for_following_back.set_visible_child_name("following");
        episodes_menu_for_following_back.set_visible(false);
        navigation_for_following_back.select_row(Some(&row_for_following_back));
        title_for_following_back.set_text("Following");
        spinner_for_following_back.stop();
        spinner_for_following_back.set_opacity(0.0);
    });
    let pages_for_star_back = pages.clone();
    let title_for_star_back = title.clone();
    let spinner_for_star_back = spinner.clone();
    let navigation_for_star_back = navigation.clone();
    let library_for_star_back = library_navigation.clone();
    let starred_for_back = starred_row.clone();
    let episodes_menu_for_star_back = episodes_menu.clone();
    let back_to_starred: Rc<dyn Fn()> = Rc::new(move || {
        pages_for_star_back.set_transition_type(gtk::StackTransitionType::SlideRight);
        pages_for_star_back.set_visible_child_name("starred");
        navigation_for_star_back.unselect_all();
        library_for_star_back.select_row(Some(&starred_for_back));
        episodes_menu_for_star_back.set_visible(false);
        title_for_star_back.set_text("Starred");
        spinner_for_star_back.stop();
        spinner_for_star_back.set_opacity(0.0);
    });
    let discovery = discovery::page(
        &entry, &page_back, &podcast_menu, &pages, status, store.clone(), artwork.clone(), playback.clone(), downloads.clone(), toast.clone(),
        follow_toast,
        back_to_following,
        back_to_starred,
    );
    pages.add_named(&discovery.widget, Some("discover"));
    let pages_for_search = pages.clone();
    let navigation_for_search = navigation.clone();
    let discover_for_search = discover_row.clone();
    let search_for_submit = search_button.clone();
    let library_for_search = library_navigation.clone();
    let title_for_search = title.clone();
    let spinner_for_search = spinner.clone();
    entry.connect_activate(move |entry| {
        if !entry.text().trim().is_empty() {
            pages_for_search.set_transition_type(gtk::StackTransitionType::SlideLeft);
            pages_for_search.set_visible_child_name("discover");
            navigation_for_search.select_row(Some(&discover_for_search));
            library_for_search.unselect_all();
            title_for_search.set_text("Search results");
            spinner_for_search.stop();
            spinner_for_search.set_opacity(0.0);
            search_for_submit.set_active(false);
        }
    });
    let open_followed = discovery.open_followed.clone();
    let pages_for_open_followed = pages.clone();
    let title_for_open_followed = title.clone();
    let spinner_for_open_followed = spinner.clone();
    let episodes_menu_for_open_followed = episodes_menu.clone();
    let open_from_following: Rc<dyn Fn(crate::discovery_service::Show)> = Rc::new(move |show| {
        episodes_menu_for_open_followed.set_visible(false);
        open_followed(show.clone());
        pages_for_open_followed.set_transition_type(gtk::StackTransitionType::SlideLeft);
        pages_for_open_followed.set_visible_child_name("discover");
        title_for_open_followed.set_text(&show.title);
        spinner_for_open_followed.stop();
        spinner_for_open_followed.set_opacity(0.0);
    });
    let open_starred = discovery.open_starred.clone();
    let pages_for_open_starred = pages.clone();
    let title_for_open_starred = title.clone();
    let spinner_for_open_starred = spinner.clone();
    let episodes_menu_for_open_starred = episodes_menu.clone();
    let open_from_starred: Rc<dyn Fn(crate::discovery_service::Show)> = Rc::new(move |show| {
        episodes_menu_for_open_starred.set_visible(false);
        open_starred(show.clone());
        pages_for_open_starred.set_transition_type(gtk::StackTransitionType::SlideLeft);
        pages_for_open_starred.set_visible_child_name("discover");
        title_for_open_starred.set_text(&show.title);
        spinner_for_open_starred.stop();
        spinner_for_open_starred.set_opacity(0.0);
    });
    pages.add_named(&following::page(store.clone(), artwork.clone(), open_from_following.clone()), Some("following"));
    let weak_pages = pages.downgrade();
    let weak_title = title.downgrade();
    let weak_spinner = spinner.downgrade();
    let episodes_busy: Rc<dyn Fn(bool)> = Rc::new(move |loading| {
        let (Some(pages), Some(title), Some(spinner)) = (
            weak_pages.upgrade(), weak_title.upgrade(), weak_spinner.upgrade(),
        ) else {
            return;
        };
        if pages.visible_child_name().as_deref() != Some("new-episodes") {
            return;
        }
        title.set_text("New Episodes");
        if loading {
            spinner.start();
            spinner.set_opacity(1.0);
        } else {
            spinner.stop();
            spinner.set_opacity(0.0);
        }
    });
    let new_page = new_episodes::page(
        store.clone(), playback.clone(), downloads.clone(), toast.clone(), episodes_busy,
        open_from_following.clone(),
    );
    let restore_new_episodes = new_page.restore.clone();
    pages.add_named(&new_page.widget, Some("new-episodes"));
    let starred_page = starred::page(store.clone(), artwork.clone(), open_from_starred);
    let reset_starred = starred_page.refresh.clone();
    pages.add_named(&starred_page.widget, Some("starred"));
    let downloaded_page = downloaded::page(store.clone(), playback.clone(), downloads.clone(), toast.clone());
    let refresh_downloaded = downloaded_page.refresh.clone();
    pages.add_named(&downloaded_page.widget, Some("downloaded"));
    let history_page = history::page(
        store.clone(), playback.clone(), downloads.clone(), toast.clone(), open_from_following.clone(),
    );
    let refresh_history = history_page.refresh.clone();
    pages.add_named(&history_page.widget, Some("history"));
    setup_episodes_menu(window, app, &pages, &episodes_menu, new_page.refresh, store, toast.clone());
    setup_main_menu(window, app, &main_menu, settings.clone(), toast.clone());
    let episodes_menu_for_page = episodes_menu.clone();
    pages.connect_visible_child_name_notify(move |pages| {
        episodes_menu_for_page.set_visible(pages.visible_child_name().as_deref() == Some("new-episodes"));
    });
    pages.set_visible_child_name("discover");
    main.append(&pages);
    split.set_content(Some(&adw::NavigationPage::new(&main, "Discover")));

    let pages_for_navigation = pages.clone();
    let title_for_navigation = title.clone();
    let spinner_for_navigation = spinner.clone();
    let split_for_navigation = split.clone();
    let reset_discovery = discovery.reset.clone();
    let search_for_navigation = search_button.clone();
    let back_for_navigation = page_back.clone();
    let menu_for_navigation = podcast_menu.clone();
    let episodes_menu_for_navigation = episodes_menu.clone();
    let library_for_navigation = library_navigation.clone();
    navigation.connect_row_activated(move |list, row| {
        list.select_row(Some(row));
        library_for_navigation.unselect_all();
        match row.index() {
            0 => {
                reset_discovery();
                pages_for_navigation.set_transition_type(gtk::StackTransitionType::SlideRight);
                pages_for_navigation.set_visible_child_name("discover");
                title_for_navigation.set_text("Discover");
            }
            1 => {
                pages_for_navigation.set_visible_child_name("following");
                search_for_navigation.set_active(false);
                back_for_navigation.set_visible(false);
                menu_for_navigation.set_visible(false);
                title_for_navigation.set_text("Following");
            }
            _ => {
                pages_for_navigation.set_visible_child_name("new-episodes");
                search_for_navigation.set_active(false);
                back_for_navigation.set_visible(false);
                menu_for_navigation.set_visible(false);
                title_for_navigation.set_text("New Episodes");
                let pages = pages_for_navigation.clone();
                let restore = restore_new_episodes.clone();
                gtk::glib::timeout_add_local(std::time::Duration::from_millis(30), move || {
                    if pages.visible_child_name().as_deref() != Some("new-episodes") {
                        return gtk::glib::ControlFlow::Break;
                    }
                    if pages.is_transition_running() {
                        return gtk::glib::ControlFlow::Continue;
                    }
                    restore();
                    gtk::glib::ControlFlow::Break
                });
            }
        }
        episodes_menu_for_navigation.set_visible(row.index() == 2);
        if row.index() != 2 {
            spinner_for_navigation.stop();
            spinner_for_navigation.set_opacity(0.0);
        }
        split_for_navigation.set_show_content(true);
    });

    let pages_for_library = pages.clone();
    let title_for_library = title.clone();
    let spinner_for_library = spinner.clone();
    let search_for_library = search_button.clone();
    let back_for_library = page_back.clone();
    let menu_for_library = podcast_menu.clone();
    let episodes_menu_for_library = episodes_menu.clone();
    let navigation_for_library = navigation.clone();
    let split_for_library = split.clone();
    let refresh_downloaded_for_library = refresh_downloaded.clone();
    library_navigation.connect_row_activated(move |list, row| {
        list.select_row(Some(row));
        navigation_for_library.unselect_all();
        search_for_library.set_active(false);
        back_for_library.set_visible(false);
        menu_for_library.set_visible(false);
        episodes_menu_for_library.set_visible(false);
        spinner_for_library.stop();
        spinner_for_library.set_opacity(0.0);
        pages_for_library.set_transition_type(gtk::StackTransitionType::SlideLeft);
        if row.index() == 0 {
            reset_starred();
            pages_for_library.set_visible_child_name("starred");
            title_for_library.set_text("Starred");
        } else if row.index() == 1 {
            refresh_downloaded_for_library();
            pages_for_library.set_visible_child_name("downloaded");
            title_for_library.set_text("Downloaded");
        } else {
            refresh_history();
            pages_for_library.set_visible_child_name("history");
            title_for_library.set_text("History");
        }
        split_for_library.set_show_content(true);
    });

    let sidebar_header_for_breakpoint = sidebar_header.clone();
    let show_sidebar_for_breakpoint = show_sidebar.clone();
    let back_for_breakpoint = page_back.clone();
    split.connect_collapsed_notify(move |split| {
        sidebar_header_for_breakpoint.set_show_end_title_buttons(split.is_collapsed());
        show_sidebar_for_breakpoint.set_visible(split.is_collapsed() && !back_for_breakpoint.is_visible());
    });
    let show_sidebar_for_back = show_sidebar.clone();
    let split_for_back_visibility = split.clone();
    page_back.connect_visible_notify(move |back| {
        show_sidebar_for_back.set_visible(split_for_back_visibility.is_collapsed() && !back.is_visible());
    });

    let adaptive = adw::BreakpointBin::new();
    adaptive.set_size_request(320, 400);
    adaptive.set_vexpand(true);
    adaptive.set_child(Some(&split));
    let compact = adw::Breakpoint::new(adw::BreakpointCondition::new_length(
        adw::BreakpointConditionLengthType::MaxWidth,
        720.0,
        adw::LengthUnit::Sp,
    ));
    compact.add_setter(&split, "collapsed", Some(&true.to_value()));
    adaptive.add_breakpoint(compact);
    let layout = gtk::Box::new(gtk::Orientation::Vertical, 0);
    layout.append(&adaptive);
    let weak_overlay = overlay.downgrade();
    let open_panel: Rc<dyn Fn()> = Rc::new(move || {
        if let Some(overlay) = weak_overlay.upgrade() {
            overlay.set_show_sidebar(true);
        }
    });
    let weak_overlay = overlay.downgrade();
    let close_panel: Rc<dyn Fn()> = Rc::new(move || {
        if let Some(overlay) = weak_overlay.upgrade() {
            overlay.set_show_sidebar(false);
        }
    });
    layout.append(&player::bar(playback.clone(), artwork.clone(), settings.clone(), toast.clone(), open_panel));
    overlay.set_content(Some(&layout));
    overlay.set_sidebar(Some(&library_panel::panel(playback, toast, close_panel)));
    toast_overlay.set_child(Some(&overlay));
    toast_overlay.upcast()
}

pub(super) fn queue_button_with_indicator(
    button: &gtk::Button,
    playback: &Rc<PlaybackController>,
) {
    let Some(icon) = button.child() else { return };
    button.set_child(None::<&gtk::Widget>);
    let overlay = gtk::Overlay::new();
    overlay.set_size_request(16, 16);
    overlay.set_halign(gtk::Align::Center);
    overlay.set_valign(gtk::Align::Center);
    overlay.set_child(Some(&icon));
    let indicator = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    indicator.add_css_class("queue-indicator");
    indicator.set_size_request(6, 6);
    indicator.set_halign(gtk::Align::End);
    indicator.set_valign(gtk::Align::Start);
    indicator.set_can_target(false);
    indicator.set_visible(!playback.queue().is_empty());
    overlay.add_overlay(&indicator);
    button.set_child(Some(&overlay));
    let weak_indicator = indicator.downgrade();
    let weak_playback = Rc::downgrade(playback);
    playback.watch(move || {
        let (Some(indicator), Some(playback)) = (weak_indicator.upgrade(), weak_playback.upgrade()) else {
            return false;
        };
        indicator.set_visible(!playback.queue().is_empty());
        true
    });
}

fn navigation_row(icon: &str, label: &str) -> gtk::ListBoxRow {
    let row = gtk::ListBoxRow::new();
    let content = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    content.set_margin_start(6);
    content.set_margin_end(6);
    let image = gtk::Image::from_icon_name(icon);
    image.add_css_class("sidebar-icon");
    image.set_margin_end(8);
    content.append(&image);
    let text = gtk::Label::new(Some(label));
    text.set_halign(gtk::Align::Start);
    text.set_hexpand(true);
    text.set_xalign(0.0);
    text.set_ellipsize(gtk::pango::EllipsizeMode::End);
    content.append(&text);
    row.set_child(Some(&content));
    row
}

fn menu_button() -> gtk::MenuButton {
    gtk::MenuButton::builder()
        .icon_name("open-menu-symbolic")
        .tooltip_text("Main menu")
        .build()
}

fn setup_main_menu(
    window: &adw::ApplicationWindow,
    app: &adw::Application,
    button: &gtk::MenuButton,
    settings: Rc<crate::settings::AppSettings>,
    toast: Rc<dyn Fn(&str)>,
) {
    let action = gtk::gio::SimpleAction::new("show-settings", None);
    let weak_window = window.downgrade();
    action.connect_activate(move |_, _| {
        if let Some(window) = weak_window.upgrade() {
            settings::present(&window, settings.clone(), toast.clone());
        }
    });
    window.add_action(&action);
    app.set_accels_for_action("win.show-settings", &["<Primary>comma"]);
    let menu = gtk::gio::Menu::new();
    menu.append(Some("Settings"), Some("win.show-settings"));
    menu.append(Some("Keyboard Shortcuts"), Some("win.show-shortcuts"));
    menu.append(Some("About Elite Podcasts"), Some("win.show-about"));
    button.set_menu_model(Some(&menu));
}

fn setup_episodes_menu(
    window: &adw::ApplicationWindow,
    app: &adw::Application,
    pages: &gtk::Stack,
    menu: &gtk::MenuButton,
    refresh: Rc<dyn Fn()>,
    store: Rc<FollowingStore>,
    toast: Rc<dyn Fn(&str)>,
) {
    let refresh_action = gtk::gio::SimpleAction::new("refresh-episodes", None);
    let weak_pages = pages.downgrade();
    refresh_action.connect_activate(move |_, _| {
        if weak_pages.upgrade().is_some_and(|pages| {
            pages.visible_child_name().as_deref() == Some("new-episodes")
        }) {
            refresh();
        }
    });
    window.add_action(&refresh_action);
    app.set_accels_for_action("win.refresh-episodes", &["<Primary>r"]);

    let shortcuts_action = gtk::gio::SimpleAction::new("show-shortcuts", None);
    let weak_window = window.downgrade();
    shortcuts_action.connect_activate(move |_, _| {
        if let Some(window) = weak_window.upgrade() {
            show_shortcuts(&window);
        }
    });
    window.add_action(&shortcuts_action);
    app.set_accels_for_action("win.show-shortcuts", &["<Primary>question"]);

    let about_action = gtk::gio::SimpleAction::new("show-about", None);
    let weak_window = window.downgrade();
    about_action.connect_activate(move |_, _| {
        if let Some(window) = weak_window.upgrade() {
            adw::AboutDialog::builder()
                .application_name("Elite Podcasts")
                .application_icon("io.github.elite.Podcasts")
                .version(env!("CARGO_PKG_VERSION"))
                .developer_name("Elite Podcasts contributors")
                .developers(["Elite Podcasts contributors"])
                .comments("Discover, follow, and listen to podcasts.")
                .license_type(gtk::License::Gpl30)
                .build()
                .present(Some(&window));
        }
    });
    window.add_action(&about_action);

    opml_io::install_actions(window, app, store, toast);

    let model = gtk::gio::Menu::new();
    let feed_actions = gtk::gio::Menu::new();
    feed_actions.append(Some("Check for New Episodes"), Some("win.refresh-episodes"));
    feed_actions.append(Some("Import Shows"), Some("win.import-shows"));
    feed_actions.append(Some("Export Shows"), Some("win.export-shows"));
    model.append_section(None, &feed_actions);
    let app_actions = gtk::gio::Menu::new();
    app_actions.append(Some("Keyboard Shortcuts"), Some("win.show-shortcuts"));
    app_actions.append(Some("About Elite Podcasts"), Some("win.show-about"));
    model.append_section(None, &app_actions);
    menu.set_menu_model(Some(&model));
}

fn show_shortcuts(window: &adw::ApplicationWindow) {
    let body = gtk::Box::new(gtk::Orientation::Vertical, 12);
    body.set_margin_top(20);
    body.set_margin_bottom(24);
    body.set_margin_start(24);
    body.set_margin_end(24);
    for (label, keys) in [
        ("Settings", "<Primary>comma"),
        ("Check for New Episodes", "<Primary>r"),
        ("Import Shows", "<Primary>o"),
        ("Export Shows", "<Primary>e"),
        ("Keyboard Shortcuts", "<Primary>question"),
    ] {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 24);
        let title = gtk::Label::new(Some(label));
        title.set_xalign(0.0);
        title.set_hexpand(true);
        row.append(&title);
        let shortcut = gtk::ShortcutLabel::new(keys);
        row.append(&shortcut);
        body.append(&row);
    }
    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&adw::HeaderBar::new());
    toolbar.set_content(Some(&body));
    adw::Dialog::builder()
        .title("Keyboard Shortcuts")
        .content_width(420)
        .content_height(250)
        .child(&toolbar)
        .build()
        .present(Some(window));
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
