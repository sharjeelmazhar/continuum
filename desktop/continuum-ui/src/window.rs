//! Main window: device list, the empty "pair your phone" state, and the QR pairing dialog.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use adw::prelude::*;
use futures_util::StreamExt;

use crate::daemon::{self, DaemonProxy, DeviceRow};
use crate::qr;

struct Ui {
    window: adw::ApplicationWindow,
    stack: gtk::Stack,
    page: adw::PreferencesPage,
    group: RefCell<Option<adw::PreferencesGroup>>,
    toasts: adw::ToastOverlay,
    add_button: gtk::Button,
    error_page: adw::StatusPage,
    proxy: RefCell<Option<DaemonProxy<'static>>>,
    pair_dialog: RefCell<Option<adw::Dialog>>,
    /// Bumped whenever a pairing dialog opens or closes, so stale countdowns stop.
    pair_generation: Cell<u64>,
}

pub fn build(app: &adw::Application) -> adw::ApplicationWindow {
    let stack = gtk::Stack::builder()
        .transition_type(gtk::StackTransitionType::Crossfade)
        .build();

    let spinner = gtk::Spinner::builder()
        .spinning(true)
        .width_request(32)
        .height_request(32)
        .halign(gtk::Align::Center)
        .valign(gtk::Align::Center)
        .build();
    stack.add_named(&spinner, Some("loading"));

    let pair_button = gtk::Button::builder()
        .label("Pair a Phone")
        .halign(gtk::Align::Center)
        .css_classes(["pill", "suggested-action"])
        .build();
    let empty = adw::StatusPage::builder()
        .icon_name("phone-symbolic")
        .title("Pair Your Phone")
        .description("Install Continuum on your Android phone, then scan a code to connect it to this computer.")
        .child(&pair_button)
        .build();
    stack.add_named(&empty, Some("empty"));

    let page = adw::PreferencesPage::new();
    stack.add_named(&page, Some("list"));

    let error_page = adw::StatusPage::builder()
        .icon_name("dialog-error-symbolic")
        .title("Continuum Isn't Running")
        .build();
    stack.add_named(&error_page, Some("error"));

    let add_button = gtk::Button::builder()
        .icon_name("list-add-symbolic")
        .tooltip_text("Pair a Phone")
        .visible(false)
        .build();
    let header = adw::HeaderBar::new();
    header.pack_start(&add_button);

    let toasts = adw::ToastOverlay::new();
    toasts.set_child(Some(&stack));
    let view = adw::ToolbarView::new();
    view.add_top_bar(&header);
    view.set_content(Some(&toasts));

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("Continuum")
        .default_width(520)
        .default_height(600)
        .content(&view)
        .build();

    let ui = Rc::new(Ui {
        window: window.clone(),
        stack,
        page,
        group: RefCell::new(None),
        toasts,
        add_button: add_button.clone(),
        error_page,
        proxy: RefCell::new(None),
        pair_dialog: RefCell::new(None),
        pair_generation: Cell::new(0),
    });

    for button in [&pair_button, &add_button] {
        let ui = ui.clone();
        button.connect_clicked(move |_| open_pairing(&ui));
    }

    let ui2 = ui.clone();
    glib::spawn_future_local(async move { start(ui2).await });
    window
}

async fn start(ui: Rc<Ui>) {
    let proxy = match daemon::connect().await {
        Ok(p) => p,
        Err(e) => {
            ui.error_page
                .set_description(Some(&glib::markup_escape_text(&e)));
            ui.stack.set_visible_child_name("error");
            return;
        }
    };
    *ui.proxy.borrow_mut() = Some(proxy.clone());
    refresh(&ui).await;

    if let Ok(mut changed) = proxy.receive_devices_changed().await {
        let ui = ui.clone();
        glib::spawn_future_local(async move {
            while changed.next().await.is_some() {
                refresh(&ui).await;
            }
        });
    }
    if let Ok(mut paired) = proxy.receive_paired().await {
        glib::spawn_future_local(async move {
            while let Some(signal) = paired.next().await {
                let name = signal
                    .args()
                    .map(|a| a.name.to_string())
                    .unwrap_or_default();
                if let Some(dialog) = ui.pair_dialog.borrow_mut().take() {
                    dialog.close();
                }
                ui.toasts
                    .add_toast(adw::Toast::new(&format!("Paired with {name}")));
            }
        });
    }
}

fn proxy(ui: &Ui) -> Option<DaemonProxy<'static>> {
    ui.proxy.borrow().clone()
}

async fn refresh(ui: &Rc<Ui>) {
    let Some(proxy) = proxy(ui) else { return };
    let devices = match proxy.list_devices().await {
        Ok(d) => d,
        Err(e) => {
            ui.error_page
                .set_description(Some(&glib::markup_escape_text(&e.to_string())));
            ui.stack.set_visible_child_name("error");
            return;
        }
    };
    ui.add_button.set_visible(!devices.is_empty());
    if devices.is_empty() {
        ui.stack.set_visible_child_name("empty");
        return;
    }
    let group = adw::PreferencesGroup::builder().title("Devices").build();
    for d in &devices {
        group.add(&device_row(ui, d));
    }
    if let Some(old) = ui.group.replace(Some(group.clone())) {
        ui.page.remove(&old);
    }
    ui.page.add(&group);
    ui.stack.set_visible_child_name("list");
}

