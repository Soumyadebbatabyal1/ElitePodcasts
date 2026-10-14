# Contributor guidance

- Keep the application native: Rust, GTK4, and libadwaita. Do not add web views or web UI frameworks.
- Keep credentials in environment variables; never commit API keys or secrets.
- Keep the parked fictional artwork local to `assets/artwork/` if it is used again.
- Follow GNOME interaction patterns, support the active light or dark Adwaita theme, and keep the discovery page usable at narrow window widths.
- Put application startup in `src/app.rs`, categories in `src/data.rs`, network discovery in `src/discovery_service.rs`, and interface code in `src/ui/`.
- Do not add Spotify integration.
- Do not change the proposed application ID for a package or release until its final value is confirmed.
- Build with `cargo build` after Rust changes when the required GTK development libraries are available.
