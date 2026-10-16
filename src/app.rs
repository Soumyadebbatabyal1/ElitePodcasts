use adw::prelude::*;

pub fn run() {
    let bytes = gtk::glib::Bytes::from_static(include_bytes!(concat!(env!("OUT_DIR"), "/icons.gresource")));
    let resource = gtk::gio::Resource::from_data(&bytes).expect("invalid bundled player icons");
    gtk::gio::resources_register(&resource);
    let app = adw::Application::builder()
        .application_id("io.github.elite.Podcasts")
        .build();
    app.connect_activate(|app| {
        if let Some(window) = app.active_window() {
            window.present();
            return;
        }
        if let Some(display) = gtk::gdk::Display::default() {
            gtk::IconTheme::for_display(&display)
                .add_resource_path("/io/github/elite/Podcasts/icons");
        }
        let window = adw::ApplicationWindow::builder()
            .application(app)
            .title("Elite Podcasts")
            .default_width(960)
            .default_height(680)
            .build();
        window.set_size_request(700, 520);
        window.set_content(Some(&crate::ui::window_content(&window, app)));
        window.present();
    });
    app.run();
}
