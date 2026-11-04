//! Continuum desktop app (GTK4 + libadwaita). Holds no connection state: everything comes from
//! the daemon over D-Bus, so the window can be closed at any time.

mod autotext;
mod battery;
mod ctx;
mod daemon;
mod messages;
mod mirror;
mod models;
mod notifications;
mod photos;
mod player;
mod qr;
mod storage;
mod textsize;
mod transfers;
mod window;

use adw::prelude::*;

const APP_ID: &str = "io.github.sharjeelmazhar.Continuum";
/// Phone Mirroring is its own app (own dock icon), like iPhone Mirroring on a Mac.
const MIRROR_ID: &str = "io.github.sharjeelmazhar.Continuum.Mirror";

fn main() -> glib::ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--mirror") {
        return mirror_app();
    }
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

/// `continuum-ui --mirror [DEVICE-ID]`: the Phone Mirroring app. Without an id it mirrors the
/// connected phone (or the first paired one, which then says it isn't connected). One process
/// serves every phone: a later launch hands its id to the running one, which opens (or brings
/// forward) that phone's window.
fn mirror_app() -> glib::ExitCode {
    let app = adw::Application::builder()
        .application_id(MIRROR_ID)
        .flags(gtk::gio::ApplicationFlags::HANDLES_COMMAND_LINE)
        .build();
    app.connect_command_line(|app, cmd| {
        let args: Vec<String> = cmd
            .arguments()
            .iter()
            .filter_map(|a| a.to_str().map(str::to_owned))
            .collect();
        let device = args
            .iter()
            .position(|a| a == "--mirror")
            .and_then(|i| args.get(i + 1))
            .filter(|id| !id.is_empty() && id.bytes().all(|b| b.is_ascii_hexdigit()))
            .cloned();
        // No phone named and a window is up: just bring it forward.
        if device.is_none()
            && let Some(w) = app.windows().first()
        {
            w.present();
            return glib::ExitCode::SUCCESS;
        }
        start_mirror(app, device);
        glib::ExitCode::SUCCESS
    });
    app.run_with_args(&std::env::args().collect::<Vec<_>>())
}

/// Finds the phone (`device`, else the connected one, else the first paired) and opens its window.
fn start_mirror(app: &adw::Application, device: Option<String>) {
    window::load_css();
    let hold = app.hold();
    let app = app.clone();
    glib::spawn_future_local(async move {
        let picked = match daemon::connect().await {
            Ok(proxy) => {
                let rows = proxy.list_devices().await.unwrap_or_default();
                let row = rows
                    .iter()
                    .find(|r| device.as_deref() == Some(r.0.as_str()))
                    .or_else(|| rows.iter().find(|r| r.3))
                    .or(rows.first())
                    .cloned();
                row.map(|r| (proxy, r))
                    .ok_or_else(|| "Pair your phone in Continuum first, then try again.".to_owned())
            }
            Err(e) => Err(format!("Continuum isn't running: {e}")),
        };
        match picked {
            Ok((proxy, (id, name, ty, connected, battery, charging))) => {
                let d = ctx::Device {
                    id,
                    name,
                    ty,
                    connected,
                    battery,
                    charging,
                };
                mirror::open(app.upcast_ref(), proxy, d);
                drop(hold);
            }
            Err(text) => {
                let dialog = adw::AlertDialog::new(Some("Can't mirror your phone"), Some(&text));
                dialog.add_response("close", "Close");
                // The app stays until the message is closed.
                dialog.connect_closed(move |_| {
                    let _ = &hold;
                });
                dialog.present(None::<&gtk::Widget>);
            }
        }
    });
}
