//! GNOME media controls through the MPRIS interfaces on the session bus.

use std::cell::RefCell;
use std::rc::Rc;

use gtk::gio::{self, prelude::*, DBusConnection, RegistrationId};
use gtk::glib::{variant::{ObjectPath, ToVariant}, Variant, VariantDict};
use gtk::prelude::*;
use sha1::{Digest, Sha1};

use crate::playback::PlaybackController;

const PATH: &str = "/org/mpris/MediaPlayer2";
const ROOT: &str = "org.mpris.MediaPlayer2";
const PLAYER: &str = "org.mpris.MediaPlayer2.Player";
const BUS_NAME: &str = "org.mpris.MediaPlayer2.elitepodcasts";

const XML: &str = r#"<node>
  <interface name="org.mpris.MediaPlayer2">
    <method name="Raise"/>
    <method name="Quit"/>
    <property name="CanQuit" type="b" access="read"/>
    <property name="Fullscreen" type="b" access="readwrite"/>
    <property name="CanSetFullscreen" type="b" access="read"/>
    <property name="CanRaise" type="b" access="read"/>
    <property name="HasTrackList" type="b" access="read"/>
    <property name="Identity" type="s" access="read"/>
    <property name="SupportedUriSchemes" type="as" access="read"/>
    <property name="SupportedMimeTypes" type="as" access="read"/>
  </interface>
  <interface name="org.mpris.MediaPlayer2.Player">
    <method name="Next"/>
    <method name="Previous"/>
    <method name="Pause"/>
    <method name="PlayPause"/>
    <method name="Stop"/>
    <method name="Play"/>
    <method name="Seek"><arg name="Offset" type="x" direction="in"/></method>
    <method name="SetPosition">
      <arg name="TrackId" type="o" direction="in"/>
      <arg name="Position" type="x" direction="in"/>
    </method>
    <method name="OpenUri"><arg name="Uri" type="s" direction="in"/></method>
    <signal name="Seeked"><arg name="Position" type="x"/></signal>
    <property name="PlaybackStatus" type="s" access="read"/>
    <property name="Rate" type="d" access="readwrite"/>
    <property name="Metadata" type="a{sv}" access="read"/>
    <property name="Position" type="x" access="read"/>
    <property name="MinimumRate" type="d" access="read"/>
    <property name="MaximumRate" type="d" access="read"/>
    <property name="CanGoNext" type="b" access="read"/>
    <property name="CanGoPrevious" type="b" access="read"/>
    <property name="CanPlay" type="b" access="read"/>
    <property name="CanPause" type="b" access="read"/>
    <property name="CanSeek" type="b" access="read"/>
    <property name="CanControl" type="b" access="read"/>
  </interface>
</node>"#;

#[derive(Clone, PartialEq)]
struct Snapshot {
    track: Option<String>,
    status: &'static str,
    rate: f64,
    duration: u64,
    can_next: bool,
    can_play: bool,
    can_seek: bool,
    seek_revision: u64,
}

impl Snapshot {
    fn from_playback(playback: &PlaybackController) -> Self {
        Self {
            track: playback.current().map(|item| track_path(&item.feed_url, &item.episode.id)),
            status: status(playback),
            rate: playback.rate(),
            duration: playback.duration(),
            can_next: playback.can_go_next(),
            can_play: playback.can_play(),
            can_seek: playback.can_seek(),
            seek_revision: playback.seek_revision(),
        }
    }
}

struct Export {
    connection: DBusConnection,
    root_id: RegistrationId,
    player_id: RegistrationId,
}

pub struct Mpris {
    owner: RefCell<Option<gio::OwnerId>>,
    export: Rc<RefCell<Option<Export>>>,
}

