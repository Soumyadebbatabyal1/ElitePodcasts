use adw::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::mpsc;
use std::time::Duration;

use super::artwork::ArtworkLoader;
use crate::data::CATEGORIES;
use crate::discovery_service::{self, Episode, FeedDetails, Show};
use crate::following::FollowingStore;
use crate::playback::PlaybackController;

#[derive(Clone)]
enum Request {
    Search(String),
    Category(String),
}

pub(super) struct DiscoveryPage {
    pub widget: gtk::Widget,
    pub reset: Rc<dyn Fn()>,
    pub open_followed: Rc<dyn Fn(Show)>,
}

pub(super) fn page(
    entry: &gtk::SearchEntry,
    back: &gtk::Button,
    menu: &gtk::MenuButton,
    status: Rc<dyn Fn(&str, bool)>,
    store: Rc<FollowingStore>,
    artwork: Rc<ArtworkLoader>,
    playback: Rc<PlaybackController>,
    toast: Rc<dyn Fn(&str)>,
    back_to_following: Rc<dyn Fn()>,
) -> DiscoveryPage {
    let view = gtk::Stack::new();
    view.set_vexpand(true);
    let (website, selected_show, website_action, unsubscribe_action, mark_action, play_action) =
        setup_podcast_menu(menu, store.clone(), toast.clone());
    let origin_following = Rc::new(Cell::new(false));

    let results = gtk::Box::new(gtk::Orientation::Vertical, 0);
    results.set_margin_top(16);
    results.set_margin_bottom(24);
    let listing = gtk::Box::new(gtk::Orientation::Vertical, 0);
    listing.set_visible(false);
    results.append(&listing);
    let results_clamp = adw::Clamp::builder().maximum_size(620).build();
    results_clamp.set_margin_start(16);
    results_clamp.set_margin_end(16);
    results_clamp.set_child(Some(&results));
    let results_scroller = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .child(&results_clamp)
        .build();
    view.add_named(&results_scroller, Some("results"));

    let feed_content = gtk::Box::new(gtk::Orientation::Vertical, 12);
    feed_content.set_margin_top(28);
    feed_content.set_margin_bottom(24);
    feed_content.set_visible(false);
    let feed_artwork = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    feed_artwork.set_halign(gtk::Align::Center);
    feed_artwork.set_margin_bottom(8);
    feed_content.append(&feed_artwork);
    let feed_description = gtk::Label::new(None);
    feed_description.set_halign(gtk::Align::Center);
    feed_description.set_justify(gtk::Justification::Center);
    feed_description.set_wrap(true);
    feed_description.set_max_width_chars(70);
    feed_description.set_visible(false);
    feed_content.append(&feed_description);
    let full_description = Rc::new(RefCell::new(String::new()));
    let full_for_link = full_description.clone();
    feed_description.connect_activate_link(move |label, uri| {
        if uri == "more" {
            show_description_dialog(label, &full_for_link.borrow());
            gtk::glib::Propagation::Stop
        } else {
            gtk::glib::Propagation::Proceed
        }
    });
    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    actions.set_halign(gtk::Align::Center);
    actions.set_margin_top(2);
    actions.set_margin_bottom(8);
    actions.set_visible(false);
    let mark_button = gtk::Button::from_icon_name("audio-headphones-symbolic");
    mark_button.add_css_class("circular");
    mark_button.add_css_class("podcast-round-action");
    mark_button.set_size_request(40, 40);
    mark_button.set_valign(gtk::Align::Center);
    mark_button.set_tooltip_text(Some("Mark all episodes as played"));
    mark_button.set_sensitive(false);
    actions.append(&mark_button);
    let latest_button = gtk::Button::new();
    latest_button.add_css_class("latest-episode-button");
    latest_button.set_tooltip_text(Some("Play the latest episode"));
    latest_button.set_sensitive(false);
    let latest_content = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let latest_icon = gtk::Image::from_icon_name("media-playback-start-symbolic");
    latest_content.append(&latest_icon);
    latest_content.append(&gtk::Label::new(Some("Latest episode")));
    latest_button.set_child(Some(&latest_content));
    actions.append(&latest_button);
    let action_for_latest_button = play_action.clone();
    latest_button.connect_clicked(move |_| action_for_latest_button.activate(None));
    let current_latest = Rc::new(RefCell::new(None::<Episode>));
    let latest_icon_weak = latest_icon.downgrade();
    let latest_for_update = current_latest.clone();
    let selected_for_update = selected_show.clone();
    let playback_for_update = Rc::downgrade(&playback);
    playback.watch(move || {
        let (Some(icon), Some(playback)) = (latest_icon_weak.upgrade(), playback_for_update.upgrade()) else {
            return false;
        };
        let active = selected_for_update.borrow().as_ref().zip(latest_for_update.borrow().as_ref())
            .is_some_and(|(show, episode)| playback.is_current(&show.feed_url, &episode.id) && playback.is_playing());
        icon.set_icon_name(Some(if active {
            "media-playback-pause-symbolic"
        } else {
            "media-playback-start-symbolic"
        }));
        true
    });
    let latest_for_action = current_latest.clone();
    let selected_for_latest = selected_show.clone();
    let playback_for_latest = playback.clone();
    let toast_for_latest = toast.clone();
    play_action.connect_activate(move |_, _| {
        if let (Some(show), Some(episode)) = (
            selected_for_latest.borrow().as_ref(),
            latest_for_action.borrow().as_ref(),
        ) {
            if let Err(error) = playback_for_latest.play_or_toggle(show, episode) {
                toast_for_latest(&error);
            }
        }
    });
    let follow_button = gtk::Button::from_icon_name("list-add-symbolic");
    follow_button.add_css_class("circular");
    follow_button.add_css_class("podcast-round-action");
    follow_button.set_size_request(40, 40);
    follow_button.set_valign(gtk::Align::Center);
    follow_button.set_tooltip_text(Some("Add to Following"));
    actions.append(&follow_button);
    feed_content.append(&actions);
    let episodes = gtk::Box::new(gtk::Orientation::Vertical, 0);
    episodes.set_margin_top(8);
    feed_content.append(&episodes);
    let current_episodes = Rc::new(RefCell::new(Vec::<String>::new()));
    let store_for_mark = store.clone();
    let selected_for_mark = selected_show.clone();
    let episode_ids_for_mark = current_episodes.clone();
    let toast_for_mark = toast.clone();
    mark_action.connect_activate(move |_, _| {
        let Some(show) = selected_for_mark.borrow().clone() else {
            return;
        };
        let ids = episode_ids_for_mark.borrow();
        match store_for_mark.mark_played(&show.feed_url, &ids) {
            Ok(added) => {
                toast_for_mark(if added == 0 {
                    "Episodes were already marked as played"
                } else {
                    "Episodes marked as played"
                });
            }
            Err(message) => toast_for_mark(&message),
        }
    });
    let action_for_mark_button = mark_action.clone();
    mark_button.connect_clicked(move |_| action_for_mark_button.activate(None));
    let selected_for_follow = selected_show.clone();
    let store_for_follow = store.clone();
    let toast_for_follow = toast.clone();
    let menu_unsubscribe_for_follow = unsubscribe_action.clone();
    follow_button.connect_clicked(move |_| {
        let Some(show) = selected_for_follow.borrow().clone() else {
            return;
        };
        if store_for_follow.contains(&show.feed_url) {
            match store_for_follow.remove(&show.feed_url) {
                Ok(true) => {
                    menu_unsubscribe_for_follow.set_enabled(false);
                    toast_for_follow(&format!("Unsubscribed from {}", show.title));
                }
                Ok(false) => {}
                Err(message) => toast_for_follow(&message),
            }
        } else {
            match store_for_follow.add(&show) {
                Ok(true) => {
                    menu_unsubscribe_for_follow.set_enabled(true);
                    toast_for_follow(&format!("Added {} to Following", show.title));
                }
                Ok(false) => {}
                Err(message) => toast_for_follow(&message),
            }
        }
    });
    let weak_follow = follow_button.downgrade();
    let weak_store = Rc::downgrade(&store);
    let selected_for_watch = selected_show.clone();
    store.watch(move || {
        if let (Some(button), Some(store)) = (weak_follow.upgrade(), weak_store.upgrade()) {
            if let Some(show) = selected_for_watch.borrow().as_ref() {
                update_follow_button(&button, store.contains(&show.feed_url));
            }
            true
        } else {
            false
        }
    });
    let feed_clamp = adw::Clamp::builder().maximum_size(560).build();
    feed_clamp.set_margin_start(16);
    feed_clamp.set_margin_end(16);
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
    let menu_for_back = menu.clone();
    let origin_for_back = origin_following.clone();
    back.connect_clicked(move |_| {
        generation_for_back.set(generation_for_back.get().wrapping_add(1));
        view_for_back.set_transition_type(gtk::StackTransitionType::SlideRight);
        if view_for_back.visible_child_name().as_deref() == Some("feed") {
            menu_for_back.set_visible(false);
            if origin_for_back.replace(false) {
                back_for_click.set_visible(false);
                back_to_following();
            } else {
                view_for_back.set_visible_child_name("results");
                status_for_back(&result_title_for_back.borrow(), false);
                back_for_click.set_tooltip_text(Some("Back to Discover"));
            }
        } else {
            view_for_back.set_visible_child_name("home");
            status_for_back("Discover", false);
            back_for_click.set_visible(false);
        }
    });
    let open_feed: Rc<dyn Fn(Show, bool)> = {
        let view = view.clone();
        let generation = generation.clone();
        let status = status.clone();
        let artwork = artwork.clone();
        let back = back.clone();
        let feed_content = feed_content.clone();
        let full_description = full_description.clone();
        let actions = actions.clone();
        let follow_button = follow_button.clone();
        let mark_button = mark_button.clone();
        let current_episodes = current_episodes.clone();
        let mark_action = mark_action.clone();
        let play_action = play_action.clone();
        let latest_button = latest_button.clone();
        let current_latest = current_latest.clone();
        let playback = playback.clone();
        let toast = toast.clone();
        let menu = menu.clone();
        let website = website.clone();
        let selected_show = selected_show.clone();
        let website_action = website_action.clone();
        let unsubscribe_action = unsubscribe_action.clone();
        let store = store.clone();
        Rc::new(move |show, animate: bool| {
            status(&show.title, true);
            *website.borrow_mut() = None;
            *selected_show.borrow_mut() = Some(show.clone());
            website_action.set_enabled(false);
            unsubscribe_action.set_enabled(store.contains(&show.feed_url));
            menu.set_visible(true);
            feed_content.set_visible(false);
            clear(&feed_artwork);
            feed_description.set_text("");
            feed_description.set_visible(false);
            full_description.borrow_mut().clear();
            actions.set_visible(false);
            update_follow_button(&follow_button, store.contains(&show.feed_url));
            mark_button.set_sensitive(false);
            mark_action.set_enabled(false);
            current_episodes.borrow_mut().clear();
            *current_latest.borrow_mut() = None;
            play_action.set_enabled(false);
            latest_button.set_sensitive(false);
            clear(&episodes);
            view.set_transition_type(if animate {
                gtk::StackTransitionType::SlideLeft
            } else {
                gtk::StackTransitionType::None
            });
            view.set_visible_child_name("feed");
            back.set_visible(true);
            back.set_tooltip_text(Some("Back to results"));

            let id = generation.get().wrapping_add(1);
            generation.set(id);
            let (tx, rx) = mpsc::channel();
            let fallback_title = show.title.clone();
            let fallback_artwork = show.artwork_urls.clone();
            let show_for_feed = show.clone();
            std::thread::spawn(move || {
                let _ = tx.send(discovery_service::feed(&show.feed_url));
            });
            let generation = generation.clone();
            let episodes = episodes.clone();
            let feed_description = feed_description.clone();
            let feed_artwork = feed_artwork.clone();
            let feed_content = feed_content.clone();
            let full_description = full_description.clone();
            let actions = actions.clone();
            let mark_button = mark_button.clone();
            let mark_action = mark_action.clone();
            let current_episodes = current_episodes.clone();
            let status = status.clone();
            let artwork = artwork.clone();
            let website = website.clone();
            let website_action = website_action.clone();
            let selected_show = selected_show.clone();
            let play_action = play_action.clone();
            let latest_button = latest_button.clone();
            let current_latest = current_latest.clone();
            let playback = playback.clone();
            let toast = toast.clone();
            gtk::glib::timeout_add_local(Duration::from_millis(100), move || {
                if generation.get() != id {
                    return gtk::glib::ControlFlow::Break;
                }
                match rx.try_recv() {
                    Ok(answer) => {
                        match answer {
                            Ok(feed) => {
                                *website.borrow_mut() = feed.website.clone();
                                website_action.set_enabled(feed.website.is_some());
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
                                let full_description = full_description.clone();
                                let actions = actions.clone();
                                let mark_button = mark_button.clone();
                                let mark_action = mark_action.clone();
                                let current_episodes = current_episodes.clone();
                                let artwork_for_feed = artwork.clone();
                                let status = status.clone();
                                let show_for_feed = show_for_feed.clone();
                                let selected_show = selected_show.clone();
                                let play_action = play_action.clone();
                                let latest_button = latest_button.clone();
                                let current_latest = current_latest.clone();
                                let playback = playback.clone();
                                let toast = toast.clone();
                                let mut artwork_groups = vec![covers.clone()];
                                for episode in &feed.episodes {
                                    if let Some(url) = &episode.artwork_url {
                                        let mut candidates = vec![url.clone()];
                                        candidates.extend(covers.iter().cloned());
                                        artwork_groups.push(candidates);
                                    }
                                }
                                artwork.preload(artwork_groups, move || {
                                    if generation.get() != id {
                                        return;
                                    }
                                    let mut show_for_feed = show_for_feed;
                                    show_for_feed.artwork_urls = covers.clone();
                                    *selected_show.borrow_mut() = Some(show_for_feed.clone());
                                    feed_artwork.append(&artwork_for_feed.cached_widget(&covers, 190));
                                    let ids = feed.all_episode_ids.clone();
                                    *current_episodes.borrow_mut() = ids;
                                    let can_mark = !current_episodes.borrow().is_empty();
                                    mark_button.set_sensitive(can_mark);
                                    mark_action.set_enabled(can_mark);
                                    *current_latest.borrow_mut() = feed.episodes.iter()
                                        .find(|episode| episode.audio_url.is_some())
                                        .cloned();
                                    let can_play = current_latest.borrow().is_some();
                                    play_action.set_enabled(can_play);
                                    latest_button.set_sensitive(can_play);
                                    show_feed(
                                        &feed_description, &full_description, &episodes, feed,
                                        &show_for_feed, artwork_for_feed.clone(), &covers,
                                        playback, toast,
                                    );
                                    actions.set_visible(true);
                                    feed_content.set_visible(true);
                                    status(&title, false);
                                });
                            }
                            Err(message) => {
                                status(&fallback_title, false);
                                show_message(&episodes, "Could not load RSS feed", &message);
                                actions.set_visible(true);
                                feed_content.set_visible(true);
                            }
                        }
                        gtk::glib::ControlFlow::Break
                    }
                    Err(mpsc::TryRecvError::Empty) => gtk::glib::ControlFlow::Continue,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        status(&fallback_title, false);
                        show_message(&episodes, "Could not load RSS feed", "The request ended unexpectedly.");
                        actions.set_visible(true);
                        feed_content.set_visible(true);
                        gtk::glib::ControlFlow::Break
                    }
                }
            });
        })
    };
    let open_followed: Rc<dyn Fn(Show)> = {
        let origin_following = origin_following.clone();
        let open_feed = open_feed.clone();
        Rc::new(move |show| {
            origin_following.set(true);
            open_feed(show, false);
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
        let menu = menu.clone();
        let store = store.clone();
        let toast = toast.clone();
        Rc::new(move |request| {
            let cue_first = matches!(&request, Request::Search(_));
            let title = match &request {
                Request::Search(_) => "Search results".to_string(),
                Request::Category(name) => name.clone(),
            };
            *result_title.borrow_mut() = title.clone();
            status(&title, true);
            menu.set_visible(false);
            clear(&listing);
            listing.set_visible(false);
            view.set_transition_type(if view.is_mapped() {
                gtk::StackTransitionType::SlideLeft
            } else {
                gtk::StackTransitionType::None
            });
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
            let store = store.clone();
            let toast = toast.clone();
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
                                let store = store.clone();
                                let toast = toast.clone();
                                artwork.preload(covers, move || {
                                    if generation.get() != id {
                                        return;
                                    }
                                    clear(&listing);
                                    show_results(&listing, shows, open_feed, &artwork_for_results, cue_first, &store, toast);
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
    content.set_margin_top(16);
    content.set_margin_bottom(24);
    content.append(&categories(request.clone()));

    let clamp = adw::Clamp::builder().maximum_size(800).build();
    clamp.set_margin_start(16);
    clamp.set_margin_end(16);
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
            view_for_clear.set_transition_type(if view_for_clear.is_mapped() {
                gtk::StackTransitionType::SlideRight
            } else {
                gtk::StackTransitionType::None
            });
            view_for_clear.set_visible_child_name("home");
            status_for_clear("Discover", false);
            back_for_clear.set_visible(false);
        }
    });
    let reset_view = view.clone();
    let reset_generation = generation.clone();
    let reset_status = status.clone();
    let reset_back = back.clone();
    let reset_menu = menu.clone();
    let reset_origin = origin_following.clone();
    let reset: Rc<dyn Fn()> = Rc::new(move || {
        reset_generation.set(reset_generation.get().wrapping_add(1));
        reset_view.set_transition_type(if reset_view.is_mapped() {
            gtk::StackTransitionType::SlideRight
        } else {
            gtk::StackTransitionType::None
        });
        reset_view.set_visible_child_name("home");
        reset_status("Discover", false);
        reset_back.set_visible(false);
        reset_menu.set_visible(false);
        reset_origin.set(false);
    });
    DiscoveryPage { widget: view.upcast(), reset, open_followed }
}

fn clear(container: &gtk::Box) {
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }
}

fn show_message(container: &gtk::Box, title: &str, message: &str) {
    let content = gtk::Box::new(gtk::Orientation::Vertical, 8);
    content.set_halign(gtk::Align::Center);
    content.set_margin_top(20);
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
    open_feed: Rc<dyn Fn(Show, bool)>,
    artwork: &ArtworkLoader,
    cue_first: bool,
    store: &Rc<FollowingStore>,
    toast: Rc<dyn Fn(&str)>,
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
        content.set_margin_start(10);
        content.set_margin_end(10);
        content.set_margin_top(8);
        content.set_margin_bottom(8);
        let layout = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        layout.set_hexpand(true);
        let cover = artwork.cached_widget(&show.artwork_urls, 56);
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
        add.set_valign(gtk::Align::Center);
        update_add_button(&add, store.contains(&show.feed_url));
        let show_for_add = show.clone();
        let store_for_add = store.clone();
        let toast_for_add = toast.clone();
        add.connect_clicked(move |_| {
            if store_for_add.contains(&show_for_add.feed_url) {
                return;
            }
            match store_for_add.add(&show_for_add) {
                Ok(true) => toast_for_add(&format!("Added {} to Following", show_for_add.title)),
                Ok(false) => {}
                Err(message) => toast_for_add(&message),
            }
        });
        let weak_add = add.downgrade();
        let store_for_watch = Rc::downgrade(store);
        let feed_url = show.feed_url.clone();
        store.watch(move || {
            if let (Some(add), Some(store)) = (weak_add.upgrade(), store_for_watch.upgrade()) {
                update_add_button(&add, store.contains(&feed_url));
                true
            } else {
                false
            }
        });
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
                open_feed(show.clone(), true);
            }
        }
    });
    container.append(&list);
}

