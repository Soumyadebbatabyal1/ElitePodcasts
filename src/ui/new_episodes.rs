use adw::prelude::*;
use serde::{Deserialize, Serialize};
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet, VecDeque};
use std::rc::Rc;
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

use super::episode::{parse_date, show_dialog};
use super::download_manager::DownloadManager;
use crate::cache;
use crate::discovery_service::{self, Episode, Show};
use crate::following::FollowingStore;
use crate::playback::PlaybackController;

#[derive(Clone, Serialize, Deserialize)]
struct SavedEpisode {
    show: Show,
    episode: Episode,
}

struct RowRecord {
    row: gtk::ListBoxRow,
    group: usize,
    published: i64,
    saved: SavedEpisode,
}

type RowDetails = (Show, Episode, Rc<Cell<bool>>);

struct EpisodeGroup {
    heading: gtk::Label,
    list: gtk::ListBox,
    details: Rc<RefCell<HashMap<usize, RowDetails>>>,
}

struct EpisodeList {
    content: gtk::Box,
    groups: Vec<EpisodeGroup>,
    rows: HashMap<(String, String), RowRecord>,
    status: Option<gtk::Widget>,
    playback: Rc<PlaybackController>,
    downloads: Rc<DownloadManager>,
    store: Rc<FollowingStore>,
    toast: Rc<dyn Fn(&str)>,
    open_show: Rc<dyn Fn(Show)>,
}

impl EpisodeList {
    fn new(
        content: &gtk::Box,
        store: Rc<FollowingStore>,
        playback: Rc<PlaybackController>,
        downloads: Rc<DownloadManager>,
        toast: Rc<dyn Fn(&str)>,
        open_show: Rc<dyn Fn(Show)>,
    ) -> Self {
        let mut groups = Vec::new();
        for (index, title) in ["Today", "Yesterday", "This week", "This month", "Older"]
            .into_iter().enumerate()
        {
            let heading = gtk::Label::new(Some(title));
            heading.set_xalign(0.0);
            heading.add_css_class("title-3");
            heading.set_margin_top(if index == 0 { 0 } else { 24 });
            heading.set_margin_bottom(8);
            heading.set_visible(false);
            let list = gtk::ListBox::new();
            list.add_css_class("boxed-list");
            list.set_selection_mode(gtk::SelectionMode::None);
            list.set_activate_on_single_click(true);
            list.set_visible(false);
            let details = Rc::new(RefCell::new(HashMap::<usize, RowDetails>::new()));
            let details_for_dialog = details.clone();
            let playback_for_dialog = playback.clone();
            let toast_for_dialog = toast.clone();
            list.connect_row_activated(move |_, row| {
                let key = row.as_ptr() as usize;
                if let Some((show, episode, over)) = details_for_dialog.borrow().get(&key) {
                    if !over.get() {
                        show_dialog(
                            row, show, episode,
                            playback_for_dialog.clone(), toast_for_dialog.clone(),
                        );
                    }
                }
            });
            content.append(&heading);
            content.append(&list);
            groups.push(EpisodeGroup { heading, list, details });
        }
        Self {
            content: content.clone(), groups, rows: HashMap::new(), status: None,
            playback, downloads, store, toast, open_show,
        }
    }

    fn clear_status(&mut self) {
        if let Some(status) = self.status.take() {
            self.content.remove(&status);
        }
    }

    fn show_status(&mut self, title: &str, description: &str) {
        self.clear_status();
        if !self.rows.is_empty() {
            return;
        }
        let status: gtk::Widget = adw::StatusPage::builder()
            .icon_name("media-playlist-repeat-symbolic")
            .title(title)
            .description(description)
            .vexpand(true)
            .build()
            .upcast();
        self.content.append(&status);
        self.status = Some(status);
    }

