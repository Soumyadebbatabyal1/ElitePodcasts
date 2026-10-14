# Contributor guidance

- Keep the application native: Rust, GTK4, and libadwaita. Do not add web views or web UI frameworks.
- Keep Phase 1 independent of network services, credentials, playback, persistent storage, and external podcast data.
- Keep sample content clearly identifiable as placeholder data and keep its artwork local to `assets/artwork/`.
- Follow GNOME interaction patterns, support the active light or dark Adwaita theme, and keep the discovery page usable at narrow window widths.
- Put application startup in `src/app.rs`, data definitions in `src/data.rs`, and interface code in `src/ui/`.
- Do not add Spotify integration.
- Do not change the proposed application ID for a package or release until its final value is confirmed.
- Build with `cargo build` after Rust changes when the required GTK development libraries are available.