fn update_add_button(button: &gtk::Button, added: bool) {
    button.set_sensitive(!added);
    button.set_tooltip_text(Some(if added {
        "Already in Following"
    } else {
        "Add to Following"
    }));
    if added {
        button.set_child(Some(&gtk::Label::new(Some("Added"))));
        button.add_css_class("added-button");
    } else {
        let content = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        content.append(&gtk::Image::from_icon_name("list-add-symbolic"));
        content.append(&gtk::Label::new(Some("Add")));
        button.set_child(Some(&content));
        button.remove_css_class("added-button");
    }
}

fn update_follow_button(button: &gtk::Button, followed: bool) {
    button.set_icon_name(if followed {
        "object-select-symbolic"
    } else {
        "list-add-symbolic"
    });
    button.set_tooltip_text(Some(if followed {
        "Unsubscribe"
    } else {
        "Add to Following"
    }));
}

fn show_description_dialog(parent: &gtk::Label, description: &str) {
    let text = gtk::Label::new(Some(description));
    text.set_wrap(true);
    text.set_xalign(0.0);
    text.set_yalign(0.0);
    text.set_valign(gtk::Align::Start);
    text.set_can_focus(false);
    let body = gtk::Box::new(gtk::Orientation::Vertical, 0);
    body.set_margin_top(16);
    body.set_margin_bottom(24);
    body.set_margin_start(36);
    body.set_margin_end(36);
    body.append(&text);
    let scroller = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .child(&body)
        .build();
    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&adw::HeaderBar::new());
    toolbar.set_content(Some(&scroller));
    let dialog = adw::Dialog::builder()
        .title("About this podcast")
        .content_width(440)
        .content_height(360)
        .child(&toolbar)
        .build();
    dialog.present(Some(parent));
}

