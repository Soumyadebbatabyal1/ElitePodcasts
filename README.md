# Elite Podcasts

Elite Podcasts is a native GTK4 and libadwaita podcast application for GNOME. Search reads live Podcast Index results, and selecting a result shows recent episodes from its public RSS feed. The window uses a Settings-style sidebar, a current-page title, and a loading spinner beside that title. Playback and following are planned for later phases.

## Requirements

- Rust stable and Cargo (Rust 1.78 or newer)
- GTK 4.8 or newer development files
- libadwaita development files (1.5 or newer)
- A C compiler and `pkg-config`

On Fedora, the relevant packages are `rust`, `cargo`, `gtk4-devel`, `libadwaita-devel`, and `gcc`.

## Build and run

```sh
cargo run
```

Cargo downloads the Rust crates on the first build. Close any older Elite Podcasts window before launching a newly built version; the application ID allows only one running instance.

Select the search button, type a podcast name, and press Enter. Search uses Podcast Index's public keyless endpoint. Category tiles load trending feeds with API credentials, except Climate, which uses a public text search. Network requests run in a worker thread so the window remains responsive.

Select a result to read its RSS title, description when available as plain text, and up to 20 recent episode titles and dates. Episode playback is not implemented yet.

To enable category browsing, obtain your own Podcast Index API key and secret and launch the app with these environment variables set:

```sh
export PODCAST_INDEX_KEY='your-key'
export PODCAST_INDEX_SECRET='your-secret'
cargo run
```

Keep credentials out of the repository. If either value is missing, the category page explains what is needed. Podcast Index credentials have not been used to verify category responses yet.

## Project layout

- `src/app.rs` starts the libadwaita application and creates its window.
- `src/ui/` contains the adaptive sidebar, page header, discovery views, and nine-category grid.
- `src/data.rs` defines the category tiles.
- `src/discovery_service.rs` handles Podcast Index requests and reads public RSS feeds.
- `assets/artwork/` retains original local SVG placeholder art for a possible future feature; the carousel is currently hidden.
- `assets/style.css` adds restrained styling; the accent CSS files use Apple blue in light and dark mode.

## Technology choices

The app uses GTK4 through gtk4-rs 0.9 and libadwaita through libadwaita-rs 0.7. The discovery client uses ureq 2.12, serde, and SHA-1 for Podcast Index's documented authentication scheme. RSS is parsed with roxmltree. Playback, SQLite, GSettings, and MPRIS remain future work.

The proposed application ID `io.github.elite.Podcasts` is temporary. Confirm a final reverse-DNS ID before packaging or publishing.

Trending Podcasts, Top Shows, and Recently Updated remain for later passes. Search and category results currently show show names, publishers, and available descriptions with a symbolic placeholder instead of downloaded artwork. The RSS view is informational: audio playback, episode descriptions, and subscriptions are not yet available.

## License

GPL-3.0-or-later.
