use adw::prelude::*;

pub fn run() {
    let app = adw::Application::builder()
        .application_id("io.github.elite.Podcasts")
        .build();
    app.connect_activate(|app| {
        let window = adw::ApplicationWindow::builder()
            .application(app)
            .title("Elite Podcasts")
            .default_width(1120)
            .default_height(820)
            .content(&crate::ui::window_content())
            .build();
        window.present();
    });
    app.run();
}
