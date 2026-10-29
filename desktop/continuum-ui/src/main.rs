//! Continuum desktop app (GTK4 + libadwaita). Holds no connection state: everything comes from
//! the daemon over D-Bus, so the window can be closed at any time.

mod daemon;
mod qr;
mod window;

use adw::prelude::*;

const APP_ID: &str = "io.github.sharjeelmazhar.Continuum";

fn main() -> glib::ExitCode {
    let app = adw::Application::builder().application_id(APP_ID).build();
    app.connect_activate(|app| {
        if let Some(win) = app.active_window() {
            win.present();
            return;
        }
        window::build(app).present();
    });
    app.run()
}
