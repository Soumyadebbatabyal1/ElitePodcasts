# Elite Podcasts

A podcast player for GNOME. Discover shows, follow podcasts, and listen to episodes in a native GTK and libadwaita app.

## Features

- Find shows through search, categories, charts, and recommendations.
- Follow podcasts and browse new episodes.
- Play episodes, manage a queue, and control playback from GNOME media controls.
- Star episodes, keep listening history, and download episodes for offline listening.
- Import and export podcast subscriptions with OPML.
- Choose light or dark appearance.

Show discovery uses Apple's public podcast directory. Episode lists and audio come from each podcast's public RSS feed. No API key is needed.

## Get the app

Elite Podcasts is in early development, and there is not a ready-to-install public release yet. When one is available, this section will link to the download. The planned Linux distribution is Flathub.

## Build from source

For now, you can run the app from a checkout. You need Rust, GTK4, libadwaita, GStreamer, and their development files installed.

```sh
cargo run --locked
```

On Fedora, install the main system dependencies with:

```sh
sudo dnf install gtk4-devel libadwaita-devel gstreamer1-devel \
  gstreamer1-plugins-base gstreamer1-plugins-good pkgconf-pkg-config gcc
```

The first build downloads Rust dependencies. Close any other running copy of Elite Podcasts before launching the development build.

## Privacy

Your followed shows, playback position, and listening history are stored on your device. The app connects to Apple's public podcast directory for discovery and to podcast feed and audio servers to show and play episodes.

## License

Elite Podcasts is free software under the GNU General Public License, version 3 or later (GPL-3.0-or-later).
