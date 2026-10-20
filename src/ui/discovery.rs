use adw::prelude::*;
use gtk::glib::variant::ToVariant;
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::mpsc;
use std::time::Duration;

use super::artwork::ArtworkLoader;
use super::download_manager::DownloadManager;
use super::episode::{format_date, parse_date, plain_description, show_dialog as show_episode_dialog};
use super::episode_row::EpisodeRowControls;
use crate::data::{self, CATEGORIES};
use crate::discovery_service::{self, Episode, FeedDetails, Show};
use crate::following::FollowingStore;
use crate::playback::PlaybackController;

pub(super) struct DiscoveryPage {
    pub widget: gtk::Widget,
    pub reset: Rc<dyn Fn()>,
    pub open_followed: Rc<dyn Fn(Show)>,
    pub open_starred: Rc<dyn Fn(Show)>,
}

#[derive(Clone, Copy, Default)]
struct EpisodeViewPrefs {
    played_only: bool,
    oldest_first: bool,
}

struct EpisodeListControls {
    list: gtk::ListBox,
    empty: gtk::Label,
    played_only: Rc<Cell<bool>>,
    oldest_first: Rc<Cell<bool>>,
    starred_only: bool,
    feed_url: String,
}

fn refresh_episode_visibility(
    rows: &HashMap<String, EpisodeRowControls>,
    controls: &EpisodeListControls,
    store: &FollowingStore,
) {
    let mut visible = 0;
    for (id, row) in rows {
        let show = (!controls.played_only.get() || row.played.get())
            && (!controls.starred_only || store.is_starred(&controls.feed_url, id));
        row.row.set_visible(show);
        visible += usize::from(show);
    }
    controls.list.set_visible(visible > 0);
    controls.empty.set_text(if controls.starred_only {
        if controls.played_only.get() { "No played starred episodes" } else { "No starred episodes" }
    } else {
        "No played episodes"
    });
    controls.empty.set_visible(visible == 0);
}

fn starred_ids_for_show(store: &FollowingStore, feed_url: &str) -> Vec<String> {
    store.starred().unwrap_or_default().into_iter()
        .filter(|item| item.show.feed_url == feed_url)
        .map(|item| item.episode.id)
        .collect()
}