fn status_text(connected: bool, battery: i32, charging: bool) -> String {
    match (connected, battery) {
        (false, _) => "Not connected".to_owned(),
        (true, b) if b < 0 => "Connected".to_owned(),
        (true, b) if charging => format!("Connected · {b}% battery, charging"),
        (true, b) => format!("Connected · {b}% battery"),
    }
}

fn device_row(
    ui: &Rc<Ui>,
    (id, name, ty, connected, battery, charging): &DeviceRow,
) -> adw::ActionRow {
    let row = adw::ActionRow::builder()
        .title(name)
        .use_markup(false)
        .subtitle(status_text(*connected, *battery, *charging))
        .build();
    let icon = match ty.as_str() {
        "laptop" | "desktop" => "computer-symbolic",
        _ => "phone-symbolic",
    };
    row.add_prefix(&gtk::Image::from_icon_name(icon));
    let dot = gtk::Image::from_icon_name(if *connected {
        "object-select-symbolic"
    } else {
        "network-offline-symbolic"
    });
    dot.add_css_class(if *connected { "success" } else { "dim-label" });
    row.add_suffix(&dot);
    let remove = gtk::Button::builder()
        .icon_name("user-trash-symbolic")
        .tooltip_text("Remove this device")
        .valign(gtk::Align::Center)
        .css_classes(["flat"])
        .build();
    let (ui, id, name) = (ui.clone(), id.clone(), name.clone());
    remove.connect_clicked(move |_| confirm_unpair(&ui, &id, &name));
    row.add_suffix(&remove);
    row
}

fn confirm_unpair(ui: &Rc<Ui>, id: &str, name: &str) {
    let dialog = adw::AlertDialog::new(
        Some(&format!("Remove {name}?")),
        Some(&format!(
            "{name} will no longer connect to this computer. You can pair it again at any time."
        )),
    );
    dialog.add_responses(&[("cancel", "Cancel"), ("remove", "Remove")]);
    dialog.set_response_appearance("remove", adw::ResponseAppearance::Destructive);
    dialog.set_close_response("cancel");
    let (ui_ref, id_ref) = (ui.clone(), id.to_owned());
    dialog.connect_response(None, move |_, response| {
        if response != "remove" {
            return;
        }
        let (ui, id) = (ui_ref.clone(), id_ref.clone());
        glib::spawn_future_local(async move {
            if let Some(p) = proxy(&ui)
                && let Err(e) = p.unpair(&id).await
            {
                ui.toasts.add_toast(adw::Toast::new(&format!(
                    "Could not remove the device: {e}"
                )));
            }
        });
    });
    dialog.present(Some(&ui.window));
}

fn open_pairing(ui: &Rc<Ui>) {
    let Some(proxy) = proxy(ui) else { return };
    let generation = ui.pair_generation.get() + 1;
    ui.pair_generation.set(generation);

    let picture = gtk::Picture::builder()
        .width_request(280)
        .height_request(280)
        .can_shrink(true)
        .halign(gtk::Align::Center)
        .build();
    picture.add_css_class("card");
    picture.set_overflow(gtk::Overflow::Hidden);
    let steps = gtk::Label::builder()
        .label("1. Install Continuum on your Android phone.\n2. Open it and tap <b>Scan QR Code</b>.\n3. Point the camera at this code.")
        .use_markup(true)
        .wrap(true)
        .xalign(0.0)
        .build();
    let expiry = gtk::Label::builder()
        .css_classes(["dim-label", "caption"])
        .build();
    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(18)
        .margin_top(12)
        .margin_bottom(24)
        .margin_start(24)
        .margin_end(24)
        .build();
    content.append(&picture);
    content.append(&steps);
    content.append(&expiry);
    let view = adw::ToolbarView::new();
    view.add_top_bar(&adw::HeaderBar::new());
    view.set_content(Some(&content));
    let dialog = adw::Dialog::builder()
        .title("Pair a Phone")
        .content_width(400)
        .child(&view)
        .build();

    let ui2 = ui.clone();
    let proxy2 = proxy.clone();
    dialog.connect_closed(move |_| {
        if ui2.pair_generation.get() == generation {
            ui2.pair_generation.set(generation + 1);
            ui2.pair_dialog.borrow_mut().take();
            let p = proxy2.clone();
            glib::spawn_future_local(async move {
                let _ = p.cancel_pairing().await;
            });
        }
    });
    *ui.pair_dialog.borrow_mut() = Some(dialog.clone());
    dialog.present(Some(&ui.window));

    // Show a code, count down, and replace it with a fresh one when it expires.
    let ui = ui.clone();
    glib::spawn_future_local(async move {
        while ui.pair_generation.get() == generation {
            let (uri, ttl) = match proxy.start_pairing().await {
                Ok(r) => r,
                Err(e) => {
                    expiry.set_label(&format!("Could not create a pairing code: {e}"));
                    return;
                }
            };
            picture.set_paintable(qr::texture(&uri).as_ref());
            for left in (1..=ttl).rev() {
                if ui.pair_generation.get() != generation {
                    return;
                }
                expiry.set_label(&format!(
                    "This code works once and changes in {}:{:02}.",
                    left / 60,
                    left % 60
                ));
                glib::timeout_future(Duration::from_secs(1)).await;
            }
        }
    });
}