type PodcastMenuState = (
    Rc<RefCell<Option<String>>>,
    Rc<RefCell<Option<Show>>>,
    gtk::gio::SimpleAction,
    gtk::gio::SimpleAction,
    gtk::gio::SimpleAction,
    gtk::gio::SimpleAction,
);

fn setup_podcast_menu(
    button: &gtk::MenuButton,
    store: Rc<FollowingStore>,
    toast: Rc<dyn Fn(&str)>,
) -> PodcastMenuState {
    button.set_visible(false);
    let model = gtk::gio::Menu::new();
    model.append(Some("Open Website"), Some("podcast.website"));
    model.append(Some("Mark All as Played"), Some("podcast.mark-played"));
    model.append(Some("Play Latest Episode"), Some("podcast.play-latest"));
    let actions = gtk::gio::Menu::new();
    actions.append(Some("Unsubscribe"), Some("podcast.unsubscribe"));
    model.append_section(None, &actions);
    button.set_menu_model(Some(&model));

    let group = gtk::gio::SimpleActionGroup::new();
    let website = Rc::new(RefCell::new(None::<String>));
    let selected_show = Rc::new(RefCell::new(None::<Show>));
    let website_action = gtk::gio::SimpleAction::new("website", None);
    website_action.set_enabled(false);
    let website_for_action = website.clone();
    let toast_for_website = toast.clone();
    website_action.connect_activate(move |_, _| {
        let url = website_for_action.borrow().clone();
        if let Some(url) = url {
            if let Err(error) = gtk::gio::AppInfo::launch_default_for_uri(
                &url,
                None::<&gtk::gio::AppLaunchContext>,
            ) {
                toast_for_website(&format!("Could not open the website: {error}"));
            }
        }
    });
    group.add_action(&website_action);
    let mark_action = gtk::gio::SimpleAction::new("mark-played", None);
    mark_action.set_enabled(false);
    group.add_action(&mark_action);
    let play_action = gtk::gio::SimpleAction::new("play-latest", None);
    play_action.set_enabled(false);
    group.add_action(&play_action);
    let unsubscribe = gtk::gio::SimpleAction::new("unsubscribe", None);
    unsubscribe.set_enabled(false);
    let selected_for_unsubscribe = selected_show.clone();
    unsubscribe.connect_activate(move |action, _| {
        let show = selected_for_unsubscribe.borrow().clone();
        if let Some(show) = show {
            match store.remove(&show.feed_url) {
                Ok(true) => {
                    action.set_enabled(false);
                    toast(&format!("Unsubscribed from {}", show.title));
                }
                Ok(false) => {}
                Err(message) => toast(&message),
            }
        }
    });
    group.add_action(&unsubscribe);
    button.insert_action_group("podcast", Some(&group));
    (website, selected_show, website_action, unsubscribe, mark_action, play_action)
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

fn show_feed(
    description: &gtk::Label,
    full_description: &Rc<RefCell<String>>,
    episodes: &gtk::Box,
    feed: FeedDetails,
    show: &Show,
    artwork: Rc<ArtworkLoader>,
    covers: &[String],
    playback: Rc<PlaybackController>,
    toast: Rc<dyn Fn(&str)>,
) {
    let text = plain_description(&feed.description);
    *full_description.borrow_mut() = text.clone();
    if text.chars().count() > 180 {
        let prefix: String = text.chars().take(180).collect();
        let prefix = prefix.rsplit_once(' ').map_or(prefix.as_str(), |(head, _)| head);
        let escaped = gtk::glib::markup_escape_text(prefix);
        description.set_markup(&format!("{escaped}… <a href=\"more\" class=\"more-link\">More</a>"));
    } else {
        description.set_text(&text);
    }
    description.set_visible(!text.is_empty());
    if feed.episodes.is_empty() {
        show_message(episodes, "No episodes found", "This RSS feed has no recent episodes to show.");
        return;
    }
    let list = gtk::ListBox::new();
    list.add_css_class("boxed-list");
    list.set_selection_mode(gtk::SelectionMode::None);
    list.set_activate_on_single_click(true);
    let all_episodes = feed.episodes;
    let mut over_play_buttons = Vec::with_capacity(all_episodes.len());
    for episode in &all_episodes {
        let row = gtk::ListBoxRow::new();
        let content = gtk::Box::new(gtk::Orientation::Horizontal, 14);
        content.set_margin_top(13);
        content.set_margin_bottom(13);
        content.set_margin_start(12);
        content.set_margin_end(12);
        let mut candidates = Vec::new();
        if let Some(url) = &episode.artwork_url {
            candidates.push(url.clone());
        }
        candidates.extend(covers.iter().cloned());
        let cover = artwork.cached_widget(&candidates, 68);
        cover.set_valign(gtk::Align::Center);
        content.append(&cover);
        let text = gtk::Box::new(gtk::Orientation::Vertical, 3);
        text.set_hexpand(true);
        text.set_valign(gtk::Align::Center);
        let date = gtk::Label::new(Some(&format_episode_date(&episode.date)));
        date.set_xalign(0.0);
        date.add_css_class("caption");
        date.add_css_class("dim-label");
        text.append(&date);
        let title = gtk::Label::new(Some(&episode.title));
        title.set_xalign(0.0);
        title.set_wrap(true);
        title.set_lines(2);
        title.set_ellipsize(gtk::pango::EllipsizeMode::End);
        title.add_css_class("heading");
        text.append(&title);
        let summary = plain_description(&episode.description);
        if !summary.is_empty() {
            let preview = gtk::Label::new(Some(&summary));
            preview.set_xalign(0.0);
            preview.set_wrap(true);
            preview.set_lines(if episode.title.chars().count() > 55 { 1 } else { 2 });
            preview.set_ellipsize(gtk::pango::EllipsizeMode::End);
            preview.add_css_class("caption");
            preview.add_css_class("dim-label");
            text.append(&preview);
        }
        content.append(&text);
        let play_button = gtk::Button::new();
        play_button.add_css_class("episode-play-button");
        play_button.set_valign(gtk::Align::Center);
        let play_content = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        let play_icon = gtk::Image::from_icon_name("media-playback-start-symbolic");
        play_content.append(&play_icon);
        if let Some(seconds) = episode.duration_seconds {
            let minutes = (seconds + 30) / 60;
            play_content.append(&gtk::Label::new(Some(&format!("{minutes} min"))));
        }
        play_button.set_child(Some(&play_content));
        play_button.set_tooltip_text(Some("Play episode"));
        play_button.set_sensitive(episode.audio_url.is_some());
        let feed_url = show.feed_url.clone();
        let episode_id = episode.id.clone();
        update_episode_play_icon(&play_icon, &play_button, &playback, &feed_url, &episode_id);
        let weak_icon = play_icon.downgrade();
        let weak_button = play_button.downgrade();
        let weak_playback = Rc::downgrade(&playback);
        playback.watch(move || {
            let (Some(icon), Some(button), Some(playback)) = (
                weak_icon.upgrade(), weak_button.upgrade(), weak_playback.upgrade(),
            ) else {
                return false;
            };
            update_episode_play_icon(&icon, &button, &playback, &feed_url, &episode_id);
            true
        });
        let over_play = Rc::new(Cell::new(false));
        let motion = gtk::EventControllerMotion::new();
        let entered = over_play.clone();
        motion.connect_enter(move |_, _, _| entered.set(true));
        let left = over_play.clone();
        motion.connect_leave(move |_| left.set(false));
        play_button.add_controller(motion);
        over_play_buttons.push(over_play);
        let show_for_play = show.clone();
        let episode_for_play = episode.clone();
        let playback_for_play = playback.clone();
        let toast_for_play = toast.clone();
        play_button.connect_clicked(move |_| {
            if let Err(error) = playback_for_play.play_or_toggle(&show_for_play, &episode_for_play) {
                toast_for_play(&error);
            }
        });
        content.append(&play_button);
        row.set_child(Some(&content));
        list.append(&row);
    }
    let show_for_dialog = show.clone();
    let artwork_for_dialog = artwork.clone();
    let covers_for_dialog = covers.to_vec();
    list.connect_row_activated(move |_, row| {
        let index = row.index() as usize;
        if !over_play_buttons.get(index).is_some_and(|over| over.get()) {
            if let Some(episode) = all_episodes.get(index) {
                show_episode_dialog(
                    row, &show_for_dialog, episode, &artwork_for_dialog,
                    &covers_for_dialog, playback.clone(), toast.clone(),
                );
            }
        }
    });
    episodes.append(&list);
}

fn update_episode_play_icon(
    icon: &gtk::Image,
    button: &gtk::Button,
    playback: &PlaybackController,
    feed_url: &str,
    episode_id: &str,
) {
    let active = playback.is_current(feed_url, episode_id) && playback.is_playing();
    icon.set_icon_name(Some(if active {
        "media-playback-pause-symbolic"
    } else {
        "media-playback-start-symbolic"
    }));
    button.set_tooltip_text(Some(if active { "Pause episode" } else { "Play episode" }));
}

fn show_episode_dialog(
    parent: &gtk::ListBoxRow,
    show: &Show,
    episode: &Episode,
    artwork: &ArtworkLoader,
    covers: &[String],
    playback: Rc<PlaybackController>,
    toast: Rc<dyn Fn(&str)>,
) {
    let body = gtk::Box::new(gtk::Orientation::Vertical, 16);
    body.set_margin_top(20);
    body.set_margin_bottom(24);
    body.set_margin_start(24);
    body.set_margin_end(24);
    let heading = gtk::Box::new(gtk::Orientation::Horizontal, 16);
    let mut candidates = Vec::new();
    if let Some(url) = &episode.artwork_url {
        candidates.push(url.clone());
    }
    candidates.extend(covers.iter().cloned());
    heading.append(&artwork.cached_widget(&candidates, 88));
    let heading_text = gtk::Box::new(gtk::Orientation::Vertical, 6);
    heading_text.set_valign(gtk::Align::Center);
    let title = gtk::Label::new(Some(&episode.title));
    title.set_xalign(0.0);
    title.set_wrap(true);
    title.add_css_class("title-3");
    heading_text.append(&title);
    let mut details = format_episode_date(&episode.date);
    if let Some(seconds) = episode.duration_seconds {
        details.push_str(&format!(" · {} min", (seconds + 30) / 60));
    }
    let subtitle = gtk::Label::new(Some(&details));
    subtitle.set_xalign(0.0);
    subtitle.add_css_class("dim-label");
    heading_text.append(&subtitle);
    heading.append(&heading_text);
    body.append(&heading);
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

fn format_episode_date(raw: &str) -> String {
    let raw = raw.trim();
    let parts: Vec<&str> = raw.trim_start_matches(|character: char| character.is_ascii_alphabetic() || character == ',')
        .split_whitespace().collect();
    if parts.len() < 4 {
        return raw.to_string();
    }
    let month = match parts[1].to_ascii_lowercase().as_str() {
        "jan" => 1, "feb" => 2, "mar" => 3, "apr" => 4, "may" => 5, "jun" => 6,
        "jul" => 7, "aug" => 8, "sep" => 9, "oct" => 10, "nov" => 11, "dec" => 12,
        _ => return raw.to_string(),
    };
    let day = parts[0].parse::<u8>().unwrap_or(0);
    let year = parts[2].parse::<i32>().unwrap_or(0);
    let zone = parts.get(4).copied().unwrap_or("+0000");
    let offset = if zone.len() == 5 && (zone.starts_with('+') || zone.starts_with('-')) {
        format!("{}:{}", &zone[..3], &zone[3..])
    } else {
        "+00:00".to_string()
    };
    let iso = format!("{year:04}-{month:02}-{day:02}T{}{offset}", parts[3]);
    let Some(published) = gtk::glib::DateTime::from_iso8601(&iso, None).ok()
        .and_then(|date| date.to_local().ok()) else {
        return raw.to_string();
    };
    let Some(now) = gtk::glib::DateTime::now_local().ok() else {
        return raw.to_string();
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
        let format = if published.year() == now.year() { "%e %b" } else { "%e %b %Y" };
        published.format(format).map(|value| value.trim().to_string()).unwrap_or_else(|_| raw.to_string())
    }
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
        .column_spacing(10)
        .row_spacing(10)
        .homogeneous(true)
        .build();

    for (icon, name, color) in CATEGORIES.iter() {
        let tile = gtk::Button::new();
        tile.set_size_request(160, 74);
        tile.set_overflow(gtk::Overflow::Hidden);
        tile.add_css_class("category-tile");
        tile.add_css_class(&format!("category-{color}"));

        let layout = gtk::Overlay::new();
        let surface = gtk::Box::new(gtk::Orientation::Vertical, 0);
        surface.set_size_request(-1, 74);
        layout.set_child(Some(&surface));

        let image = gtk::Image::from_icon_name(icon);
        image.set_pixel_size(80);
        image.set_halign(gtk::Align::End);
        image.set_valign(gtk::Align::End);
        image.set_margin_end(8);
        image.add_css_class("category-icon");
        layout.add_overlay(&image);

        let label = gtk::Label::new(Some(name));
        label.set_halign(gtk::Align::Start);
        label.set_valign(gtk::Align::End);
        label.set_margin_start(14);
        label.set_margin_bottom(9);
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