    fn add(&mut self, saved: SavedEpisode) {
        let key = (saved.show.feed_url.clone(), saved.episode.id.clone());
        if self.rows.contains_key(&key) {
            return;
        }
        let published = parse_date(&saved.episode.date);
        let now = gtk::glib::DateTime::now_local().ok();
        let group_index = date_group(published.as_ref(), now.as_ref());
        let timestamp = published.as_ref().map(|date| date.to_unix()).unwrap_or(i64::MIN);
        let group = &self.groups[group_index];
        let position = self.rows.values()
            .filter(|row| row.group == group_index && row.published >= timestamp)
            .count() as i32;
        let (row, over, _) = super::episode_row::create(
            &saved.show, &saved.episode, &self.store, &self.playback, &self.downloads,
            &self.toast, Some(self.open_show.clone()),
        );
        group.details.borrow_mut().insert(
            row.as_ptr() as usize,
            (saved.show.clone(), saved.episode.clone(), over),
        );
        group.list.insert(&row, position);
        group.heading.set_visible(true);
        group.list.set_visible(true);
        self.rows.insert(key, RowRecord { row, group: group_index, published: timestamp, saved });
        self.clear_status();
    }

    fn replace_show(&mut self, show: Show, episodes: Vec<Episode>, played: &HashSet<String>) -> Vec<SavedEpisode> {
        let incoming: HashSet<String> = episodes.iter()
            .filter(|episode| episode.audio_url.is_some() && !played.contains(&episode.id))
            .map(|episode| episode.id.clone())
            .collect();
        let stale: Vec<_> = self.rows.keys()
            .filter(|(url, id)| url == &show.feed_url && !incoming.contains(id))
            .cloned()
            .collect();
        for key in stale {
            self.remove(&key);
        }
        episodes.into_iter()
            .filter(|episode| incoming.contains(&episode.id))
            .filter(|episode| !self.rows.contains_key(&(show.feed_url.clone(), episode.id.clone())))
            .map(|episode| SavedEpisode { show: show.clone(), episode })
            .collect()
    }

    fn remove(&mut self, key: &(String, String)) {
        if let Some(record) = self.rows.remove(key) {
            let group = &self.groups[record.group];
            group.details.borrow_mut().remove(&(record.row.as_ptr() as usize));
            group.list.remove(&record.row);
            if group.list.first_child().is_none() {
                group.heading.set_visible(false);
                group.list.set_visible(false);
            }
        }
    }

    fn retain_feeds(&mut self, followed: &HashSet<String>) {
        let removed: Vec<_> = self.rows.keys()
            .filter(|(feed_url, _)| !followed.contains(feed_url))
            .cloned()
            .collect();
        for key in removed {
            self.remove(&key);
        }
    }

    fn snapshot(&self) -> Vec<SavedEpisode> {
        let mut rows: Vec<_> = self.rows.values().collect();
        rows.sort_by(|left, right| right.published.cmp(&left.published));
        rows.into_iter().map(|row| row.saved.clone()).collect()
    }

    fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
}

struct FeedResult {
    show: Show,
    episodes: Option<Vec<Episode>>,
    failed: bool,
}

pub(super) struct NewEpisodesPage {
    pub widget: gtk::Widget,
    pub refresh: Rc<dyn Fn()>,
    pub restore: Rc<dyn Fn()>,
}

