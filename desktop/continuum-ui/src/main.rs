//! Continuum desktop app (GTK4 + libadwaita).

use adw::prelude::*;

const APP_ID: &str = "org.continuum.Continuum";

fn main() -> glib::ExitCode {
    let app = adw::Application::builder().application_id(APP_ID).build();
    app.connect_activate(|app| {
        let page = adw::StatusPage::builder()
            .icon_name("phone-symbolic")
            .title("Continuum")
            .description("No devices paired yet")
            .build();
        let view = adw::ToolbarView::new();
        view.add_top_bar(&adw::HeaderBar::new());
        view.set_content(Some(&page));
        adw::ApplicationWindow::builder()
            .application(app)
            .title("Continuum")
            .default_width(480)
            .default_height(560)
            .content(&view)
            .build()
            .present();
    });
    app.run()
}
