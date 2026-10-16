use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;
use std::time::Duration;

use gst::prelude::*;

use crate::discovery_service::{Episode, Show};
use crate::following::FollowingStore;

#[derive(Clone)]
pub struct PlayingEpisode {
    pub show: Show,
    pub show_title: String,
    pub feed_url: String,
    pub episode: Episode,
    pub artwork_urls: Vec<String>,
}

#[derive(Clone)]
pub struct QueuedEpisode {
    pub show: Show,
    pub episode: Episode,
}

pub struct PlaybackController {
    player: gst::Element,
    bus: gst::Bus,
    store: Rc<FollowingStore>,
    current: RefCell<Option<PlayingEpisode>>,
    queue: RefCell<VecDeque<QueuedEpisode>>,
    library_revision: Cell<u64>,
    playing: Cell<bool>,
    finished: Cell<bool>,
    position: Cell<u64>,
    duration: Cell<u64>,
    seek_revision: Cell<u64>,
    pending_resume: Cell<u64>,
    rate: Cell<f64>,
    ticks: Cell<u8>,
    listeners: RefCell<Vec<Box<dyn Fn() -> bool>>>,
    toast: Rc<dyn Fn(&str)>,
}

impl PlaybackController {
    pub fn new(store: Rc<FollowingStore>, toast: Rc<dyn Fn(&str)>) -> Result<Rc<Self>, String> {
        gst::init().map_err(|error| format!("Could not initialize audio playback: {error}"))?;
        let player = gst::ElementFactory::make("playbin")
            .build()
            .map_err(|error| format!("GStreamer playbin is unavailable: {error}"))?;
        if let Ok(filter) = gst::ElementFactory::make("scaletempo").build() {
            player.set_property("audio-filter", &filter);
        }
        let bus = player.bus().ok_or("GStreamer did not provide a message bus")?;
        let controller = Rc::new(Self {
            player,
            bus,
            store,
            current: RefCell::new(None),
            queue: RefCell::new(VecDeque::new()),
            library_revision: Cell::new(0),
            playing: Cell::new(false),
            finished: Cell::new(false),
            position: Cell::new(0),
            duration: Cell::new(0),
            seek_revision: Cell::new(0),
            pending_resume: Cell::new(0),
            rate: Cell::new(1.0),
            ticks: Cell::new(0),
            listeners: RefCell::new(Vec::new()),
            toast,
        });
        let weak = Rc::downgrade(&controller);
        gtk::glib::timeout_add_local(Duration::from_millis(250), move || {
            if let Some(controller) = weak.upgrade() {
                controller.update();
                gtk::glib::ControlFlow::Continue
            } else {
                gtk::glib::ControlFlow::Break
            }
        });
        Ok(controller)
    }

    pub fn current(&self) -> Option<PlayingEpisode> {
        self.current.borrow().clone()
    }

    pub fn queue(&self) -> Vec<QueuedEpisode> {
        self.queue.borrow().iter().cloned().collect()
    }

    pub fn library_revision(&self) -> u64 {
        self.library_revision.get()
    }

    pub fn enqueue(&self, show: &Show, episode: &Episode) -> Result<(), String> {
        if episode.audio_url.is_none() {
            return Err("This episode has no audio source".to_string());
        }
        self.queue.borrow_mut().push_back(QueuedEpisode {
            show: show.clone(), episode: episode.clone(),
        });
        self.library_changed();
        Ok(())
    }

    pub fn remove_queued(&self, index: usize) {
        let removed = self.queue.borrow_mut().remove(index).is_some();
        if removed {
            self.library_changed();
        }
    }

    pub fn play_queued(&self, index: usize) -> Result<(), String> {
        let item = self.queue.borrow().get(index).cloned()
            .ok_or("This episode is no longer in the queue")?;
        self.play_episode(&item.show, &item.episode)?;
        self.remove_queued(index);
        Ok(())
    }

    pub fn clear_queue(&self) {
        let had_items = !self.queue.borrow().is_empty();
        if had_items {
            self.queue.borrow_mut().clear();
            self.library_changed();
        }
    }

    pub fn history(&self) -> Result<Vec<crate::following::HistoryEntry>, String> {
        self.store.history()
    }

    pub fn clear_history(&self) -> Result<(), String> {
        self.store.clear_history()?;
        self.library_changed();
        Ok(())
    }

    pub fn is_playing(&self) -> bool {
        self.playing.get()
    }

    pub fn can_play(&self) -> bool {
        self.current.borrow().is_some() || !self.queue.borrow().is_empty()
    }

    pub fn can_go_next(&self) -> bool {
        !self.queue.borrow().is_empty()
    }

    pub fn can_seek(&self) -> bool {
        self.current.borrow().is_some() && !self.finished.get()
    }