impl Mpris {
    pub fn start(app: &adw::Application, playback: &Rc<PlaybackController>) -> Self {
        let export: Rc<RefCell<Option<Export>>> = Rc::new(RefCell::new(None));
        let last = Rc::new(RefCell::new(Snapshot::from_playback(playback)));
        let weak_playback = Rc::downgrade(playback);
        let export_for_watch = export.clone();
        let last_for_watch = last.clone();
        playback.watch(move || {
            let Some(playback) = weak_playback.upgrade() else { return false };
            let next = Snapshot::from_playback(&playback);
            let previous = last_for_watch.replace(next.clone());
            if let Some(active) = export_for_watch.borrow().as_ref() {
                if next.seek_revision != previous.seek_revision {
                    let params = (micros(playback.position()),).to_variant();
                    let _ = active.connection.emit_signal(None, PATH, PLAYER, "Seeked", Some(&params));
                }
                if next != previous {
                    emit_player_properties(&active.connection, &playback);
                }
            }
            true
        });

        let export_for_bus = export.clone();
        let playback_for_bus = Rc::downgrade(playback);
        let app_for_bus = app.downgrade();
        let owner = gio::bus_own_name(
            gio::BusType::Session,
            BUS_NAME,
            gio::BusNameOwnerFlags::NONE,
            move |connection, _| {
                let (Some(playback), Some(app)) = (playback_for_bus.upgrade(), app_for_bus.upgrade()) else {
                    return;
                };
                match register(&connection, &app, &playback) {
                    Ok(active) => *export_for_bus.borrow_mut() = Some(active),
                    Err(error) => eprintln!("Could not publish media controls: {error}"),
                }
            },
            |_, _| {},
            {
                let export = export.clone();
                move |_, _| clear_export(&export)
            },
        );
        Self { owner: RefCell::new(Some(owner)), export }
    }

    pub fn stop(&self) {
        clear_export(&self.export);
        if let Some(owner) = self.owner.borrow_mut().take() {
            gio::bus_unown_name(owner);
        }
    }
}

fn clear_export(export: &RefCell<Option<Export>>) {
    if let Some(active) = export.borrow_mut().take() {
        let _ = active.connection.unregister_object(active.player_id);
        let _ = active.connection.unregister_object(active.root_id);
    }
}

fn register(
    connection: &DBusConnection,
    app: &adw::Application,
    playback: &Rc<PlaybackController>,
) -> Result<Export, String> {
    let node = gio::DBusNodeInfo::for_xml(XML).map_err(|error| error.to_string())?;
    let root_info = node.lookup_interface(ROOT).ok_or("MPRIS root interface is missing")?;
    let player_info = node.lookup_interface(PLAYER).ok_or("MPRIS player interface is missing")?;

    let app_for_raise = app.downgrade();
    let root_id = connection.register_object(PATH, &root_info)
        .method_call(move |_, _, _, _, method, _, invocation| {
            match method {
                "Raise" => {
                    if let Some(app) = app_for_raise.upgrade() {
                        if let Some(window) = app.active_window() { window.present(); }
                    }
                    invocation.return_value(None);
                }
                "Quit" => {
                    if let Some(app) = app_for_raise.upgrade() { app.quit(); }
                    invocation.return_value(None);
                }
                _ => invocation.return_dbus_error("org.freedesktop.DBus.Error.UnknownMethod", method),
            }
        })
        .property(|_, _, _, _, name| root_property(name))
        .set_property(|_, _, _, _, _, _| false)
        .build().map_err(|error| error.to_string())?;

    let playback_for_call = Rc::downgrade(playback);
    let playback_for_get = Rc::downgrade(playback);
    let playback_for_set = Rc::downgrade(playback);
    let player_id = match connection.register_object(PATH, &player_info)
        .method_call(move |_, _, _, _, method, args, invocation| {
            let Some(playback) = playback_for_call.upgrade() else {
                invocation.return_dbus_error("org.mpris.MediaPlayer2.Player.Error.Failed", "Player is unavailable");
                return;
            };
            let result = player_method(&playback, method, &args);
            match result {
                Ok(()) => invocation.return_value(None),
                Err(error) => invocation.return_dbus_error("org.mpris.MediaPlayer2.Player.Error.Failed", &error),
            }
        })
        .property(move |_, _, _, _, name| {
            playback_for_get.upgrade().map(|playback| player_property(&playback, name))
                .unwrap_or_else(|| false.to_variant())
        })
        .set_property(move |_, _, _, _, name, value| {
            let Some(playback) = playback_for_set.upgrade() else { return false };
            match name {
                "Rate" => value.get::<f64>()
                    .is_some_and(|rate| (0.5..=2.0).contains(&rate) && playback.set_rate(rate).is_ok()),
                _ => false,
            }
        })
        .build() {
        Ok(id) => id,
        Err(error) => {
            let _ = connection.unregister_object(root_id);
            return Err(error.to_string());
        }
    };
    Ok(Export { connection: connection.clone(), root_id, player_id })
}