pub(super) fn page(
    store: Rc<FollowingStore>,
    playback: Rc<PlaybackController>,
    downloads: Rc<DownloadManager>,
    toast: Rc<dyn Fn(&str)>,
    busy: Rc<dyn Fn(bool)>,
    open_show: Rc<dyn Fn(Show)>,
) -> NewEpisodesPage {
    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content.set_vexpand(true);
    content.set_margin_top(20);
    content.set_margin_bottom(28);
    let clamp = adw::Clamp::builder().maximum_size(700).vexpand(true).build();
    clamp.set_margin_start(16);
    clamp.set_margin_end(16);
    clamp.set_child(Some(&content));
    let scroller = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .child(&clamp)
        .build();
    let list = Rc::new(RefCell::new(EpisodeList::new(
        &content, store.clone(), playback, downloads, toast.clone(), open_show,
    )));
    let restored_once = Rc::new(Cell::new(false));
    let load_generation = Rc::new(Cell::new(0u64));
    let refreshing = Rc::new(Cell::new(false));

    let restore: Rc<dyn Fn()> = Rc::new({
        let list = list.clone();
        let busy = busy.clone();
        let restored_once = restored_once.clone();
        let load_generation = load_generation.clone();
        move || {
            if restored_once.replace(true) {
                return;
            }
            let id = load_generation.get().wrapping_add(1);
            load_generation.set(id);
            busy(true);
            let (tx, rx) = mpsc::channel();
            std::thread::spawn(move || {
                let saved = cache::read_json::<Vec<SavedEpisode>>("new-episodes-v1", "latest", None);
                let result = saved.map(|saved| {
                    FollowingStore::episode_filters().map(|(followed, played)| {
                        saved.into_iter()
                            .filter(|item| followed.contains(&item.show.feed_url))
                            .filter(|item| !played.get(&item.show.feed_url)
                                .is_some_and(|ids| ids.contains(&item.episode.id)))
                            .collect::<Vec<_>>()
                    })
                }).transpose();
                let _ = tx.send(result);
            });
            let list = list.clone();
            let busy = busy.clone();
            let load_generation = load_generation.clone();
            let mut pending = VecDeque::new();
            let mut loaded = false;
            gtk::glib::timeout_add_local(Duration::from_millis(16), move || {
                if load_generation.get() != id {
                    return gtk::glib::ControlFlow::Break;
                }
                if !loaded {
                    match rx.try_recv() {
                        Ok(Ok(Some(saved))) => {
                            pending = saved.into();
                            loaded = true;
                            if pending.is_empty() {
                                list.borrow_mut().show_status(
                                    "New Episodes",
                                    "Choose Check for New Episodes to refresh your followed shows.",
                                );
                            }
                        }
                        Ok(Ok(None)) => {
                            list.borrow_mut().show_status(
                                "New Episodes",
                                "Choose Check for New Episodes from the page menu to load unplayed episodes.",
                            );
                            busy(false);
                            return gtk::glib::ControlFlow::Break;
                        }
                        Ok(Err(message)) => {
                            list.borrow_mut().show_status("Could not load episodes", &message);
                            busy(false);
                            return gtk::glib::ControlFlow::Break;
                        }
                        Err(mpsc::TryRecvError::Empty) => return gtk::glib::ControlFlow::Continue,
                        Err(mpsc::TryRecvError::Disconnected) => {
                            list.borrow_mut().show_status(
                                "Could not load episodes", "The saved episode list could not be read.",
                            );
                            busy(false);
                            return gtk::glib::ControlFlow::Break;
                        }
                    }
                }
                for _ in 0..4 {
                    let Some(item) = pending.pop_front() else { break };
                    list.borrow_mut().add(item);
                }
                if pending.is_empty() {
                    busy(false);
                    gtk::glib::ControlFlow::Break
                } else {
                    gtk::glib::ControlFlow::Continue
                }
            });
        }
    });

    let refresh: Rc<dyn Fn()> = Rc::new({
        let list = list.clone();
        let busy = busy.clone();
        let toast = toast.clone();
        let restored_once = restored_once.clone();
        let load_generation = load_generation.clone();
        let refreshing = refreshing.clone();
        move || {
            if refreshing.replace(true) {
                return;
            }
            restored_once.set(true);
            load_generation.set(load_generation.get().wrapping_add(1));
            let shows = match store.list() {
                Ok(shows) => shows,
                Err(message) => {
                    list.borrow_mut().show_status("Could not load followed podcasts", &message);
                    toast(&message);
                    refreshing.set(false);
                    busy(false);
                    return;
                }
            };
            let followed: HashSet<_> = shows.iter().map(|show| show.feed_url.clone()).collect();
            list.borrow_mut().retain_feeds(&followed);
            if shows.is_empty() {
                list.borrow_mut().show_status(
                    "No followed podcasts yet",
                    "Add podcasts from search to see their unplayed episodes here.",
                );
                refreshing.set(false);
                busy(false);
                return;
            }
            list.borrow_mut().clear_status();
            busy(true);
            let total = shows.len();
            let work = Arc::new(Mutex::new(VecDeque::from(shows)));
            let (tx, rx) = mpsc::channel::<FeedResult>();
            for _ in 0..total.min(4) {
                let work = work.clone();
                let tx = tx.clone();
                std::thread::spawn(move || {
                    loop {
                        let show = match work.lock() {
                            Ok(mut queue) => queue.pop_front(),
                            Err(_) => None,
                        };
                        let Some(mut show) = show else { break };
                        let (feed, failed) = match discovery_service::feed_fresh(&show.feed_url) {
                            Ok(feed) => (Some(feed), false),
                            Err(_) => (discovery_service::cached_feed(&show.feed_url), true),
                        };
                        let episodes = feed.map(|feed| {
                            if let Some(url) = feed.artwork_url {
                                show.artwork_urls.insert(0, url);
                            }
                            feed.episodes
                        });
                        if tx.send(FeedResult { show, episodes, failed }).is_err() {
                            break;
                        }
                    }
                });
            }
            drop(tx);
            let list = list.clone();
            let store = store.clone();
            let busy = busy.clone();
            let toast = toast.clone();
            let refreshing = refreshing.clone();
            let mut completed = 0usize;
            let mut failed = 0usize;
            let mut pending = VecDeque::new();
            gtk::glib::timeout_add_local(Duration::from_millis(16), move || {
                for _ in 0..2 {
                    match rx.try_recv() {
                        Ok(result) => {
                            completed += 1;
                            if result.failed {
                                failed += 1;
                            }
                            if !store.contains(&result.show.feed_url) {
                                continue;
                            }
                            let Some(episodes) = result.episodes else { continue };
                            match store.played_ids(&result.show.feed_url) {
                                Ok(played) => {
                                    pending.extend(list.borrow_mut().replace_show(
                                        result.show, episodes, &played,
                                    ));
                                }
                                Err(message) => {
                                    failed += 1;
                                    toast(&message);
                                }
                            }
                        }
                        Err(mpsc::TryRecvError::Empty) => break,
                        Err(mpsc::TryRecvError::Disconnected) => {
                            failed += total - completed;
                            completed = total;
                            break;
                        }
                    }
                }
                for _ in 0..4 {
                    let Some(item) = pending.pop_front() else { break };
                    list.borrow_mut().add(item);
                }
                if completed < total || !pending.is_empty() {
                    return gtk::glib::ControlFlow::Continue;
                }
                let snapshot = list.borrow().snapshot();
                std::thread::spawn(move || {
                    cache::write_json("new-episodes-v1", "latest", &snapshot);
                });
                if list.borrow().is_empty() {
                    list.borrow_mut().show_status(
                        if failed > 0 { "Could not load episodes" } else { "All caught up" },
                        if failed > 0 {
                            "Some followed feeds could not be reached. Choose Check for New Episodes to retry."
                        } else {
                            "There are no unplayed episodes in your followed podcasts."
                        },
                    );
                }
                if failed > 0 {
                    toast(&format!("Could not update {failed} followed feeds"));
                }
                refreshing.set(false);
                busy(false);
                gtk::glib::ControlFlow::Break
            });
        }
    });

    NewEpisodesPage { widget: scroller.upcast(), refresh, restore }
}

pub(super) fn date_group(published: Option<&gtk::glib::DateTime>, now: Option<&gtk::glib::DateTime>) -> usize {
    let (Some(published), Some(now)) = (published, now) else {
        return 4;
    };
    let same_day = |left: &gtk::glib::DateTime, right: &gtk::glib::DateTime| {
        left.year() == right.year() && left.month() == right.month() && left.day_of_month() == right.day_of_month()
    };
    if same_day(published, now) {
        0
    } else if now.add_days(-1).is_ok_and(|yesterday| same_day(published, &yesterday)) {
        1
    } else if published.week_numbering_year() == now.week_numbering_year()
        && published.week_of_year() == now.week_of_year() {
        2
    } else if published.year() == now.year() && published.month() == now.month() {
        3
    } else {
        4
    }
}
