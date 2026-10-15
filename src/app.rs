use adw::prelude::*;

pub fn run() {
    let app = adw::Application::builder()
        .application_id("io.github.elite.Podcasts")
        .build();
    app.connect_activate(|app| {
        let window = adw::ApplicationWindow::builder()
            .application(app)
            .title("Elite Podcasts")
            .default_width(960)
            .default_height(680)
            .content(&crate::ui::window_content())
            .build();
        window.set_size_request(700, 520);
        window.present();
    });
    app.run();
}
