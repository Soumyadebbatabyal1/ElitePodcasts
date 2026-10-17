# Elite Podcasts

Elite Podcasts is a native GTK4 and libadwaita podcast application for GNOME. Search uses Apple's public podcast directory, category browsing uses its podcast charts, and selecting a result shows recent episodes from its public RSS feed. The window uses a Settings-style sidebar, a current-page title, and a loading spinner beside that title. It opens at 960×680 and has a 700×520 minimum size. Following and playback positions are saved locally in SQLite. Audio playback uses GStreamer.

## Requirements

- Rust stable and Cargo (Rust 1.78 or newer)
- GTK 4.8 or newer development files
- libadwaita development files (1.5 or newer)
- GStreamer runtime plugins and development files
- A C compiler and `pkg-config`

On Fedora, the relevant packages are `rust`, `cargo`, `gtk4-devel`, `libadwaita-devel`, `gstreamer1-devel`, `gstreamer1-plugins-base`, `gstreamer1-plugins-good`, and `gcc`.

## Build and run

```sh
cargo run
```

Cargo downloads the Rust crates on the first build. Close any older Elite Podcasts window before launching a newly built version; the application ID allows only one running instance.

Select the search button, type a podcast name, and press Enter. Search uses Apple's public podcast directory and needs no API key. Category tiles show podcasts from Apple's US category charts; the Climate tile uses the Earth Sciences chart. Chart entries are looked up in the directory to obtain public RSS URLs, so shows without one are omitted. Search and category results are cached for 30 minutes, with saved results used if the service is unavailable. Network requests run in worker threads.

Search and category results use native Adwaita boxed lists with artwork, show and publisher names, and an available date above the title. Dates use the local day: Today, Yesterday, day and month, or a year when needed. Exact title matches appear first. A search briefly outlines its first result in the accent color. Select a row to see a podcast cover, description, and up to 20 recent episodes from its public RSS feed. Use Add to follow a show; the button becomes green and says Added. The Following page displays a responsive cover grid, and its covers open the podcast page. The podcast page shortens long descriptions with an inline More link that opens a native dialog. Beneath it, the headphones button and podcast menu can mark the loaded episodes as played, and the circled tick unsubscribes. The podcast menu can also open the show's website.

Each episode row has artwork, date, title, a short description, and a play button on the right. Select the row to read the full episode description in a native dialog. The row's play button and the podcast page's Latest episode button stream audio from the RSS enclosure. A player bar spans the bottom of the window with play/pause, 10-second skips, seeking, time, and speeds from 0.75× to 2×. It stays visible across pages and hides artwork and titles at narrow widths. Playback positions are saved periodically, when pausing or switching episodes, and when closing the app. Reopening the app restores the last episode and selected speed in a paused player bar; pressing Play resumes near the saved position. Completing an episode marks it as played and clears it from the next launch's player bar. Audio streaming requires a working network connection and a codec supported by the installed GStreamer plugins.

Right-click an episode row to Play Now or Queue Up. The menu stays closed for the current episode while it is playing or paused. New Episodes rows also offer Go to Show. Direct playback leaves the queue in place; after the current episode finishes, queued episodes play in order. The queue stays in memory and clears when the app closes. The sidebar button beside playback speed opens a native sliding Queue and History panel; a header button opens it when no episode is playing. The panel slides over the player. Completed episodes are saved to SQLite History until Clear History is used. Both tabs use native lists and provide controls to play entries again or remove queued entries.

GNOME media controls use MPRIS on the session D-Bus. They show the current episode title, podcast, artwork, duration, position, and speed, and can play, pause, stop, seek, or advance to the next queued episode. Previous is unavailable because the app does not yet keep a previous-track queue. The MPRIS name is `org.mpris.MediaPlayer2.elitepodcasts`.

New Episodes reads the latest 20 entries from each followed RSS feed only when you choose Check for New Episodes from its page menu or press Ctrl+R. Navigating away and back keeps the current list without fetching feeds again; reopening the app loads the saved list in a worker. A manual refresh keeps existing rows visible and adds results as individual feeds finish, so a slow feed does not hold up faster ones. It shows playable, unplayed episodes in local-date groups: Today, Yesterday, This week, This month, and Older. Rows use the same artwork, date, title, description, and play/pause button as the podcast page. Select a row for the same episode details dialog used on podcast pages. The page menu can open Keyboard Shortcuts with Ctrl+? and show About Elite Podcasts. Checking again removes episodes that have since been played. Feeds that fail to load do not prevent other followed shows from appearing.

Import Shows (Ctrl+O) reads podcast RSS subscriptions from an OPML file, skips duplicates, and adds the new shows to Following. Covers and missing titles are filled from RSS feeds in the background; unavailable feeds remain followed with their OPML titles. Export Shows (Ctrl+E) saves the current Following list as OPML 2.0. Both commands use native file dialogs and show an in-app result message. OPML transfers subscriptions, not listening history or downloaded audio.

The 10-second skip and speedometer symbolic icons in `assets/icons/` come from [GNOME Audio Player (Decibels)](https://gitlab.gnome.org/GNOME/decibels), licensed under GPL-3.0. They are bundled as application resources so GTK can recolor them with the active theme.

Images are loaded by four background workers, with alternate artwork URLs tried only when the preferred one fails. Text and controls appear without waiting for artwork. Images use a bounded in-memory cache and a persistent disk cache under the standard user cache directory, with a symbolic placeholder while loading or when unavailable. RSS feed data is also cached: fresh cached feeds load immediately for 15 minutes, and older cached feeds load immediately while a background refresh updates the cache. RSS feeds are limited to 32 MB; larger feeds show an explicit size error.

Search and category browsing need an internet connection on their first request. No API credentials are required.

## Project layout

- `src/app.rs` starts the libadwaita application and creates its window.
- `src/ui/` contains the adaptive sidebar, page header, discovery views, New Episodes page, shared episode rows and dialog, Queue and History panel, nine-category grid, and artwork loader.
- `src/data.rs` defines the category tiles.
- `src/discovery_service.rs` searches Apple's public podcast directory, loads category charts, and reads public RSS feeds.
- `src/cache.rs` stores fetched directory responses, RSS details, the New Episodes snapshot, and artwork in the user cache directory.
- `src/following.rs` saves followed podcasts in SQLite under the standard user data directory.
- `src/opml.rs` reads and writes portable OPML subscription lists; `src/ui/opml_io.rs` provides the native file dialogs.
- `src/playback.rs` manages GStreamer streaming and restores the last episode, speed, and resume position; `src/ui/player.rs` builds the adaptive player bar.
- `src/mpris.rs` connects that playback controller to GNOME's D-Bus media controls.
- `assets/artwork/` retains original local SVG placeholder art for a possible future feature; the carousel is currently hidden.
- `assets/style.css` adds restrained styling; the accent CSS files use Apple blue in light and dark mode.

## Technology choices

The app uses GTK4 through gtk4-rs 0.9, libadwaita through libadwaita-rs 0.7, and GStreamer through gstreamer-rs 0.23. The discovery client uses ureq 2.12 and serde; SHA-1 is used for cache file names and stable MPRIS track paths. RSS is parsed with roxmltree. Following and resume positions use rusqlite with bundled SQLite, so no system SQLite development package is needed. MPRIS uses the GIO D-Bus support already included with GTK. GSettings and downloads remain future work.

The proposed application ID `io.github.elite.Podcasts` is temporary. Confirm a final reverse-DNS ID before packaging or publishing.

Trending Podcasts, Top Shows, and Recently Updated remain for later passes.

## License

GPL-3.0-or-later.
