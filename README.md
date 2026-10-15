# Elite Podcasts

Elite Podcasts is a native GTK4 and libadwaita podcast application for GNOME. Search combines live fyyd and Apple Podcasts results, and selecting a result shows recent episodes from its public RSS feed. The window uses a Settings-style sidebar, a current-page title, and a loading spinner beside that title. It opens at 960×680 and has a 700×520 minimum size. Following is saved locally in SQLite. Playback is planned for a later phase.

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

Select the search button, type a podcast name, and press Enter. Search combines fyyd's public directory with Apple's documented podcast search; neither needs a key. If one directory fails, the other's results still appear. Category tiles load Podcast Index trending feeds with API credentials, except Climate, which uses a public text search. Network requests run in worker threads so the window remains responsive.

Search and category results use native Adwaita boxed lists with artwork, show and publisher names, and an available date above the title. Dates use the local day: Today, Yesterday, day and month, or a year when needed. Exact title matches appear first. A search briefly outlines its first result in the accent color. Select a row to see a larger cover and read its RSS title, description when available as plain text, and up to 20 recent episode titles and dates. Use Add to follow a show; the button becomes green and says Added. The Following page displays a cover grid, and its covers open the podcast page. The podcast page shortens long descriptions with an inline More link that opens a native dialog. Beneath it, the headphones button and podcast menu can mark the loaded episodes as played, the circled tick unsubscribes, and the Latest episode button shows a play icon but remains disabled until playback is implemented. The podcast menu can also open the show's website. Native in-app messages confirm following, unsubscribing, and marking episodes played. Images are loaded by four background workers, with alternate artwork URLs tried only when the preferred one fails. Images stay in a bounded in-memory cache and use a symbolic placeholder when unavailable. The list appears after its artwork batch finishes or reaches the 15-second limit. The cache is cleared when the app exits. RSS feeds are limited to 32 MB; larger feeds show an explicit size error.

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
- `assets/artwork/` retains original local SVG placeholder art for a possible future feature; the carousel is currently hidden.
- `assets/style.css` adds restrained styling; the accent CSS files use Apple blue in light and dark mode.

## Technology choices

The app uses GTK4 through gtk4-rs 0.9 and libadwaita through libadwaita-rs 0.7. The discovery client uses ureq 2.12, serde, and SHA-1 for authenticated Podcast Index category requests. RSS is parsed with roxmltree. Following uses rusqlite with bundled SQLite, so no system SQLite development package is needed. Playback, GSettings, and MPRIS remain future work.

The proposed application ID `io.github.elite.Podcasts` is temporary. Confirm a final reverse-DNS ID before packaging or publishing.

Trending Podcasts, Top Shows, and Recently Updated remain for later passes. Audio playback and episode descriptions are not yet available.

## License

GPL-3.0-or-later.
