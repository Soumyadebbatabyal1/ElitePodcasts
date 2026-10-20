# Elite Podcasts

Elite Podcasts is a native GNOME app for discovering, following, and listening to podcasts. It uses GTK4, libadwaita, and GStreamer. Discovery uses Apple's public podcast directory and charts; episodes come from each show's public RSS feed. No API key is needed.

## Features

- Browse categories, Top Shows, and recommendations, or search for a show.
- Follow shows and see their new episodes in one place. Refresh New Episodes when you choose; opening the page does not fetch every feed again.
- Play, pause, seek, skip, adjust speed, and control playback from GNOME media controls. The last episode and position are restored when the app reopens.
- Queue episodes, star them, mark them played, and review listening history.
- Download episodes to `Music/Podcasts`, with progress and cancellation in the Downloaded page.
- Import and export subscriptions as OPML.
- Use the system light or dark appearance, or choose an override in Settings.

The app stores subscriptions, playback state, and history locally in SQLite. Search results, feeds, and artwork are cached locally. The playback queue itself is cleared when the app closes.

## Install with Flatpak

The Flatpak application ID is `io.github.soumyadebbatabyal1.ElitePodcasts`. It matches the [source repository](https://github.com/Soumyadebbatabyal1/ElitePodcasts); the name shown in GNOME remains **Elite Podcasts**.

Until a release is published, build the Flatpak from this checkout. Install `flatpak-builder`, add the Flathub remote, and install the GNOME 50 SDK and Rust extension:

```sh
sudo dnf install flatpak-builder
flatpak remote-add --user --if-not-exists flathub https://flathub.org/repo/flathub.flatpakrepo
flatpak install --user flathub org.gnome.Sdk//50 org.freedesktop.Sdk.Extension.rust-stable//25.08
flatpak-builder --user --install --force-clean --disable-rofiles-fuse .flatpak-build io.github.soumyadebbatabyal1.ElitePodcasts.json
flatpak run io.github.soumyadebbatabyal1.ElitePodcasts
```

The Flatpak can access the network for podcast data and audio, the audio server for playback, and `Music/Podcasts` for downloads. OPML import and export use the desktop file chooser. Flatpak stores the app's other data separately from a native Cargo installation, so subscriptions and playback state do not automatically transfer between them.

## Build from source

Install stable Rust and Cargo, GTK4 development files (4.8 or newer), libadwaita development files (1.7 or newer), GStreamer development files and plugins, a C compiler, `pkg-config`, `glib-compile-resources`, and `glib-compile-schemas`. On Fedora, the main packages are `rust`, `cargo`, `gtk4-devel`, `libadwaita-devel`, `gstreamer1-devel`, `gstreamer1-plugins-base`, `gstreamer1-plugins-good`, and `gcc`.

```sh
cargo run --locked
```

The first Cargo build downloads dependencies. Close any running Elite Podcasts window before launching a different build, since the application ID allows one running instance.

## Flatpak maintenance

The Flatpak manifest is [`io.github.soumyadebbatabyal1.ElitePodcasts.json`](io.github.soumyadebbatabyal1.ElitePodcasts.json). It builds with `cargo --offline --locked`; [`cargo-sources.json`](cargo-sources.json) lists the exact crates in `Cargo.lock`. Whenever the lockfile changes, regenerate that file with [Flatpak's Cargo source generator](https://github.com/flatpak/flatpak-builder-tools/tree/master/cargo):

```sh
curl -LO https://raw.githubusercontent.com/flatpak/flatpak-builder-tools/master/cargo/flatpak-cargo-generator.py
python3 flatpak-cargo-generator.py Cargo.lock -o cargo-sources.json
```

The current manifest uses the checked-out source tree for local builds. Before a Flathub submission, pin its application source to a public release tag and commit, publish an up-to-date screenshot, and add its permanent URL to the AppStream metadata in `data/`. Flatpak updates are delivered by publishing a new release and updating the pinned source and release metadata in the Flathub manifest. The application ID stays the same across updates.

## Project layout

- `src/app.rs` starts the application.
- `src/ui/` contains the native interface, including Discover, the library pages, episode dialogs, player, and settings.
- `src/data.rs` defines categories and subcategories.
- `src/discovery_service.rs` handles podcast discovery and public RSS feeds.
- `src/playback.rs` and `src/mpris.rs` handle playback and GNOME media controls.
- `src/following.rs`, `src/downloads.rs`, and `src/cache.rs` store local state and downloads.
- `data/` contains the desktop entry, settings schema, and AppStream metadata.
- `assets/icons/` contains the application icon and symbolic interface icons. The skip and speedometer icons are from [GNOME Audio Player (Decibels)](https://gitlab.gnome.org/GNOME/decibels), licensed under GPL-3.0.

## License

GPL-3.0-or-later.