    pub fn play(&self) -> Result<(), String> {
        if self.current.borrow().is_none() {
            return self.play_next();
        }
        if !self.playing.get() {
            self.toggle()?;
        }
        Ok(())
    }

    pub fn pause(&self) -> Result<(), String> {
        if self.playing.get() {
            self.toggle()?;
        }
        Ok(())
    }

    pub fn stop(&self) -> Result<(), String> {
        if self.current.borrow().is_none() {
            return Ok(());
        }
        self.save_position();
        self.player.set_state(gst::State::Null)
            .map_err(|error| format!("Could not stop playback: {error}"))?;
        *self.current.borrow_mut() = None;
        self.playing.set(false);
        self.finished.set(false);
        self.position.set(0);
        self.duration.set(0);
        self.pending_resume.set(0);
        self.notify();
        Ok(())
    }

    pub fn play_next(&self) -> Result<(), String> {
        let next = self.queue.borrow().front().cloned();
        if let Some(next) = next {
            self.play_episode(&next.show, &next.episode)?;
            self.queue.borrow_mut().pop_front();
            self.library_changed();
        }
        Ok(())
    }

    pub fn is_current(&self, feed_url: &str, episode_id: &str) -> bool {
        self.current.borrow().as_ref().is_some_and(|item| {
            item.feed_url == feed_url && item.episode.id == episode_id
        })
    }

    pub fn is_current_active(&self, feed_url: &str, episode_id: &str) -> bool {
        self.is_current(feed_url, episode_id) && !self.finished.get()
    }

    pub fn play_or_toggle(&self, show: &Show, episode: &Episode) -> Result<(), String> {
        if self.is_current(&show.feed_url, &episode.id) {
            self.toggle()
        } else {
            self.play_episode(show, episode)
        }
    }

    pub fn position(&self) -> u64 {
        self.position.get()
    }

    pub fn duration(&self) -> u64 {
        self.duration.get()
    }

    pub fn seek_revision(&self) -> u64 {
        self.seek_revision.get()
    }

    pub fn rate(&self) -> f64 {
        self.rate.get()
    }

    pub fn watch(&self, listener: impl Fn() -> bool + 'static) {
        self.listeners.borrow_mut().push(Box::new(listener));
    }

    pub fn play_episode(&self, show: &Show, episode: &Episode) -> Result<(), String> {
        let uri = episode.audio_url.as_deref().ok_or("This episode has no audio source")?;
        self.save_position();
        self.player
            .set_state(gst::State::Null)
            .map_err(|error| format!("Could not stop the previous episode: {error}"))?;
        while self.bus.pop().is_some() {}
        self.player.set_property("uri", uri);
        let mut artwork_urls = Vec::new();
        if let Some(url) = &episode.artwork_url {
            artwork_urls.push(url.clone());
        }
        artwork_urls.extend(show.artwork_urls.iter().cloned());
        let duration = episode.duration_seconds.unwrap_or(0);
        let saved = self.store.playback_position(&show.feed_url, &episode.id);
        let resume = if duration > 0 && saved.saturating_add(15) >= duration {
            0
        } else {
            saved
        };
        *self.current.borrow_mut() = Some(PlayingEpisode {
            show: show.clone(),
            show_title: show.title.clone(),
            feed_url: show.feed_url.clone(),
            episode: episode.clone(),
            artwork_urls,
        });
        self.position.set(0);
        self.duration.set(duration);
        self.pending_resume.set(resume);
        self.rate.set(1.0);
        if let Err(error) = self.player.set_state(gst::State::Playing) {
            *self.current.borrow_mut() = None;
            self.finished.set(true);
            self.pending_resume.set(0);
            let _ = self.player.set_state(gst::State::Null);
            self.notify();
            return Err(format!("Could not start this episode: {error}"));
        }
        self.playing.set(true);
        self.finished.set(false);
        self.notify();
        Ok(())
    }

    pub fn toggle(&self) -> Result<(), String> {
        if self.current.borrow().is_none() {
            return Ok(());
        }
        if self.finished.get() {
            let item = self.current.borrow().clone().expect("current episode checked above");
            return self.play_episode(&item.show, &item.episode);
        }
        if self.playing.get() {
            self.player
                .set_state(gst::State::Paused)
                .map_err(|error| format!("Could not pause playback: {error}"))?;
            self.playing.set(false);
            self.save_position();
        } else {
            self.player
                .set_state(gst::State::Playing)
                .map_err(|error| format!("Could not resume playback: {error}"))?;
            self.playing.set(true);
        }
        self.notify();
        Ok(())
    }