fn root_property(name: &str) -> Variant {
    match name {
        "CanQuit" | "CanRaise" => true.to_variant(),
        "Fullscreen" | "CanSetFullscreen" | "HasTrackList" => false.to_variant(),
        "Identity" => "Elite Podcasts".to_variant(),
        "SupportedUriSchemes" | "SupportedMimeTypes" => Vec::<String>::new().to_variant(),
        _ => false.to_variant(),
    }
}

fn player_property(playback: &PlaybackController, name: &str) -> Variant {
    match name {
        "PlaybackStatus" => status(playback).to_variant(),
        "Rate" => playback.rate().to_variant(),
        "Metadata" => metadata(playback),
        "Position" => micros(playback.position()).to_variant(),
        "MinimumRate" => 0.5_f64.to_variant(),
        "MaximumRate" => 2.0_f64.to_variant(),
        "CanGoNext" => playback.can_go_next().to_variant(),
        "CanGoPrevious" => false.to_variant(),
        "CanPlay" => playback.can_play().to_variant(),
        "CanPause" => playback.can_seek().to_variant(),
        "CanSeek" => playback.can_seek().to_variant(),
        "CanControl" => true.to_variant(),
        _ => false.to_variant(),
    }
}

fn player_method(playback: &PlaybackController, method: &str, args: &Variant) -> Result<(), String> {
    match method {
        "Play" => playback.play(),
        "Pause" => playback.pause(),
        "PlayPause" => {
            if playback.is_playing() { playback.pause() } else { playback.play() }
        }
        "Stop" => playback.stop(),
        "Next" => playback.play_next(),
        "Previous" | "OpenUri" => Ok(()),
        "Seek" => {
            if playback.can_seek() {
                let offset = args.child_get::<i64>(0) / 1_000_000;
                playback.seek_relative(offset)?;
            }
            Ok(())
        }
        "SetPosition" => {
            let path = args.child_get::<ObjectPath>(0);
            let position = args.child_get::<i64>(1);
            if playback.can_seek() && position >= 0 && playback.current().is_some_and(|item| {
                path.as_str() == track_path(&item.feed_url, &item.episode.id)
            }) {
                playback.seek_to((position as u64) / 1_000_000)?;
            }
            Ok(())
        }
        _ => Err(format!("Unknown media command: {method}")),
    }
}

fn status(playback: &PlaybackController) -> &'static str {
    if playback.is_playing() { "Playing" }
    else if playback.can_seek() { "Paused" }
    else { "Stopped" }
}

fn metadata(playback: &PlaybackController) -> Variant {
    let dict = VariantDict::new(None);
    if let Some(item) = playback.current() {
        let path = track_path(&item.feed_url, &item.episode.id);
        if let Ok(path) = ObjectPath::try_from(path) {
            dict.insert_value("mpris:trackid", &path.to_variant());
        }
        dict.insert_value("xesam:title", &item.episode.title.to_variant());
        dict.insert_value("xesam:artist", &vec![item.show_title.clone()].to_variant());
        dict.insert_value("xesam:album", &item.show_title.to_variant());
        dict.insert_value("mpris:length", &micros(playback.duration()).to_variant());
        if let Some(url) = item.artwork_urls.iter().find(|url| url.starts_with("https://") || url.starts_with("http://")) {
            dict.insert_value("mpris:artUrl", &url.to_variant());
        }
        if let Some(url) = item.episode.audio_url {
            dict.insert_value("xesam:url", &url.to_variant());
        }
    }
    dict.end()
}

fn track_path(feed_url: &str, episode_id: &str) -> String {
    let digest = Sha1::digest(format!("{feed_url}\0{episode_id}").as_bytes());
    format!("/org/mpris/MediaPlayer2/Track/t{digest:x}")
}

fn micros(seconds: u64) -> i64 {
    seconds.saturating_mul(1_000_000).min(i64::MAX as u64) as i64
}

fn emit_player_properties(connection: &DBusConnection, playback: &PlaybackController) {
    let changed = VariantDict::new(None);
    for name in [
        "PlaybackStatus", "Rate", "Metadata", "MinimumRate", "MaximumRate",
        "CanGoNext", "CanGoPrevious", "CanPlay", "CanPause", "CanSeek", "CanControl",
    ] {
        changed.insert_value(name, &player_property(playback, name));
    }
    let params = Variant::tuple_from_iter([
        PLAYER.to_variant(), changed.end(), Vec::<String>::new().to_variant(),
    ]);
    let _ = connection.emit_signal(None, PATH, "org.freedesktop.DBus.Properties", "PropertiesChanged", Some(&params));
}
