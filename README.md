# Elite Podcasts

Elite Podcasts is a native GTK4 and libadwaita podcast application for GNOME. Search combines live fyyd and Apple Podcasts results, and selecting a result shows recent episodes from its public RSS feed. The window uses a Settings-style sidebar, a current-page title, and a loading spinner beside that title. It opens at 960×680 and has a 700×520 minimum size. Following and playback positions are saved locally in SQLite. Audio playback uses GStreamer.

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

Select the search button, type a podcast name, and press Enter. Search combines fyyd's public directory with Apple's documented podcast search; neither needs a key. If one directory fails, the other's results still appear. Category tiles load Podcast Index trending feeds with API credentials, except Climate, which uses a public text search. Network requests run in worker threads so the window remains responsive.

Search and category results use native Adwaita boxed lists with artwork, show and publisher names, and an available date above the title. Dates use the local day: Today, Yesterday, day and month, or a year when needed. Exact title matches appear first. A search briefly outlines its first result in the accent color. Select a row to see a podcast cover, description, and up to 20 recent episodes from its public RSS feed. Use Add to follow a show; the button becomes green and says Added. The Following page displays a responsive cover grid, and its covers open the podcast page. The podcast page shortens long descriptions with an inline More link that opens a native dialog. Beneath it, the headphones button and podcast menu can mark the loaded episodes as played, and the circled tick unsubscribes. The podcast menu can also open the show's website.

Each episode row has artwork, date, title, a short description, and a play button on the right. Select the row to read the full episode description in a native dialog. The row's play button and the podcast page's Latest episode button stream audio from the RSS enclosure. A player bar spans the bottom of the window with play/pause, 10-second skips, seeking, time, and speeds from 0.75× to 2×. It stays visible across pages and hides artwork and titles at narrow widths. Playback positions are saved periodically, when pausing or switching episodes, and when closing the app; reopening an episode resumes near that position. Completing an episode marks it as played. Audio streaming requires a working network connection and a codec supported by the installed GStreamer plugins.

The 10-second skip and speedometer symbolic icons in `assets/icons/` come from [GNOME Audio Player (Decibels)](https://gitlab.gnome.org/GNOME/decibels), licensed under GPL-3.0. They are bundled as application resources so GTK can recolor them with the active theme.

Images are loaded by four background workers, with alternate artwork URLs tried only when the preferred one fails. Images stay in a bounded in-memory cache and use a symbolic placeholder when unavailable. The list appears after its artwork batch finishes or reaches the 15-second limit. The cache is cleared when the app exits. RSS feeds are limited to 32 MB; larger feeds show an explicit size error.

To enable category browsing, obtain your own Podcast Index API key and secret and launch the app with these environment variables set:

```sh
export PODCAST_INDEX_KEY='your-key'
export PODCAST_INDEX_SECRET='your-secret'
cargo run
```

Keep credentials out of the repository. If either value is missing, the category page explains what is needed. Podcast Index credentials have not been used to verify category responses yet.

## Project layout

- `src/app.rs` starts the libadwaita application and creates its window.
- `src/ui/` contains the adaptive sidebar, page header, discovery views, nine-category grid, and artwork loader.
- `src/data.rs` defines the category tiles.
- `src/discovery_service.rs` combines fyyd and Apple Podcasts search, handles authenticated Podcast Index category requests, and reads public RSS feeds.
- `src/following.rs` saves followed podcasts in SQLite under the standard user data directory.
- `src/playback.rs` manages GStreamer streaming and saved resume positions; `src/ui/player.rs` builds the adaptive player bar.
- `assets/artwork/` retains original local SVG placeholder art for a possible future feature; the carousel is currently hidden.
- `assets/style.css` adds restrained styling; the accent CSS files use Apple blue in light and dark mode.

## Technology choices

The app uses GTK4 through gtk4-rs 0.9, libadwaita through libadwaita-rs 0.7, and GStreamer through gstreamer-rs 0.23. The discovery client uses ureq 2.12, serde, and SHA-1 for authenticated Podcast Index category requests. RSS is parsed with roxmltree. Following and resume positions use rusqlite with bundled SQLite, so no system SQLite development package is needed. GSettings, MPRIS, downloads, and a playback queue remain future work.

The proposed application ID `io.github.elite.Podcasts` is temporary. Confirm a final reverse-DNS ID before packaging or publishing.

Trending Podcasts, Top Shows, and Recently Updated remain for later passes.

## License

GPL-3.0-or-later.