    pub fn seek_relative(&self, offset: i64) -> Result<(), String> {
        let position = self.position.get().saturating_add_signed(offset);
        self.seek_to(position)
    }

    pub fn seek_to(&self, seconds: u64) -> Result<(), String> {
        if self.current.borrow().is_none() {
            return Ok(());
        }
        let limit = self.duration.get();
        let position = if limit > 0 { seconds.min(limit) } else { seconds };
        self.pending_resume.set(0);
        self.player
            .seek_simple(gst::SeekFlags::FLUSH | gst::SeekFlags::KEY_UNIT, gst::ClockTime::from_seconds(position))
            .map_err(|error| format!("Could not seek in this episode: {error}"))?;
        self.position.set(position);
        self.seek_revision.set(self.seek_revision.get().wrapping_add(1));
        self.save_position();
        self.notify();
        Ok(())
    }

    pub fn set_rate(&self, rate: f64) -> Result<(), String> {
        if self.current.borrow().is_none() || !(0.5..=2.0).contains(&rate) {
            return Ok(());
        }
        let position = self.position.get();
        self.player
            .seek(
                rate,
                gst::SeekFlags::FLUSH | gst::SeekFlags::ACCURATE,
                gst::SeekType::Set,
                gst::ClockTime::from_seconds(position),
                gst::SeekType::None,
                None::<gst::ClockTime>,
            )
            .map_err(|error| format!("Could not change playback speed: {error}"))?;
        self.rate.set(rate);
        self.notify();
        Ok(())
    }

    fn update(&self) {
        let mut ended = false;
        while let Some(message) = self.bus.pop() {
            match message.view() {
                gst::MessageView::Error(error) => {
                    self.playing.set(false);
                    self.finished.set(true);
                    self.save_position();
                    let _ = self.player.set_state(gst::State::Null);
                    (self.toast)(&format!("Playback failed: {}", error.error()));
                }
                gst::MessageView::Eos(_) => {
                    ended = true;
                    self.playing.set(false);
                    self.finished.set(true);
                    self.position.set(0);
                    self.pending_resume.set(0);
                    let _ = self.player.set_state(gst::State::Ready);
                    if let Some(item) = self.current.borrow().as_ref() {
                        let _ = self.store.save_playback_position(&item.feed_url, &item.episode.id, 0);
                        if let Err(error) = self.store.mark_played(&item.feed_url, &[item.episode.id.clone()]) {
                            (self.toast)(&error);
                        }
                        if let Err(error) = self.store.record_completed(&item.show, &item.episode) {
                            (self.toast)(&error);
                        } else {
                            self.library_changed();
                        }
                    }
                }
                _ => {}
            }
        }
        if self.current.borrow().is_none() {
            return;
        }
        if ended {
            let next = self.queue.borrow_mut().pop_front();
            if let Some(next) = next {
                self.library_changed();
                if let Err(error) = self.play_episode(&next.show, &next.episode) {
                    (self.toast)(&error);
                }
            } else {
                self.notify();
            }
            return;
        }
        if let Some(duration) = self.player.query_duration::<gst::ClockTime>() {
            self.duration.set(duration.seconds());
        }
        let queried_position = self.player.query_position::<gst::ClockTime>();
        if self.pending_resume.get() > 0 && (self.duration.get() > 0 || queried_position.is_some()) {
            let resume = if self.duration.get() > 0 {
                self.pending_resume.get().min(self.duration.get().saturating_sub(1))
            } else {
                self.pending_resume.get()
            };
            if self.player.seek_simple(
                gst::SeekFlags::FLUSH | gst::SeekFlags::KEY_UNIT,
                gst::ClockTime::from_seconds(resume),
            ).is_ok() {
                self.position.set(resume);
                self.pending_resume.set(0);
            }
        } else if let Some(position) = queried_position {
            self.position.set(position.seconds());
        }
        let ticks = self.ticks.get().wrapping_add(1);
        self.ticks.set(ticks);
        if ticks >= 20 {
            self.ticks.set(0);
            if self.playing.get() {
                self.save_position();
            }
        }
        self.notify();
    }

    fn save_position(&self) {
        if let Some(item) = self.current.borrow().as_ref() {
            if let Err(error) = self.store.save_playback_position(
                &item.feed_url,
                &item.episode.id,
                self.pending_resume.get().max(self.position.get()),
            ) {
                (self.toast)(&error);
            }
        }
    }

    fn notify(&self) {
        self.listeners.borrow_mut().retain(|listener| listener());
    }

    fn library_changed(&self) {
        self.library_revision.set(self.library_revision.get().wrapping_add(1));
        self.notify();
    }
}

impl Drop for PlaybackController {
    fn drop(&mut self) {
        self.save_position();
        let _ = self.player.set_state(gst::State::Null);
    }
}