pub(super) fn page(
    entry: &gtk::SearchEntry,
    back: &gtk::Button,
    menu: &gtk::MenuButton,
    outer_pages: &gtk::Stack,
    status: Rc<dyn Fn(&str, bool)>,
    store: Rc<FollowingStore>,
    artwork: Rc<ArtworkLoader>,
    playback: Rc<PlaybackController>,
    downloads: Rc<DownloadManager>,
    toast: Rc<dyn Fn(&str)>,
    follow_toast: Rc<dyn Fn(&str)>,
    back_to_following: Rc<dyn Fn()>,
    back_to_starred: Rc<dyn Fn()>,
) -> DiscoveryPage {
    let view = gtk::Stack::new();
    view.set_vexpand(true);
    let (website, selected_show, website_action, unsubscribe_action, mark_action, play_action,
        showing_action, sorting_action, mark_menu) =
        setup_podcast_menu(menu, store.clone(), toast.clone(), follow_toast.clone());
    let origin_following = Rc::new(Cell::new(false));
    let origin_starred = Rc::new(Cell::new(false));
    let starred_filter = Rc::new(Cell::new(false));

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

    let category_content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    category_content.set_vexpand(true);
    view.add_named(&category_content, Some("category"));

    let feed_content = gtk::Box::new(gtk::Orientation::Vertical, 12);
    feed_content.set_margin_top(28);
    feed_content.set_margin_bottom(24);
    feed_content.set_visible(false);
    let feed_artwork = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    feed_artwork.set_halign(gtk::Align::Center);
    feed_artwork.set_margin_bottom(8);
    feed_content.append(&feed_artwork);
    let feed_metadata = gtk::Label::new(None);
    feed_metadata.add_css_class("podcast-metadata");
    feed_metadata.set_halign(gtk::Align::Center);
    feed_metadata.set_wrap(true);
    feed_metadata.set_justify(gtk::Justification::Center);
    feed_metadata.set_visible(false);
    feed_content.append(&feed_metadata);
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
    let details_button = gtk::Button::from_icon_name("help-about-symbolic");
    details_button.add_css_class("circular");
    details_button.add_css_class("podcast-round-action");
    details_button.set_size_request(40, 40);
    details_button.set_valign(gtk::Align::Center);
    details_button.set_tooltip_text(Some("Podcast details"));
    details_button.set_sensitive(false);
    actions.append(&details_button);
    let current_feed = Rc::new(RefCell::new(None::<FeedDetails>));
    let feed_for_details = current_feed.clone();
    let show_for_details = selected_show.clone();
    details_button.connect_clicked(move |button| {
        if let (Some(feed), Some(show)) = (feed_for_details.borrow().as_ref(), show_for_details.borrow().as_ref()) {
            show_podcast_details(button, show, feed);
        }
    });
    let follow_button = gtk::Button::new();
    follow_button.add_css_class("podcast-follow-button");
    follow_button.set_valign(gtk::Align::Center);
    actions.append(&follow_button);
    let latest_button = gtk::Button::new();
    latest_button.add_css_class("circular");
    latest_button.add_css_class("podcast-round-action");
    latest_button.set_tooltip_text(Some("Play the latest episode"));
    latest_button.set_sensitive(false);
    let latest_icon = gtk::Image::from_icon_name("media-playback-start-symbolic");
    latest_button.set_child(Some(&latest_icon));
    latest_button.set_size_request(40, 40);
    latest_button.set_valign(gtk::Align::Center);
    actions.append(&latest_button);
    let action_for_latest_button = play_action.clone();
    latest_button.connect_clicked(move |_| action_for_latest_button.activate(None));
    let current_latest = Rc::new(RefCell::new(None::<Episode>));
    let all_played = Rc::new(Cell::new(false));
    let latest_icon_weak = latest_icon.downgrade();
    let latest_button_weak = latest_button.downgrade();
    let latest_for_update = current_latest.clone();
    let selected_for_update = selected_show.clone();
    let playback_for_update = Rc::downgrade(&playback);
    let all_played_for_update = all_played.clone();
    playback.watch(move || {
        let (Some(icon), Some(button), Some(playback)) = (latest_icon_weak.upgrade(), latest_button_weak.upgrade(), playback_for_update.upgrade()) else {
            return false;
        };
        let active = selected_for_update.borrow().as_ref().zip(latest_for_update.borrow().as_ref())
            .is_some_and(|(show, episode)| playback.is_current(&show.feed_url, &episode.id) && playback.is_playing());
        update_latest_button(&icon, &button, active, all_played_for_update.get());
        true
    });
    let latest_for_action = current_latest.clone();
    let selected_for_latest = selected_show.clone();
    let playback_for_latest = playback.clone();
    let toast_for_latest = toast.clone();
    let all_played_for_latest = all_played.clone();
    play_action.connect_activate(move |_, _| {
        if let (Some(show), Some(episode)) = (
            selected_for_latest.borrow().as_ref(),
            latest_for_action.borrow().as_ref(),
        ) {
            let result = if all_played_for_latest.get() {
                playback_for_latest.replay_episode(show, episode)
            } else {
                playback_for_latest.play_or_toggle(show, episode)
            };
            if let Err(error) = result {
                toast_for_latest(&error);
            }
        }
    });
    feed_content.append(&actions);
    let episodes = gtk::Box::new(gtk::Orientation::Vertical, 0);
    episodes.set_margin_top(8);
    feed_content.append(&episodes);
    let current_episodes = Rc::new(RefCell::new(Vec::<String>::new()));
    let episode_rows = Rc::new(RefCell::new(HashMap::<String, EpisodeRowControls>::new()));
    let episode_list_controls = Rc::new(RefCell::new(None::<EpisodeListControls>));
    let episode_view_prefs = Rc::new(RefCell::new(HashMap::<String, EpisodeViewPrefs>::new()));
    let selected_for_showing = selected_show.clone();
    let prefs_for_showing = episode_view_prefs.clone();
    let rows_for_showing = episode_rows.clone();
    let list_for_showing = episode_list_controls.clone();
    let store_for_showing = store.clone();
    showing_action.connect_activate(move |action, parameter| {
        let Some(value) = parameter.and_then(|parameter| parameter.str()) else { return };
        if !matches!(value, "all" | "played") { return; }
        action.set_state(&value.to_variant());
        if let Some(show) = selected_for_showing.borrow().as_ref() {
            prefs_for_showing.borrow_mut().entry(show.feed_url.clone()).or_default().played_only = value == "played";
        }
        if let Some(controls) = list_for_showing.borrow().as_ref() {
            controls.played_only.set(value == "played");
            refresh_episode_visibility(&rows_for_showing.borrow(), controls, &store_for_showing);
        }
    });
    let selected_for_sorting = selected_show.clone();
    let prefs_for_sorting = episode_view_prefs.clone();
    let list_for_sorting = episode_list_controls.clone();
    sorting_action.connect_activate(move |action, parameter| {
        let Some(value) = parameter.and_then(|parameter| parameter.str()) else { return };
        if !matches!(value, "newest" | "oldest") { return; }
        action.set_state(&value.to_variant());
        if let Some(show) = selected_for_sorting.borrow().as_ref() {
            prefs_for_sorting.borrow_mut().entry(show.feed_url.clone()).or_default().oldest_first = value == "oldest";
        }
        if let Some(controls) = list_for_sorting.borrow().as_ref() {
            controls.oldest_first.set(value == "oldest");
            controls.list.invalidate_sort();
        }
    });
    let store_for_mark = store.clone();
    let selected_for_mark = selected_show.clone();
    let episode_ids_for_mark = current_episodes.clone();
    let toast_for_mark = toast.clone();
    let all_played_for_mark = all_played.clone();
    let menu_for_mark = menu.clone();
    let playback_for_mark = playback.clone();
    mark_action.connect_activate(move |_, _| {
        let Some(show) = selected_for_mark.borrow().clone() else {
            return;
        };
        if all_played_for_mark.get() {
            match store_for_mark.mark_all_unplayed(&show.feed_url) {
                Ok(_) => toast_for_mark("Episodes marked as unplayed"),
                Err(message) => toast_for_mark(&message),
            }
        } else {
            let ids = episode_ids_for_mark.borrow().clone();
            let already_played = match store_for_mark.played_ids(&show.feed_url) {
                Ok(played) => played,
                Err(message) => {
                    toast_for_mark(&message);
                    return;
                }
            };
            let remaining: Vec<String> = ids.into_iter()
                .filter(|id| !already_played.contains(id))
                .collect();
            if remaining.is_empty() {
                return;
            }
            let scope = if already_played.is_empty() { "all" } else { "remaining" };
            let body = format!(
                "Are you sure you want to mark {scope} episodes as played and reset their saved listening progress?"
            );
            let dialog = adw::AlertDialog::new(Some("Mark episodes as played?"), Some(&body));
            dialog.add_responses(&[("cancel", "Cancel"), ("mark", "Mark as Played")]);
            dialog.set_close_response("cancel");
            dialog.set_default_response(Some("cancel"));
            dialog.set_response_appearance("mark", adw::ResponseAppearance::Destructive);
            let store = store_for_mark.clone();
            let playback = playback_for_mark.clone();
            let toast = toast_for_mark.clone();
            dialog.connect_response(None, move |_, response| {
                if response != "mark" {
                    return;
                }
                if remaining.iter().any(|id| playback.is_current(&show.feed_url, id)) {
                    if let Err(message) = playback.stop() {
                        toast(&message);
                        return;
                    }
                }
                match store.mark_played_and_reset(&show.feed_url, &remaining) {
                    Ok(_) => toast("Episodes marked as played"),
                    Err(message) => toast(&message),
                }
            });
            dialog.present(Some(&menu_for_mark));
        }
    });
    let selected_for_follow = selected_show.clone();
    let store_for_follow = store.clone();
    let toast_for_follow = toast.clone();
    let follow_toast_for_button = follow_toast.clone();
    let menu_unsubscribe_for_follow = unsubscribe_action.clone();
    follow_button.connect_clicked(move |_| {
        let Some(show) = selected_for_follow.borrow().clone() else {
            return;
        };
        if store_for_follow.contains(&show.feed_url) {
            match store_for_follow.remove(&show.feed_url) {
                Ok(true) => {
                    menu_unsubscribe_for_follow.set_enabled(false);
                    follow_toast_for_button(&format!("Unsubscribed from {}", show.title));
                }
                Ok(false) => {}
                Err(message) => toast_for_follow(&message),
            }
        } else {
            match store_for_follow.add(&show) {
                Ok(true) => {
                    menu_unsubscribe_for_follow.set_enabled(true);
                    follow_toast_for_button(&format!("Added {} to Following", show.title));
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
    let weak_store_for_played = Rc::downgrade(&store);
    let weak_latest_for_played = latest_button.downgrade();
    let weak_icon_for_played = latest_icon.downgrade();
    let selected_for_played = selected_show.clone();
    let ids_for_played = current_episodes.clone();
    let starred_for_played = starred_filter.clone();
    let rows_for_played = episode_rows.clone();
    let list_for_played = episode_list_controls.clone();
    let menu_for_played = mark_menu.clone();
    let all_for_played = all_played.clone();
    let latest_for_played = current_latest.clone();
    let playback_for_played = Rc::downgrade(&playback);
    store.watch(move || {
        let (Some(store), Some(button), Some(icon), Some(playback)) = (
            weak_store_for_played.upgrade(), weak_latest_for_played.upgrade(),
            weak_icon_for_played.upgrade(), playback_for_played.upgrade(),
        ) else {
            return false;
        };
        if starred_for_played.get() {
            if let Some(show) = selected_for_played.borrow().as_ref() {
                *ids_for_played.borrow_mut() = starred_ids_for_show(&store, &show.feed_url);
            }
        }
        update_played_ui(
            &store, &selected_for_played, &ids_for_played, &rows_for_played,
            &list_for_played,
            &menu_for_played, &all_for_played, &latest_for_played,
            &icon, &button, &playback,
        );
        true
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
    let feed_state = gtk::Stack::new();
    feed_state.set_vexpand(true);
    feed_state.add_named(&feed_scroller, Some("content"));
    let missing_feed = adw::StatusPage::builder()
        .icon_name("network-error-symbolic")
        .title("Podcast feed not found (404)")
        .build();
    feed_state.add_named(&missing_feed, Some("not-found"));
    let interrupted_feed = adw::StatusPage::builder()
        .icon_name("network-error-symbolic")
        .title("Connection interrupted (104)")
        .build();
    feed_state.add_named(&interrupted_feed, Some("connection-reset"));
    view.add_named(&feed_state, Some("feed"));

    let generation = Rc::new(Cell::new(0u64));
    let category_generation = Rc::new(Cell::new(0u64));
    let category_loading = Rc::new(Cell::new(false));
    let result_title = Rc::new(RefCell::new(String::from("Search results")));
    let current_category = Rc::new(RefCell::new(None::<String>));
    let feed_from_category = Rc::new(Cell::new(false));
    let feed_from_home = Rc::new(Cell::new(false));
    let view_for_back = view.clone();
    let generation_for_back = generation.clone();
    let status_for_back = status.clone();
    let result_title_for_back = result_title.clone();
    let back_for_click = back.clone();
    let menu_for_back = menu.clone();
    let origin_for_back = origin_following.clone();
    let starred_for_back = origin_starred.clone();
    let category_for_back = current_category.clone();
    let feed_category_for_back = feed_from_category.clone();
    let feed_home_for_back = feed_from_home.clone();
    let category_loading_for_back = category_loading.clone();
    let category_generation_for_back = category_generation.clone();
    let outer_pages_for_back = outer_pages.clone();
    back.connect_clicked(move |_| {
        if outer_pages_for_back.visible_child_name().as_deref() != Some("discover") {
            return;
        }
        generation_for_back.set(generation_for_back.get().wrapping_add(1));
        view_for_back.set_transition_type(gtk::StackTransitionType::SlideRight);
        if view_for_back.visible_child_name().as_deref() == Some("feed") {
            menu_for_back.set_visible(false);
            if origin_for_back.replace(false) {
                back_for_click.set_visible(false);
                back_to_following();
            } else if starred_for_back.replace(false) {
                back_for_click.set_visible(false);
                back_to_starred();
            } else if feed_category_for_back.replace(false) {
                view_for_back.set_visible_child_name("category");
                if let Some(category) = category_for_back.borrow().as_deref() {
                    status_for_back(category, category_loading_for_back.get());
                }
                back_for_click.set_tooltip_text(Some("Back to Discover"));
            } else if feed_home_for_back.replace(false) {
                view_for_back.set_visible_child_name("home");
                status_for_back("Discover", false);
                back_for_click.set_visible(false);
            } else {
                view_for_back.set_visible_child_name("results");
                status_for_back(&result_title_for_back.borrow(), false);
                back_for_click.set_tooltip_text(Some("Back to Discover"));
            }
        } else if view_for_back.visible_child_name().as_deref() == Some("results")
            && category_for_back.borrow().is_some()
        {
            view_for_back.set_visible_child_name("category");
            if let Some(category) = category_for_back.borrow().as_deref() {
                status_for_back(category, category_loading_for_back.get());
            }
        } else {
            category_generation_for_back.set(category_generation_for_back.get().wrapping_add(1));
            category_loading_for_back.set(false);
            view_for_back.set_visible_child_name("home");
            status_for_back("Discover", false);
            back_for_click.set_visible(false);
        }
    });
    let open_feed_slot: Rc<RefCell<Option<std::rc::Weak<dyn Fn(Show, bool, bool)>>>> =
        Rc::new(RefCell::new(None));
    let open_feed: Rc<dyn Fn(Show, bool, bool)> = {
        let view = view.clone();
        let outer_pages = outer_pages.clone();
        let generation = generation.clone();
        let status = status.clone();
        let artwork = artwork.clone();
        let back = back.clone();
        let feed_content = feed_content.clone();
        let feed_metadata = feed_metadata.clone();
        let full_description = full_description.clone();
        let actions = actions.clone();
        let follow_button = follow_button.clone();
        let details_button = details_button.clone();
        let current_feed = current_feed.clone();
        let current_episodes = current_episodes.clone();
        let episode_rows = episode_rows.clone();
        let episode_list_controls = episode_list_controls.clone();
        let episode_view_prefs = episode_view_prefs.clone();
        let showing_action = showing_action.clone();
        let sorting_action = sorting_action.clone();
        let all_played = all_played.clone();
        let mark_menu = mark_menu.clone();
        let mark_action = mark_action.clone();
        let play_action = play_action.clone();
        let latest_button = latest_button.clone();
        let latest_icon = latest_icon.clone();
        let feed_state = feed_state.clone();
        let current_latest = current_latest.clone();
        let playback = playback.clone();
        let downloads = downloads.clone();
        let toast = toast.clone();
        let menu = menu.clone();
        let website = website.clone();
        let selected_show = selected_show.clone();
        let website_action = website_action.clone();
        let unsubscribe_action = unsubscribe_action.clone();
        let store = store.clone();
        let feed_from_category = feed_from_category.clone();
        let feed_from_home = feed_from_home.clone();
        let starred_filter = starred_filter.clone();
        let open_feed_slot = open_feed_slot.clone();
        Rc::new(move |show, animate: bool, starred_only: bool| {
            feed_from_home.set(
                view.visible_child_name().as_deref() == Some("home")
                    && outer_pages.visible_child_name().as_deref() == Some("discover"),
            );
            feed_from_category.set(false);
            starred_filter.set(starred_only);
            status(&show.title, false);
            *website.borrow_mut() = None;
            *selected_show.borrow_mut() = Some(show.clone());
            let prefs = episode_view_prefs.borrow().get(&show.feed_url).copied().unwrap_or_default();
            showing_action.set_state(&if prefs.played_only { "played" } else { "all" }.to_variant());
            sorting_action.set_state(&if prefs.oldest_first { "oldest" } else { "newest" }.to_variant());
            website_action.set_enabled(false);
            unsubscribe_action.set_enabled(store.contains(&show.feed_url));
            menu.set_visible(true);
            feed_content.set_visible(false);
            clear(&feed_artwork);
            feed_description.set_text("");
            feed_description.set_visible(false);
            feed_metadata.set_text("");
            feed_metadata.set_visible(false);
            full_description.borrow_mut().clear();
            *current_feed.borrow_mut() = None;
            actions.set_visible(false);
            update_follow_button(&follow_button, store.contains(&show.feed_url));
            details_button.set_sensitive(false);
            mark_action.set_enabled(false);
            current_episodes.borrow_mut().clear();
            episode_rows.borrow_mut().clear();
            episode_list_controls.borrow_mut().take();
            all_played.set(false);
            set_mark_menu_label(&mark_menu, "Mark All as Played");
            *current_latest.borrow_mut() = None;
            play_action.set_enabled(false);
            latest_button.set_sensitive(false);
            update_latest_button(&latest_icon, &latest_button, false, false);
            feed_state.set_visible_child_name("content");
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
            let feed_metadata = feed_metadata.clone();
            let feed_artwork = feed_artwork.clone();
            let feed_content = feed_content.clone();
            let full_description = full_description.clone();
            let actions = actions.clone();
            let details_button = details_button.clone();
            let current_feed = current_feed.clone();
            let mark_action = mark_action.clone();
            let current_episodes = current_episodes.clone();
            let episode_rows = episode_rows.clone();
            let episode_list_controls = episode_list_controls.clone();
            let episode_view_prefs = episode_view_prefs.clone();
            let all_played = all_played.clone();
            let mark_menu = mark_menu.clone();
            let status = status.clone();
            let artwork = artwork.clone();
            let website = website.clone();
            let website_action = website_action.clone();
            let selected_show = selected_show.clone();
            let play_action = play_action.clone();
            let latest_button = latest_button.clone();
            let latest_icon = latest_icon.clone();
            let feed_state = feed_state.clone();
            let current_latest = current_latest.clone();
            let playback = playback.clone();
            let downloads = downloads.clone();
            let toast = toast.clone();
                                    let open_feed_slot = open_feed_slot.clone();
            let store = store.clone();
            let view_for_loading = view.clone();
            let outer_pages_for_loading = outer_pages.clone();
            let loading_started = Cell::new(false);
            gtk::glib::timeout_add_local(Duration::from_millis(16), move || {
                if generation.get() != id {
                    return gtk::glib::ControlFlow::Break;
                }
                if view_for_loading.is_transition_running() || outer_pages_for_loading.is_transition_running() {
                    return gtk::glib::ControlFlow::Continue;
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
                                let feed_artwork = feed_artwork.clone();
                                let feed_description = feed_description.clone();
                                let feed_metadata = feed_metadata.clone();
                                let feed_content = feed_content.clone();
                                let episodes = episodes.clone();
                                let full_description = full_description.clone();
                                let actions = actions.clone();
                                let details_button = details_button.clone();
                                let current_feed = current_feed.clone();
                                let mark_action = mark_action.clone();
                                let current_episodes = current_episodes.clone();
                                let episode_rows = episode_rows.clone();
                                let episode_list_controls = episode_list_controls.clone();
                                let episode_view_prefs = episode_view_prefs.clone();
                                let all_played = all_played.clone();
                                let mark_menu = mark_menu.clone();
                                let artwork_for_feed = artwork.clone();
                                let status = status.clone();
                                let show_for_feed = show_for_feed.clone();
                                let selected_show = selected_show.clone();
                                let play_action = play_action.clone();
                                let latest_button = latest_button.clone();
                                let latest_icon = latest_icon.clone();
                                let current_latest = current_latest.clone();
                                let playback = playback.clone();
                                let toast = toast.clone();
                                let mut show_for_feed = show_for_feed;
                                show_for_feed.artwork_urls = covers.clone();
                                *selected_show.borrow_mut() = Some(show_for_feed.clone());
                                if let Some(cover_url) = feed.artwork_url.as_ref() {
                                    if let Err(message) = store.prefer_feed_artworks(&[(show_for_feed.feed_url.clone(), cover_url.clone())]) {
                                        toast(&message);
                                    }
                                }
                                feed_artwork.append(&artwork_for_feed.large_cover_widget(&covers, 190));
                                let ids = if starred_only {
                                    starred_ids_for_show(&store, &show_for_feed.feed_url)
                                } else {
                                    feed.all_episode_ids.clone()
                                };
                                *current_episodes.borrow_mut() = ids;
                                let can_mark = !current_episodes.borrow().is_empty();
                                mark_action.set_enabled(can_mark);
                                let mut metadata = feed.categories.iter().take(2).cloned().collect::<Vec<_>>();
                                let updated = feed.last_updated.as_deref()
                                    .map(format_date)
                                    .filter(|value| !value.is_empty())
                                    .or_else(|| show_for_feed.updated.as_deref().and_then(format_search_date));
                                if let Some(updated) = updated {
                                    metadata.push(format!("Updated {updated}"));
                                }
                                feed_metadata.set_text(&metadata.join(" · "));
                                feed_metadata.set_visible(!metadata.is_empty());
                                *current_feed.borrow_mut() = Some(feed.clone());
                                details_button.set_sensitive(true);
                                *current_latest.borrow_mut() = feed.episodes.iter()
                                    .find(|episode| episode.audio_url.is_some())
                                    .cloned();
                                let can_play = current_latest.borrow().is_some();
                                play_action.set_enabled(can_play);
                                latest_button.set_sensitive(can_play);
                                let active = current_latest.borrow().as_ref().is_some_and(|episode| {
                                    playback.is_current(&show_for_feed.feed_url, &episode.id) && playback.is_playing()
                                });
                                update_latest_button(&latest_icon, &latest_button, active, false);
                                show_feed(
                                    &feed_description, &full_description, &episodes, feed,
                                    &show_for_feed,
                                    store.clone(), starred_only,
                                    if starred_only {
                                        open_feed_slot.borrow().as_ref().and_then(std::rc::Weak::upgrade).map(|open_feed| {
                                            Rc::new(move |show| open_feed(show, false, false)) as Rc<dyn Fn(Show)>
                                        })
                                    } else {
                                        None
                                    },
                                    playback.clone(), downloads.clone(), toast, episode_rows.clone(),
                                    episode_list_controls.clone(), episode_view_prefs.clone(),
                                );
                                update_played_ui(
                                    &store, &selected_show, &current_episodes, &episode_rows,
                                    &episode_list_controls,
                                    &mark_menu, &all_played, &current_latest,
                                    &latest_icon, &latest_button, &playback,
                                );
                                actions.set_visible(true);
                                feed_content.set_visible(true);
                                status(&title, false);
                            }
                            Err(message) => {
                                status(&fallback_title, false);
                                if message.contains("HTTP 404") {
                                    feed_state.set_visible_child_name("not-found");
                                } else if message.contains("os error 104")
                                    || message.to_ascii_lowercase().contains("connection reset by peer") {
                                    feed_state.set_visible_child_name("connection-reset");
                                } else {
                                    show_message(&episodes, "Could not load RSS feed", &message);
                                    actions.set_visible(true);
                                    feed_content.set_visible(true);
                                }
                            }
                        }
                        gtk::glib::ControlFlow::Break
                    }
                    Err(mpsc::TryRecvError::Empty) => {
                        if !loading_started.replace(true) {
                            status(&fallback_title, true);
                        }
                        gtk::glib::ControlFlow::Continue
                    }
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
    *open_feed_slot.borrow_mut() = Some(Rc::downgrade(&open_feed));
    let open_followed: Rc<dyn Fn(Show)> = {
        let origin_following = origin_following.clone();
        let open_feed = open_feed.clone();
        Rc::new(move |show| {
            origin_following.set(true);
            open_feed(show, false, false);
        })
    };
    let open_starred: Rc<dyn Fn(Show)> = {
        let origin_starred = origin_starred.clone();
        let open_feed = open_feed.clone();
        Rc::new(move |show| {
            origin_starred.set(true);
            open_feed(show, false, true);
        })
    };
    let request_search: Rc<dyn Fn(String)> = {
        let view = view.clone();
        let outer_pages = outer_pages.clone();
        let results_scroller = results_scroller.clone();
        let listing = listing.clone();
        let current_category = current_category.clone();
        let category_generation = category_generation.clone();
        let category_loading = category_loading.clone();
        let generation = generation.clone();
        let open_feed = open_feed.clone();
        let artwork = artwork.clone();
        let status = status.clone();
        let result_title = result_title.clone();
        let back = back.clone();
        let menu = menu.clone();
        let store = store.clone();
        let toast = toast.clone();
        let follow_toast = follow_toast.clone();
        Rc::new(move |term| {
            category_generation.set(category_generation.get().wrapping_add(1));
            category_loading.set(false);
            let title = "Search results".to_string();
            *current_category.borrow_mut() = None;
            *result_title.borrow_mut() = title.clone();
            status(&title, false);
            menu.set_visible(false);
            clear(&listing);
            listing.set_visible(false);
            if view.visible_child_name().as_deref() != Some("results") {
                results_scroller.vadjustment().set_value(0.0);
            }
            view.set_transition_type(if view.visible_child_name().as_deref() == Some("results") {
                gtk::StackTransitionType::None
            } else if view.is_mapped() {
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
                let _ = tx.send(discovery_service::search(&term));
            });
            let listing = listing.clone();
            let generation = generation.clone();
            let open_feed = open_feed.clone();
            let store = store.clone();
            let toast = toast.clone();
            let follow_toast = follow_toast.clone();
            let artwork = artwork.clone();
            let status = status.clone();
            let view_for_loading = view.clone();
            let outer_pages_for_loading = outer_pages.clone();
            let loading_started = Cell::new(false);
            gtk::glib::timeout_add_local(Duration::from_millis(16), move || {
                if generation.get() != id {
                    return gtk::glib::ControlFlow::Break;
                }
                if view_for_loading.is_transition_running() || outer_pages_for_loading.is_transition_running() {
                    return gtk::glib::ControlFlow::Continue;
                }
                match rx.try_recv() {
                    Ok(answer) => {
                        match answer {
                            Ok(shows) => {
                                clear(&listing);
                                let open_from_search: Rc<dyn Fn(Show, bool)> = {
                                    let open_feed = open_feed.clone();
                                    Rc::new(move |show, animate| open_feed(show, animate, false))
                                };
                                show_results(&listing, shows, open_from_search, &artwork, true, &store, toast.clone(), follow_toast.clone());
                                listing.set_visible(true);
                                status(&title, false);
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
                    Err(mpsc::TryRecvError::Empty) => {
                        if !loading_started.replace(true) {
                            status(&title, true);
                        }
                        gtk::glib::ControlFlow::Continue
                    }
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

    let open_category: Rc<dyn Fn(String)> = {
        let view = view.clone();
        let outer_pages = outer_pages.clone();
        let category_content = category_content.clone();
        let current_category = current_category.clone();
        let generation = generation.clone();
        let category_generation = category_generation.clone();
        let category_loading = category_loading.clone();
        let open_feed = open_feed.clone();
        let feed_from_category = feed_from_category.clone();
        let artwork = artwork.clone();
        let status = status.clone();
        let back = back.clone();
        let menu = menu.clone();
        let store = store.clone();
        let toast = toast.clone();
        let follow_toast = follow_toast.clone();
        Rc::new(move |parent| {
            generation.set(generation.get().wrapping_add(1));
            *current_category.borrow_mut() = Some(parent.clone());
            status(&parent, false);
            menu.set_visible(false);
            clear(&category_content);
            let (tabs, stack, listings, first) = category_tabs(&parent);
            category_content.append(&tabs);
            view.set_transition_type(if view.is_mapped() {
                gtk::StackTransitionType::SlideLeft
            } else {
                gtk::StackTransitionType::None
            });
            view.set_visible_child_name("category");
            back.set_visible(true);
            back.set_tooltip_text(Some("Back to Discover"));

            let id = category_generation.get().wrapping_add(1);
            category_generation.set(id);
            let started = Rc::new(RefCell::new(HashSet::<String>::new()));
            let ready = Rc::new(RefCell::new(HashSet::<String>::new()));
            let listings = Rc::new(listings.into_iter()
                .map(|(name, listing)| (name, listing.downgrade()))
                .collect::<HashMap<_, _>>());
            let load_tab: Rc<dyn Fn(String)> = {
                let stack = stack.downgrade();
                let view = view.clone();
                let outer_pages = outer_pages.clone();
                let category_generation = category_generation.clone();
                let category_loading = category_loading.clone();
                let open_feed = open_feed.clone();
                let feed_from_category = feed_from_category.clone();
                let artwork = artwork.clone();
                let status = status.clone();
                let store = store.clone();
                let toast = toast.clone();
                let follow_toast = follow_toast.clone();
                let parent = parent.clone();
                Rc::new(move |selected| {
                    let Some(stack) = stack.upgrade() else { return };
                    if ready.borrow().contains(&selected) {
                        category_loading.set(false);
                        status(&parent, false);
                        return;
                    }
                    category_loading.set(true);
                    status(&parent, true);
                    if !started.borrow_mut().insert(selected.clone()) {
                        return;
                    }
                    let (tx, rx) = mpsc::channel();
                    let parent_for_worker = parent.clone();
                    let selected_for_worker = selected.clone();
                    std::thread::spawn(move || {
                        let answer = discovery_service::category(&parent_for_worker, &selected_for_worker);
                        let _ = tx.send(answer);
                    });
                    let stack = stack.downgrade();
                    let view = view.clone();
                    let outer_pages = outer_pages.clone();
                    let category_generation = category_generation.clone();
                    let category_loading = category_loading.clone();
                    let open_feed = open_feed.clone();
                    let feed_from_category = feed_from_category.clone();
                    let artwork = artwork.clone();
                    let status = status.clone();
                    let store = store.clone();
                    let toast = toast.clone();
                    let follow_toast = follow_toast.clone();
                    let listings = listings.clone();
                    let ready = ready.clone();
                    let parent = parent.clone();
                    gtk::glib::timeout_add_local(Duration::from_millis(16), move || {
                        if category_generation.get() != id {
                            return gtk::glib::ControlFlow::Break;
                        }
                        if view.is_transition_running() || outer_pages.is_transition_running() {
                            return gtk::glib::ControlFlow::Continue;
                        }
                        let answer = match rx.try_recv() {
                            Ok(answer) => answer,
                            Err(mpsc::TryRecvError::Empty) => return gtk::glib::ControlFlow::Continue,
                            Err(mpsc::TryRecvError::Disconnected) =>
                                Err("The request ended unexpectedly.".to_string()),
                        };
                        if let Some(listing) = listings.get(&selected).and_then(|listing| listing.upgrade()) {
                            clear(&listing);
                            match answer {
                                Ok(shows) => {
                                    let open_feed = open_feed.clone();
                                    let feed_from_category = feed_from_category.clone();
                                    let open_from_category: Rc<dyn Fn(Show, bool)> = Rc::new(move |show, animate| {
                                        open_feed(show, animate, false);
                                        feed_from_category.set(true);
                                    });
                                    show_results(&listing, shows, open_from_category, &artwork, false, &store, toast.clone(), follow_toast.clone());
                                }
                                Err(message) => show_message(&listing, "Could not load podcasts", &message),
                            }
                        }
                        ready.borrow_mut().insert(selected.clone());
                        if stack.upgrade().is_some_and(|stack| stack.visible_child_name().as_deref() == Some(selected.as_str()))
                            && view.visible_child_name().as_deref() == Some("category")
                        {
                            category_loading.set(false);
                            status(&parent, false);
                        }
                        gtk::glib::ControlFlow::Break
                    });
                })
            };
            let load_on_switch = load_tab.clone();
            stack.connect_visible_child_name_notify(move |stack| {
                if let Some(selected) = stack.visible_child_name() {
                    load_on_switch(selected.to_string());
                }
            });
            if let Some(first) = first {
                let view = view.clone();
                let outer_pages = outer_pages.clone();
                let category_generation = category_generation.clone();
                let stack = stack.downgrade();
                gtk::glib::timeout_add_local(Duration::from_millis(16), move || {
                    if category_generation.get() != id {
                        return gtk::glib::ControlFlow::Break;
                    }
                    if view.is_transition_running() || outer_pages.is_transition_running() {
                        return gtk::glib::ControlFlow::Continue;
                    }
                    if stack.upgrade().is_some_and(|stack| stack.visible_child_name().as_deref() == Some(first.as_str())) {
                        load_tab(first.clone());
                    }
                    gtk::glib::ControlFlow::Break
                });
            }
        })
    };

    let home = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .build();
    let content = gtk::Box::new(gtk::Orientation::Vertical, 30);
    content.set_margin_top(16);
    content.set_margin_bottom(24);
    content.append(&categories(open_category));

    let top_section = show_section("Top Shows");
    content.append(&top_section.0);
    let recommended_section = show_section("Recommended for You");
    content.append(&recommended_section.0);

    let top_started = Cell::new(false);
    let recommended_feeds = RefCell::new(None::<Vec<String>>);
    let recommended_generation = Rc::new(Cell::new(0u64));
    let top_body = top_section.1;
    let recommended_body = recommended_section.1;
    let open_from_home = open_feed.clone();
    let artwork_for_home = artwork.clone();
    let store_for_home = store.clone();
    home.connect_map(move |_| {
        if !top_started.replace(true) {
            show_section_loading(&top_body);
            let (tx, rx) = mpsc::channel();
            std::thread::spawn(move || {
                let _ = tx.send(discovery_service::top_shows_us());
            });
            load_show_section(
                &top_body, rx, open_from_home.clone(), artwork_for_home.clone(),
                "No top shows with public RSS feeds were available.",
            );
        }
        let followed = match store_for_home.list() {
            Ok(shows) => shows,
            Err(error) => {
                show_section_message(&recommended_body, &error);
                return;
            }
        };
        let mut feed_urls: Vec<_> = followed.iter().map(|show| show.feed_url.clone()).collect();
        feed_urls.sort();
        if recommended_feeds.borrow().as_ref() == Some(&feed_urls) {
            return;
        }
        *recommended_feeds.borrow_mut() = Some(feed_urls);
        recommended_generation.set(recommended_generation.get().wrapping_add(1));
        if followed.is_empty() {
            show_section_message(&recommended_body, "Follow podcasts to get recommendations.");
            return;
        }
        show_section_loading(&recommended_body);
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(discovery_service::recommended_shows(&followed));
        });
        load_show_section_with_generation(
            &recommended_body, rx, open_from_home.clone(), artwork_for_home.clone(),
            "No related shows with public RSS feeds were available.",
            Some((recommended_generation.get(), recommended_generation.clone())),
        );
    });

    let clamp = adw::Clamp::builder().maximum_size(1080).build();
    clamp.set_margin_start(8);
    clamp.set_margin_end(8);
    clamp.set_child(Some(&content));
    home.set_child(Some(&clamp));
    view.add_named(&home, Some("home"));
    view.set_visible_child_name("home");

    let search_request = request_search.clone();
    entry.connect_activate(move |entry| {
        let term = entry.text().trim().to_string();
        if !term.is_empty() {
            search_request(term);
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
    let reset_starred_origin = origin_starred.clone();
    let reset_feed_home = feed_from_home.clone();
    let reset_category_generation = category_generation.clone();
    let reset_category_loading = category_loading.clone();
    let reset_current_category = current_category.clone();
    let reset: Rc<dyn Fn()> = Rc::new(move || {
        reset_generation.set(reset_generation.get().wrapping_add(1));
        reset_category_generation.set(reset_category_generation.get().wrapping_add(1));
        reset_category_loading.set(false);
        *reset_current_category.borrow_mut() = None;
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
        reset_starred_origin.set(false);
        reset_feed_home.set(false);
    });
    DiscoveryPage { widget: view.upcast(), reset, open_followed, open_starred }
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

fn category_tabs(parent: &str) -> (gtk::Box, adw::ViewStack, HashMap<String, gtk::Box>, Option<String>) {
    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content.set_vexpand(true);
    let stack = adw::ViewStack::new();
    stack.set_vexpand(true);
    let switcher = adw::InlineViewSwitcher::new();
    switcher.set_stack(Some(&stack));
    switcher.set_can_shrink(true);
    switcher.set_display_mode(adw::InlineViewSwitcherDisplayMode::Labels);
    let tab_scroller = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Automatic)
        .vscrollbar_policy(gtk::PolicyType::Never)
        .min_content_height(48)
        .child(&switcher)
        .build();
    tab_scroller.set_margin_start(16);
    tab_scroller.set_margin_end(16);
    content.append(&tab_scroller);

    let mut options = Vec::new();
    if parent != "Climate" {
        options.push(("All", parent));
    }
    options.extend(data::subcategories(parent).iter().map(|(name, _)| (*name, *name)));
    let first = options.first().map(|(_, selected)| (*selected).to_string());
    let mut listings = HashMap::new();
    for (label, selected) in options {
        let listing = gtk::Box::new(gtk::Orientation::Vertical, 0);
        listing.set_margin_top(16);
        listing.set_margin_bottom(24);
        let clamp = adw::Clamp::builder().maximum_size(620).build();
        clamp.set_margin_start(16);
        clamp.set_margin_end(16);
        clamp.set_child(Some(&listing));
        let scroller = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vexpand(true)
            .child(&clamp)
            .build();
        stack.add_titled(&scroller, Some(selected), label);
        listings.insert(selected.to_string(), listing);
    }
    content.append(&stack);
    (content, stack, listings, first)
}

fn show_results(
    container: &gtk::Box,
    shows: Vec<Show>,
    open_feed: Rc<dyn Fn(Show, bool)>,
    artwork: &ArtworkLoader,
    cue_first: bool,
    store: &Rc<FollowingStore>,
    toast: Rc<dyn Fn(&str)>,
    follow_toast: Rc<dyn Fn(&str)>,
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
        let follow_toast_for_add = follow_toast.clone();
        add.connect_clicked(move |_| {
            if store_for_add.contains(&show_for_add.feed_url) {
                return;
            }
            match store_for_add.add(&show_for_add) {
                Ok(true) => follow_toast_for_add(&format!("Added {} to Following", show_for_add.title)),
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
    let content = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let icon = gtk::Image::from_icon_name(if followed {
        "app-installed-symbolic"
    } else {
        "list-add-symbolic"
    });
    icon.set_pixel_size(16);
    icon.set_valign(gtk::Align::Center);
    content.append(&icon);
    content.append(&gtk::Label::new(Some(if followed { "Following" } else { "Follow" })));
    button.set_child(Some(&content));
    if followed {
        button.add_css_class("podcast-following");
    } else {
        button.remove_css_class("podcast-following");
    }
    button.set_tooltip_text(Some(if followed {
        "Unfollow this podcast"
    } else {
        "Follow this podcast"
    }));
}

fn update_latest_button(icon: &gtk::Image, button: &gtk::Button, active: bool, replay: bool) {
    icon.set_icon_name(Some(if replay {
        "view-refresh-symbolic"
    } else if active {
        "media-playback-pause-symbolic"
    } else {
        "media-playback-start-symbolic"
    }));
    button.set_tooltip_text(Some(if replay {
        "Replay the latest episode from the beginning"
    } else if active {
        "Pause the latest episode"
    } else {
        "Play the latest episode"
    }));
}

fn set_mark_menu_label(menu: &gtk::gio::Menu, label: &str) {
    menu.remove(1);
    menu.insert(1, Some(label), Some("podcast.mark-played"));
}

fn update_played_ui(
    store: &FollowingStore,
    selected_show: &Rc<RefCell<Option<Show>>>,
    episode_ids: &Rc<RefCell<Vec<String>>>,
    rows: &Rc<RefCell<HashMap<String, EpisodeRowControls>>>,
    list_controls: &Rc<RefCell<Option<EpisodeListControls>>>,
    mark_menu: &gtk::gio::Menu,
    all_played: &Cell<bool>,
    latest: &Rc<RefCell<Option<Episode>>>,
    latest_icon: &gtk::Image,
    latest_button: &gtk::Button,
    playback: &PlaybackController,
) {
    let Some(show) = selected_show.borrow().clone() else {
        return;
    };
    let ids = episode_ids.borrow();
    if ids.is_empty() {
        return;
    }
    let Ok(played) = store.played_ids(&show.feed_url) else {
        return;
    };
    for (id, row) in rows.borrow().iter() {
        row.set_played(played.contains(id), playback);
    }
    if let Some(controls) = list_controls.borrow().as_ref() {
        refresh_episode_visibility(&rows.borrow(), controls, store);
    }
    let count = ids.iter().filter(|id| played.contains(*id)).count();
    let complete = count == ids.len();
    all_played.set(complete);
    set_mark_menu_label(mark_menu, if complete {
        "Mark All as Unplayed"
    } else if count > 0 {
        "Mark Rest as Played"
    } else {
        "Mark All as Played"
    });
    let active = latest.borrow().as_ref().is_some_and(|episode| {
        playback.is_current(&show.feed_url, &episode.id) && playback.is_playing()
    });
    update_latest_button(latest_icon, latest_button, active, complete);
}

fn show_podcast_details(parent: &gtk::Button, show: &Show, feed: &FeedDetails) {
    let group = adw::PreferencesGroup::new();
    let add_detail = |title: &str, value: &str| {
        if !value.trim().is_empty() {
            let row = adw::ActionRow::builder().title(title).subtitle(value).build();
            row.set_subtitle_lines(0);
            group.add(&row);
        }
    };
    add_detail("Provider", &show.author);
    if let Some(copyright) = feed.copyright.as_deref() {
        add_detail("Copyright", copyright);
    }
    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&adw::HeaderBar::new());
    if show.author.trim().is_empty() && feed.copyright.as_deref().map_or(true, |value| value.trim().is_empty()) {
        let empty = adw::StatusPage::builder()
            .icon_name("help-about-symbolic")
            .title("No podcast details available")
            .description("This podcast feed has no additional details.")
            .build();
        toolbar.set_content(Some(&empty));
    } else {
        let body = gtk::Box::new(gtk::Orientation::Vertical, 0);
        body.set_margin_top(16);
        body.set_margin_bottom(24);
        body.set_margin_start(24);
        body.set_margin_end(24);
        body.append(&group);
        let scroller = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .child(&body)
            .build();
        toolbar.set_content(Some(&scroller));
    }
    let dialog = adw::Dialog::builder()
        .title("Podcast details")
        .content_width(440)
        .content_height(320)
        .child(&toolbar)
        .build();
    dialog.present(Some(parent));
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
    gtk::gio::SimpleAction,
    gtk::gio::SimpleAction,
    gtk::gio::Menu,
);

fn setup_podcast_menu(
    button: &gtk::MenuButton,
    store: Rc<FollowingStore>,
    toast: Rc<dyn Fn(&str)>,
    follow_toast: Rc<dyn Fn(&str)>,
) -> PodcastMenuState {
    button.set_visible(false);
    let model = gtk::gio::Menu::new();
    model.append(Some("Open Website"), Some("podcast.website"));
    model.append(Some("Mark All as Played"), Some("podcast.mark-played"));
    model.append(Some("Play Latest Episode"), Some("podcast.play-latest"));
    let showing_menu = gtk::gio::Menu::new();
    showing_menu.append(Some("All Episodes"), Some("podcast.showing::all"));
    showing_menu.append(Some("Played Episodes"), Some("podcast.showing::played"));
    model.append_submenu(Some("Showing"), &showing_menu);
    let sorting_menu = gtk::gio::Menu::new();
    sorting_menu.append(Some("Newest to Oldest"), Some("podcast.sorting::newest"));
    sorting_menu.append(Some("Oldest to Newest"), Some("podcast.sorting::oldest"));
    model.append_submenu(Some("Sorting"), &sorting_menu);
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
    let showing_state = "all".to_variant();
    let showing_action = gtk::gio::SimpleAction::new_stateful(
        "showing", Some(showing_state.type_()), &showing_state,
    );
    group.add_action(&showing_action);
    let sorting_state = "newest".to_variant();
    let sorting_action = gtk::gio::SimpleAction::new_stateful(
        "sorting", Some(sorting_state.type_()), &sorting_state,
    );
    group.add_action(&sorting_action);
    let unsubscribe = gtk::gio::SimpleAction::new("unsubscribe", None);
    unsubscribe.set_enabled(false);
    let selected_for_unsubscribe = selected_show.clone();
    unsubscribe.connect_activate(move |action, _| {
        let show = selected_for_unsubscribe.borrow().clone();
        if let Some(show) = show {
            match store.remove(&show.feed_url) {
                Ok(true) => {
                    action.set_enabled(false);
                    follow_toast(&format!("Unsubscribed from {}", show.title));
                }
                Ok(false) => {}
                Err(message) => toast(&message),
            }
        }
    });
    group.add_action(&unsubscribe);
    button.insert_action_group("podcast", Some(&group));
    (website, selected_show, website_action, unsubscribe, mark_action, play_action,
        showing_action, sorting_action, model)
}

fn format_search_date(raw: &str) -> Option<String> {
    let updated = if let Ok(seconds) = raw.parse::<i64>() {
        gtk::glib::DateTime::from_unix_utc(seconds).ok()?.to_local().ok()?
    } else {
        gtk::glib::DateTime::from_iso8601(raw, None).ok()?.to_local().ok()?
    };
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
    store: Rc<FollowingStore>,
    starred_only: bool,
    go_to_show: Option<Rc<dyn Fn(Show)>>,
    playback: Rc<PlaybackController>,
    downloads: Rc<DownloadManager>,
    toast: Rc<dyn Fn(&str)>,
    episode_rows: Rc<RefCell<HashMap<String, EpisodeRowControls>>>,
    list_controls: Rc<RefCell<Option<EpisodeListControls>>>,
    view_prefs: Rc<RefCell<HashMap<String, EpisodeViewPrefs>>>,
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
    let all_episodes: Vec<_> = if starred_only {
        match store.starred() {
            Ok(items) => items.into_iter()
                .filter(|item| item.show.feed_url == show.feed_url && item.episode.audio_url.is_some())
                .map(|item| item.episode)
                .collect(),
            Err(error) => {
                show_message(episodes, "Could not load Starred", &error);
                return;
            }
        }
    } else {
        feed.episodes.into_iter()
            .filter(|episode| episode.audio_url.is_some())
            .collect()
    };
    if all_episodes.is_empty() {
        if starred_only {
            show_message(episodes, "No starred episodes", "Star episodes from their right-click menu.");
        } else {
            show_message(episodes, "No playable episodes found", "This RSS feed has no public audio episodes to show.");
        }
        return;
    }
    let list = gtk::ListBox::new();
    list.add_css_class("boxed-list");
    list.set_selection_mode(gtk::SelectionMode::None);
    list.set_activate_on_single_click(true);
    let prefs = view_prefs.borrow().get(&show.feed_url).copied().unwrap_or_default();
    let played_only = Rc::new(Cell::new(prefs.played_only));
    let oldest_first = Rc::new(Cell::new(prefs.oldest_first));
    let empty = gtk::Label::new(Some("No played episodes"));
    empty.add_css_class("dim-label");
    empty.set_margin_top(28);
    empty.set_margin_bottom(28);
    empty.set_visible(false);
    *list_controls.borrow_mut() = Some(EpisodeListControls {
        list: list.clone(), empty: empty.clone(), played_only, oldest_first: oldest_first.clone(),
        starred_only, feed_url: show.feed_url.clone(),
    });
    let mut row_details = HashMap::new();
    let mut sort_details = HashMap::new();
    for (position, episode) in all_episodes.iter().enumerate() {
        let (row, over_play, row_controls) = super::episode_row::create(
            show, episode, &store, &playback, &downloads, &toast, go_to_show.clone(),
        );
        let key = row.as_ptr() as usize;
        row_details.insert(key, (episode.clone(), over_play.clone()));
        let timestamp = parse_date(&episode.date).map(|date| date.to_unix()).unwrap_or(i64::MIN);
        sort_details.insert(key, (timestamp, position));
        episode_rows.borrow_mut().insert(episode.id.clone(), row_controls);
        list.append(&row);
    }
    let sort_details = Rc::new(sort_details);
    list.set_sort_func(move |first, second| {
        let first = sort_details.get(&(first.as_ptr() as usize)).copied().unwrap_or((i64::MIN, usize::MAX));
        let second = sort_details.get(&(second.as_ptr() as usize)).copied().unwrap_or((i64::MIN, usize::MAX));
        if oldest_first.get() {
            first.0.cmp(&second.0).then(first.1.cmp(&second.1)).into()
        } else {
            second.0.cmp(&first.0).then(first.1.cmp(&second.1)).into()
        }
    });
    let show_for_dialog = show.clone();
    let row_details = Rc::new(row_details);
    list.connect_row_activated(move |_, row| {
        if let Some((episode, over_play)) = row_details.get(&(row.as_ptr() as usize)) {
            if !over_play.get() {
                show_episode_dialog(
                    row, &show_for_dialog, episode,
                    playback.clone(), toast.clone(),
                );
            }
        }
    });
    episodes.append(&list);
    episodes.append(&empty);
}

fn categories(open_category: Rc<dyn Fn(String)>) -> gtk::Widget {
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
        let open_category = open_category.clone();
        let category = (*name).to_string();
        tile.connect_clicked(move |_| open_category(category.clone()));
        flow.insert(&tile, -1);
    }

    flow.upcast()
}

fn show_section(title: &str) -> (gtk::Box, gtk::Box) {
    let section = gtk::Box::new(gtk::Orientation::Vertical, 10);
    let heading = gtk::Label::new(Some(title));
    heading.set_xalign(0.0);
    heading.add_css_class("heading");
    section.append(&heading);
    let body = gtk::Box::new(gtk::Orientation::Vertical, 0);
    section.append(&body);
    (section, body)
}

fn show_section_loading(body: &gtk::Box) {
    clear(body);
    let line = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    line.set_margin_top(10);
    let spinner = gtk::Spinner::new();
    spinner.start();
    line.append(&spinner);
    let label = gtk::Label::new(Some("Loading shows…"));
    label.add_css_class("dim-label");
    line.append(&label);
    body.append(&line);
}

fn show_section_message(body: &gtk::Box, message: &str) {
    clear(body);
    let label = gtk::Label::new(Some(message));
    label.set_xalign(0.0);
    label.set_wrap(true);
    label.add_css_class("dim-label");
    label.set_margin_top(10);
    body.append(&label);
}

fn load_show_section(
    body: &gtk::Box,
    rx: mpsc::Receiver<Result<Vec<Show>, String>>,
    open_show: Rc<dyn Fn(Show, bool, bool)>,
    artwork: Rc<ArtworkLoader>,
    empty_message: &'static str,
) {
    load_show_section_with_generation(body, rx, open_show, artwork, empty_message, None);
}

fn load_show_section_with_generation(
    body: &gtk::Box,
    rx: mpsc::Receiver<Result<Vec<Show>, String>>,
    open_show: Rc<dyn Fn(Show, bool, bool)>,
    artwork: Rc<ArtworkLoader>,
    empty_message: &'static str,
    generation: Option<(u64, Rc<Cell<u64>>)>,
) {
    let weak_body = body.downgrade();
    gtk::glib::timeout_add_local(Duration::from_millis(50), move || {
        if generation.as_ref().is_some_and(|(id, current)| current.get() != *id) {
            return gtk::glib::ControlFlow::Break;
        }
        let Some(body) = weak_body.upgrade() else {
            return gtk::glib::ControlFlow::Break;
        };
        match rx.try_recv() {
            Ok(Ok(shows)) if !shows.is_empty() => {
                clear(&body);
                let grid = gtk::FlowBox::builder()
                    .selection_mode(gtk::SelectionMode::None)
                    .min_children_per_line(1)
                    .max_children_per_line(3)
                    .column_spacing(12)
                    .row_spacing(12)
                    .homogeneous(true)
                    .hexpand(true)
                    .build();
                for show in shows.into_iter().take(9) {
                    let card = gtk::Button::new();
                    card.add_css_class("card");
                    card.add_css_class("discovery-show-card");
                    card.set_size_request(190, 80);
                    card.set_hexpand(true);
                    card.set_tooltip_text(Some(&show.title));
                    let content = gtk::Box::new(gtk::Orientation::Horizontal, 12);
                    content.set_margin_top(12);
                    content.set_margin_bottom(12);
                    content.set_margin_start(12);
                    content.set_margin_end(12);
                    let cover = artwork.cached_widget(&show.artwork_urls, 56);
                    cover.add_css_class("discovery-card-artwork");
                    content.append(&cover);
                    let details = gtk::Box::new(gtk::Orientation::Vertical, 3);
                    details.set_hexpand(true);
                    details.set_valign(gtk::Align::Center);
                    let name = gtk::Label::new(Some(&show.title));
                    name.set_xalign(0.0);
                    name.set_ellipsize(gtk::pango::EllipsizeMode::End);
                    name.set_max_width_chars(16);
                    name.add_css_class("heading");
                    details.append(&name);
                    let author = gtk::Label::new(Some(&show.author));
                    author.set_xalign(0.0);
                    author.set_ellipsize(gtk::pango::EllipsizeMode::End);
                    author.set_max_width_chars(16);
                    author.add_css_class("dim-label");
                    details.append(&author);
                    content.append(&details);
                    card.set_child(Some(&content));
                    let open_show = open_show.clone();
                    card.connect_clicked(move |_| open_show(show.clone(), true, false));
                    grid.insert(&card, -1);
                }
                body.append(&grid);
                gtk::glib::ControlFlow::Break
            }
            Ok(Ok(_)) => {
                show_section_message(&body, empty_message);
                gtk::glib::ControlFlow::Break
            }
            Ok(Err(error)) => {
                show_section_message(&body, &error);
                gtk::glib::ControlFlow::Break
            }
            Err(mpsc::TryRecvError::Empty) => gtk::glib::ControlFlow::Continue,
            Err(mpsc::TryRecvError::Disconnected) => {
                show_section_message(&body, "Could not load shows.");
                gtk::glib::ControlFlow::Break
            }
        }
    });
}

