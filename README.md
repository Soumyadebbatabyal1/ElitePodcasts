# Elite Podcasts

Elite Podcasts is a native GTK4 and libadwaita podcast discovery and playback application for GNOME. The project is currently in **Phase 1: Application Foundation and UI Shell**. All podcast listings and artwork are local placeholders; discovery, feeds, playback, and storage are not connected yet.

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

Run it from the project directory so the relative paths to the placeholder SVG artwork resolve. Cargo downloads the Rust crates on the first build.

## Project layout

- `src/app.rs` starts the libadwaita application and creates its window.
- `src/ui/` contains the header, navigation, responsive discovery page, featured carousel, and a nine-category grid.
- `src/data.rs` defines sample podcasts and categories.
- `assets/artwork/` contains original local SVG placeholder art.
- `assets/style.css` adds restrained styling on top of the active Adwaita light or dark theme.

## Technology choices

The shell uses GTK4 through gtk4-rs 0.9 and libadwaita through libadwaita-rs 0.7. These API generations work together and keep the Rust compiler baseline modest. Playback, Podcast Index, RSS networking, SQLite, GSettings, and MPRIS are planned for later phases and are intentionally absent from this shell.

The proposed application ID `io.github.elite.Podcasts` is temporary. Confirm a final reverse-DNS ID before packaging or publishing.

Trending Podcasts, Top Shows, Recently Updated, and podcast cards are planned for a later discovery pass.

## License

GPL-3.0-or-later.
