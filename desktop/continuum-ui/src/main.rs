//! Continuum desktop app (GTK4 + libadwaita). Holds no connection state: everything comes from
//! the daemon over D-Bus, so the window can be closed at any time.

mod ctx;
mod daemon;
mod messages;
mod notifications;
mod photos;
mod qr;
mod storage;
mod window;

use adw::prelude::*;

const APP_ID: &str = "io.github.sharjeelmazhar.Continuum";

fn main() -> glib::ExitCode {
    let app = adw::Application::builder().application_id(APP_ID).build();
    app.connect_activate(|app| {
        if let Some(win) = app
            .windows()
            .into_iter()
            .find(|w| w.is::<adw::ApplicationWindow>())
        {
            win.present();
            return;
        }
        let win = window::build(app);
        // Started by the service just to show a quick reply: the (hidden) main window keeps
        // the app running, and appears when the app is opened.
        if std::env::var_os("CONTINUUM_BACKGROUND").is_none() {
            win.present();
        }
    });
    app.run()
}
