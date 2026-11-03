//! Main window, Phone Link style: a sidebar with the phone (picture, battery) and its
//! features, and the selected feature on the right. Also the QR pairing dialog.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use adw::prelude::*;
use futures_util::StreamExt;

use crate::ctx::{self, Ctx, Device};
use crate::daemon::{self, DaemonProxy};
use crate::{battery, messages, notifications, photos, qr, storage};

/// The feature pages, in sidebar order: (stack name, title, icons to try).
const PAGES: &[(&str, &str, &[&str])] = &[
    ("photos", "Photos", &["image-x-generic-symbolic"]),
    (
        "messages",
        "Messages",
        &["chat-bubble-text-symbolic", "mail-unread-symbolic"],
    ),
    (
        "notifications",
        "Notifications",
        &["preferences-system-notifications-symbolic"],
    ),
    ("storage", "Phone Storage", &["folder-symbolic"]),
    ("files", "Send Files", &["document-send-symbolic"]),
];

/// The capability each page needs from the phone ("" = always there).
fn page_cap(page: &str) -> &'static str {
    match page {
        "photos" => "photos",
        "messages" => "sms",
        "notifications" => "notif",
        "storage" => "files",
        _ => "",
    }
}

struct Ui {
    ctx: Rc<Ctx>,
    window: adw::ApplicationWindow,
    toasts: adw::ToastOverlay,
    stack: gtk::Stack,
    error_page: adw::StatusPage,
    pair_dialog: RefCell<Option<adw::Dialog>>,
    /// Bumped whenever a pairing dialog opens or closes, so stale countdowns stop.
    pair_generation: Cell<u64>,
    devices: RefCell<Vec<Device>>,
    // Sidebar
    picture: gtk::Image,
    phone_name: gtk::Label,
    /// Connection badge (green check or grey dot) and its word.
    link_icon: gtk::Image,
    link_label: gtk::Label,
    battery: battery::Battery,
    battery_label: gtk::Label,
    switcher: gtk::DropDown,
    nav: gtk::ListBox,
    // Content
    pages: gtk::Stack,
    page_title: adw::WindowTitle,
    split: adw::NavigationSplitView,
    refresh: gtk::Button,
    files_page: adw::StatusPage,
    photos: Rc<photos::Photos>,
    messages: Rc<messages::Messages>,
    notifications: Rc<notifications::Notifications>,
    storage: Rc<storage::Storage>,
    ring: gtk::Button,
    /// (page, device id) each page last loaded, so a page reloads only when needed.
    loaded: RefCell<Vec<(String, String)>>,
    /// Set while `refresh` fills the switcher, whose selection changes as a side effect.
    filling: Cell<bool>,
}

pub fn build(app: &adw::Application) -> adw::ApplicationWindow {
    load_css();
    let stack = gtk::Stack::builder()
        .transition_type(gtk::StackTransitionType::Crossfade)
        .build();
    stack.add_named(
        &gtk::Spinner::builder()
            .spinning(true)
            .width_request(32)
            .height_request(32)
            .halign(gtk::Align::Center)
            .valign(gtk::Align::Center)
            .build(),
        Some("loading"),
    );

    let pair_button = gtk::Button::builder()
        .label("Pair a Phone")
        .halign(gtk::Align::Center)
        .css_classes(["pill", "suggested-action"])
        .build();
    let empty = adw::StatusPage::builder()
        .paintable(&device_texture("phone"))
        .title("Pair Your Phone")
        .description("Install Continuum on your Android phone, then scan a code to see your photos, messages and notifications here.")
        .child(&pair_button)
        .build();
    stack.add_named(&empty, Some("empty"));

    let error_page = adw::StatusPage::builder()
        .icon_name("dialog-error-symbolic")
        .title("Continuum Isn't Running")
        .build();
    stack.add_named(&error_page, Some("error"));

    let toasts = adw::ToastOverlay::new();
    toasts.set_child(Some(&stack));
    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("Continuum")
        .default_width(1180)
        .default_height(780)
        .width_request(360)
        .height_request(400)
        .content(&toasts)
        .build();

    let ctx = Rc::new(Ctx {
        window: window.clone(),
        toasts: toasts.clone(),
        proxy: RefCell::new(None),
        device: RefCell::new(None),
    });

    // Sidebar: the phone, then the features.
    let picture = gtk::Image::builder().pixel_size(112).margin_top(12).build();
    let phone_name = gtk::Label::builder()
        .css_classes(["title-2"])
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .build();
    // Status line, Apple style: ✓ Connected   [▮▮▮ ] 82%
    let link_icon = gtk::Image::builder()
        .pixel_size(10)
        .valign(gtk::Align::Center)
        .build();
    let link_label = gtk::Label::builder().css_classes(["status-text"]).build();
    let battery = battery::Battery::new();
    let battery_label = gtk::Label::builder()
        .css_classes(["status-text", "numeric"])
        .build();
    let link = gtk::Box::builder().spacing(5).build();
    link.append(&link_icon);
    link.append(&link_label);
    let power = gtk::Box::builder().spacing(5).build();
    power.append(&battery.widget);
    power.append(&battery_label);
    let status = gtk::Box::builder()
        .spacing(16)
        .halign(gtk::Align::Center)
        .css_classes(["status-row"])
        .build();
    status.append(&link);
    status.append(&power);
    let switcher = gtk::DropDown::builder()
        .visible(false)
        .halign(gtk::Align::Center)
        .css_classes(["flat"])
        .build();
    let hero = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(6)
        .margin_top(6)
        .margin_bottom(22)
        .css_classes(["phone-hero"])
        .build();
    hero.append(&picture);
    phone_name.set_margin_top(8);
    hero.append(&phone_name);
    hero.append(&status);
    switcher.set_margin_top(6);
    hero.append(&switcher);
    let ring = gtk::Button::builder()
        .halign(gtk::Align::Center)
        .margin_top(8)
        .css_classes(["pill"])
        .tooltip_text("Ring your phone at full volume, even when it's on silent")
        .child(
            &adw::ButtonContent::builder()
                .icon_name("audio-volume-high-symbolic")
                .label("Ring")
                .build(),
        )
        .build();
    hero.append(&ring);

    let photos = photos::build(&ctx);
    let messages = messages::build(&ctx);
    let notifications = notifications::build(&ctx);
    let storage = storage::build(&ctx);

    let nav = gtk::ListBox::builder()
        .css_classes(["navigation-sidebar"])
        .build();
    for (_, title, icons) in PAGES {
        let row = gtk::Box::builder()
            .spacing(12)
            .margin_top(10)
            .margin_bottom(10)
            .margin_start(6)
            .build();
        row.append(&gtk::Image::from_icon_name(ctx::icon(icons)));
        row.append(
            &gtk::Label::builder()
                .label(*title)
                .hexpand(true)
                .xalign(0.0)
                .build(),
        );
        if *title == "Notifications" {
            row.append(&notifications.badge);
        }
        nav.append(&row);
    }
    let menu = gtk::gio::Menu::new();
    menu.append(Some("Pair a Phone…"), Some("win.pair"));
    menu.append(Some("Change Picture…"), Some("win.picture"));
    menu.append(Some("Remove This Phone…"), Some("win.unpair"));
    let menu_button = gtk::MenuButton::builder()
        .icon_name("open-menu-symbolic")
        .menu_model(&menu)
        .tooltip_text("Menu")
        .build();
    let side_header = adw::HeaderBar::new();
    side_header.pack_end(&menu_button);
    let side_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
    side_box.append(&hero);
    side_box.append(&nav);
    let side_view = adw::ToolbarView::new();
    side_view.add_top_bar(&side_header);
    side_view.set_content(Some(
        &gtk::ScrolledWindow::builder()
            .child(&side_box)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .build(),
    ));
    let sidebar = adw::NavigationPage::builder()
        .title("Continuum")
        .child(&side_view)
        .build();

    // Content: one page per feature.
    let pages = gtk::Stack::builder()
        .transition_type(gtk::StackTransitionType::Crossfade)
        .vexpand(true)
        .build();
    pages.add_named(&photos.widget, Some("photos"));
    pages.add_named(&messages.widget, Some("messages"));
    pages.add_named(&notifications.widget, Some("notifications"));
    pages.add_named(&storage.widget, Some("storage"));
    let choose = gtk::Button::builder()
        .label("Choose Files…")
        .halign(gtk::Align::Center)
        .css_classes(["pill", "suggested-action"])
        .build();
    let files_page = adw::StatusPage::builder()
        .icon_name("document-send-symbolic")
        .title("Send Files")
        .description("Drop files anywhere in this window, or choose them. They're saved in Downloads/Continuum on the phone.")
        .child(&choose)
        .build();
    pages.add_named(&files_page, Some("files"));

    let page_title = adw::WindowTitle::new("", "");
    {
        let (title, pages) = (page_title.clone(), pages.clone());
        photos.subtitle.connect_label_notify(move |l| {
            if pages.visible_child_name().as_deref() == Some("photos") {
                title.set_subtitle(&l.label());
            }
        });
    }
    let refresh = gtk::Button::builder()
        .icon_name("view-refresh-symbolic")
        .tooltip_text("Refresh")
        .action_name("win.reload")
        .build();
    let content_header = adw::HeaderBar::builder().title_widget(&page_title).build();
    content_header.pack_end(&refresh);
    content_header.pack_end(&notifications.clear);
    content_header.pack_end(&photos.fit);
    let content_view = adw::ToolbarView::new();
    content_view.add_top_bar(&content_header);
    content_view.set_content(Some(&pages));
    let content = adw::NavigationPage::builder()
        .title("Photos")
        .child(&content_view)
        .build();
    let split = adw::NavigationSplitView::builder()
        .sidebar(&sidebar)
        .content(&content)
        .min_sidebar_width(260.0)
        .max_sidebar_width(300.0)
        .build();
    stack.add_named(&split, Some("main"));
    // Narrow windows show one pane at a time.
    let bp = adw::Breakpoint::new(adw::BreakpointCondition::new_length(
        adw::BreakpointConditionLengthType::MaxWidth,
        720.0,
        adw::LengthUnit::Sp,
    ));
    bp.add_setter(&split, "collapsed", Some(&true.to_value()));
    window.add_breakpoint(bp);

    let ui = Rc::new(Ui {
        ctx,
        window: window.clone(),
        toasts,
        stack,
        error_page,
        pair_dialog: RefCell::new(None),
        pair_generation: Cell::new(0),
        devices: RefCell::default(),
        picture,
        phone_name,
        link_icon,
        link_label,
        battery,
        battery_label,
        switcher: switcher.clone(),
        nav: nav.clone(),
        pages,
        page_title,
        split: split.clone(),
        refresh,
        files_page,
        photos,
        messages,
        notifications,
        storage,
        ring: ring.clone(),
        loaded: RefCell::default(),
        filling: Cell::new(false),
    });

    {
        let ui = ui.clone();
        nav.connect_row_selected(move |_, row| {
            if let Some(row) = row {
                show_page(&ui, PAGES[row.index() as usize].0, false);
                ui.split.set_show_content(true);
            }
        });
    }
    {
        let ui = ui.clone();
        switcher.connect_selected_notify(move |dd| {
            if ui.filling.get() {
                return;
            }
            let d = ui.devices.borrow().get(dd.selected() as usize).cloned();
            if let Some(d) = d
                && ui.ctx.device().map(|c| c.id) != Some(d.id.clone())
            {
                select_device(&ui, d);
            }
        });
    }
    {
        let ui = ui.clone();
        pair_button.connect_clicked(move |_| open_pairing(&ui));
    }
    {
        let ui = ui.clone();
        choose.connect_clicked(move |_| {
            let Some(d) = ui.ctx.device() else { return };
            let ui = ui.clone();
            glib::spawn_future_local(async move { pick_and_send(&ui, &d.id, &d.name).await });
        });
    }
    {
        let ui = ui.clone();
        ring.connect_clicked(move |_| ring_phone(&ui));
    }
    add_actions(&ui);
    add_drop_target(&ui);

    let ui2 = ui.clone();
    glib::spawn_future_local(async move { start(ui2).await });
    dev_snapshots(&window, "");
    window
}

/// Development aid: with `CONTINUUM_SNAPSHOT=/path.png`, saves a picture of the window every
/// two seconds (GNOME doesn't let scripts take screenshots).
fn dev_snapshots(window: &impl IsA<gtk::Window>, suffix: &str) {
    let Some(path) = std::env::var_os("CONTINUUM_SNAPSHOT") else {
        return;
    };
    let mut path = std::path::PathBuf::from(path);
    if !suffix.is_empty() {
        path = path.with_extension(format!("{suffix}.png"));
    }
    let window = window.upcast_ref::<gtk::Window>().downgrade();
    glib::timeout_add_local(Duration::from_secs(2), move || {
        let Some(w) = window.upgrade() else {
            return glib::ControlFlow::Break;
        };
        let paintable = gtk::WidgetPaintable::new(Some(&w));
        let (width, height) = (w.width() as f64, w.height() as f64);
        let snap = gtk::Snapshot::new();
        paintable.snapshot(&snap, width, height);
        if let (Some(node), Some(renderer)) =
            (snap.to_node(), w.native().and_then(|n| n.renderer()))
        {
            let tmp = path.with_extension("part.png");
            if renderer
                .render_texture(&node, None)
                .save_to_png(&tmp)
                .is_ok()
            {
                let _ = std::fs::rename(&tmp, &path);
            }
        }
        glib::ControlFlow::Continue
    });
}

fn load_css() {
    let css = gtk::CssProvider::new();
    css.load_from_string(include_str!("style.css"));
    if let Some(display) = gtk::gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &css,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}

fn add_actions(ui: &Rc<Ui>) {
    let reload = gtk::gio::SimpleAction::new("reload", None);
    {
        let ui = ui.clone();
        reload.connect_activate(move |_, _| {
            if let Some(name) = ui.pages.visible_child_name() {
                show_page(&ui, &name, true);
            }
        });
    }
    let pair = gtk::gio::SimpleAction::new("pair", None);
    {
        let ui = ui.clone();
        pair.connect_activate(move |_, _| open_pairing(&ui));
    }
    let unpair = gtk::gio::SimpleAction::new("unpair", None);
    {
        let ui = ui.clone();
        unpair.connect_activate(move |_, _| {
            if let Some(d) = ui.ctx.device() {
                confirm_unpair(&ui, &d.id, &d.name);
            }
        });
    }
    // Also reachable from outside (org.gtk.Actions on the session bus), e.g. a notification
    // that opens Messages, or tests.
    let page = gtk::gio::SimpleAction::new("show-page", Some(glib::VariantTy::STRING));
    {
        let ui = ui.clone();
        page.connect_activate(move |_, arg| {
            if let Some(name) = arg.and_then(|a| a.str()) {
                show_page(&ui, name, false);
                ui.split.set_show_content(true);
                ui.window.present();
            }
        });
    }
    let device = gtk::gio::SimpleAction::new("select-device", Some(glib::VariantTy::STRING));
    {
        let ui = ui.clone();
        device.connect_activate(move |_, arg| {
            let Some(id) = arg.and_then(|a| a.str()) else {
                return;
            };
            let i = ui.devices.borrow().iter().position(|d| d.id == id);
            if let Some(i) = i {
                ui.switcher.set_selected(i as u32);
            }
        });
    }
    let reply = gtk::gio::SimpleAction::new("quick-reply", Some(glib::VariantTy::STRING));
    {
        let ui = ui.clone();
        reply.connect_activate(move |_, arg| {
            let Some((device, key)) = arg.and_then(|a| a.str()).and_then(|s| s.split_once('\n'))
            else {
                return;
            };
            let (ui, device, key) = (ui.clone(), device.to_owned(), key.to_owned());
            glib::spawn_future_local(async move { quick_reply(&ui, &device, &key).await });
        });
    }
    let present = gtk::gio::SimpleAction::new("present", None);
    {
        let ui = ui.clone();
        present.connect_activate(move |_, _| ui.window.present());
    }
    let send = gtk::gio::SimpleAction::new("send-files", None);
    {
        let ui = ui.clone();
        send.connect_activate(move |_, _| {
            ui.window.present();
            show_page(&ui, "files", false);
            let Some(d) = ui.ctx.device() else { return };
            let ui = ui.clone();
            glib::spawn_future_local(async move { pick_and_send(&ui, &d.id, &d.name).await });
        });
    }
    if let Some(app) = ui.window.application() {
        app.add_action(&present);
        app.add_action(&reply);
        app.add_action(&send);
        app.add_action(&page);
        app.add_action(&device);
    }
    let picture = gtk::gio::SimpleAction::new("picture", None);
    {
        let ui = ui.clone();
        picture.connect_activate(move |_, _| {
            let ui = ui.clone();
            glib::spawn_future_local(async move { choose_picture(&ui).await });
        });
    }
    ui.window.add_action(&picture);
    ui.window.add_action(&reload);
    ui.window.add_action(&pair);
    ui.window.add_action(&unpair);
}

/// Files dropped anywhere on the window go to the selected phone.
fn add_drop_target(ui: &Rc<Ui>) {
    let target = gtk::DropTarget::new(
        gtk::gdk::FileList::static_type(),
        gtk::gdk::DragAction::COPY,
    );
    ui.stack.add_controller(target.clone());
    let ui = ui.clone();
    target.connect_drop(move |_, value, _, _| {
        let (Ok(list), Some(d)) = (value.get::<gtk::gdk::FileList>(), ui.ctx.device()) else {
            return false;
        };
        let paths: Vec<String> = list
            .files()
            .iter()
            .filter_map(|f| f.path())
            .map(|p| p.to_string_lossy().into_owned())
            .collect();
        if paths.is_empty() {
            return false;
        }
        let ui = ui.clone();
        glib::spawn_future_local(async move { send_paths(&ui, &d.id, &d.name, paths).await });
        true
    });
}

/// Shows a feature page, loading it if it hasn't been loaded for this phone (or `force`).
fn show_page(ui: &Rc<Ui>, name: &str, force: bool) {
    let Some(i) = PAGES.iter().position(|p| p.0 == name) else {
        return;
    };
    ui.pages.set_visible_child_name(name);
    ui.page_title.set_title(PAGES[i].1);
    ui.refresh.set_visible(name != "files");
    ui.photos.fit.set_visible(name == "photos");
    ui.notifications
        .clear
        .set_visible(name == "notifications" && ui.notifications.badge.is_visible());
    if ui.nav.selected_row().map(|r| r.index()) != Some(i as i32)
        && let Some(row) = ui.nav.row_at_index(i as i32)
    {
        ui.nav.select_row(Some(&row));
    }
    let device = ui.ctx.device().map(|d| d.id).unwrap_or_default();
    let key = (name.to_owned(), device);
    let fresh = !ui.loaded.borrow().contains(&key);
    match name {
        "photos" => {
            ui.page_title.set_subtitle(&ui.photos.subtitle.label());
            if fresh || force {
                photos::reload(&ui.photos);
            }
        }
        "messages" => {
            ui.page_title.set_subtitle("");
            if fresh || force {
                messages::reload(&ui.messages, true);
            }
        }
        "notifications" => {
            ui.page_title.set_subtitle("");
            notifications::reload(&ui.notifications);
        }
        "storage" => {
            ui.page_title.set_subtitle("");
            if fresh || force {
                storage::reload(&ui.storage, fresh);
            }
        }
        _ => ui.page_title.set_subtitle(""),
    }
    let mut loaded = ui.loaded.borrow_mut();
    loaded.retain(|(p, _)| p != name);
    loaded.push(key);
}

fn select_device(ui: &Rc<Ui>, d: Device) {
    ui.ctx.device.replace(Some(d.clone()));
    save_last_device(&d.id);
    ui.loaded.borrow_mut().clear();
    update_hero(ui, &d);
    let page = ui
        .pages
        .visible_child_name()
        .map(|s| s.to_string())
        .unwrap_or("photos".into());
    show_page(ui, &page, false);
    notifications::reload(&ui.notifications);
}

fn update_hero(ui: &Rc<Ui>, d: &Device) {
    ui.picture.set_paintable(Some(&device_picture(d)));
    ui.ring.set_sensitive(d.connected);
    update_features(ui, d);
    ui.phone_name.set_label(&d.name);
    if d.connected {
        ui.link_icon.set_icon_name(Some("emblem-ok-symbolic"));
        ui.link_icon.set_css_classes(&["link-ok"]);
        ui.link_label.set_label("Connected");
    } else {
        ui.link_icon.set_icon_name(Some("media-record-symbolic"));
        ui.link_icon.set_css_classes(&["link-off"]);
        ui.link_label.set_label("Not connected");
    }
    let level = if d.connected { d.battery } else { -1 };
    ui.battery.set(level, d.charging);
    if d.connected && d.charging {
        let (battery, proxy, id) = (ui.battery.clone(), ui.ctx.proxy(), d.id.clone());
        glib::spawn_future_local(async move {
            let Some(p) = proxy else { return };
            if let Ok((watts, fast, full_in)) = p.battery_details(&id).await {
                battery.set_details(watts, fast, full_in);
            }
        });
    }
    ui.battery_label.set_visible(level >= 0);
    ui.battery_label.set_label(&format!("{level}%"));
    ui.battery_label.set_css_classes(if d.charging {
        &["status-text", "numeric", "charging"]
    } else if level <= 20 {
        &["status-text", "numeric", "low"]
    } else {
        &["status-text", "numeric"]
    });
    ui.files_page
        .set_title(&format!("Send Files to {}", d.name));
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
    *ui.ctx.proxy.borrow_mut() = Some(proxy.clone());
    refresh(&ui).await;

    if let Ok(mut changed) = proxy.receive_devices_changed().await {
        let ui = ui.clone();
        glib::spawn_future_local(async move {
            while changed.next().await.is_some() {
                refresh(&ui).await;
            }
        });
    }
    if let Ok(mut events) = proxy.receive_phone_event().await {
        let ui = ui.clone();
        glib::spawn_future_local(async move {
            while let Some(signal) = events.next().await {
                let Ok(args) = signal.args() else { continue };
                if ui.ctx.device().map(|d| d.id) != Some(args.id.to_string()) {
                    continue;
                }
                if args.kind == "sms.changed" {
                    let open = ui.loaded.borrow().iter().any(|(p, _)| p == "messages");
                    if open {
                        messages::reload(&ui.messages, false);
                    }
                } else {
                    notifications::reload(&ui.notifications);
                }
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
    ui.ctx.proxy()
}

async fn refresh(ui: &Rc<Ui>) {
    let Some(proxy) = proxy(ui) else { return };
    let rows = match proxy.list_devices().await {
        Ok(d) => d,
        Err(e) => {
            ui.error_page
                .set_description(Some(&glib::markup_escape_text(&e.to_string())));
            ui.stack.set_visible_child_name("error");
            return;
        }
    };
    let devices: Vec<Device> = rows
        .into_iter()
        .map(|(id, name, ty, connected, battery, charging)| Device {
            id,
            name,
            ty,
            connected,
            battery,
            charging,
        })
        .collect();
    if devices.is_empty() {
        ui.ctx.device.replace(None);
        ui.devices.borrow_mut().clear();
        ui.stack.set_visible_child_name("empty");
        return;
    }
    let names: Vec<&str> = devices.iter().map(|d| d.name.as_str()).collect();
    *ui.devices.borrow_mut() = devices.clone();
    ui.filling.set(true);
    ui.switcher.set_model(Some(&gtk::StringList::new(&names)));
    ui.switcher.set_visible(devices.len() > 1);

    let current = ui.ctx.device();
    // Keep the phone that was selected (or last time's); otherwise prefer one that's connected.
    let wanted = current.as_ref().map(|c| c.id.clone()).or_else(last_device);
    let pick = wanted
        .and_then(|id| devices.iter().position(|d| d.id == id))
        .or_else(|| devices.iter().position(|d| d.connected))
        .unwrap_or(0);
    let d = devices[pick].clone();
    ui.switcher.set_selected(pick as u32);
    ui.filling.set(false);
    ui.stack.set_visible_child_name("main");
    match current {
        Some(c) if c.id == d.id => {
            ui.ctx.device.replace(Some(d.clone()));
            update_hero(ui, &d);
            // Just reconnected: what the pages show may be out of date.
            if d.connected && !c.connected {
                ui.loaded.borrow_mut().clear();
                if let Some(name) = ui.pages.visible_child_name() {
                    show_page(ui, &name, false);
                }
            }
        }
        _ => select_device(ui, d),
    }
}

fn last_device_file() -> std::path::PathBuf {
    glib::user_config_dir()
        .join("continuum")
        .join("last-device")
}

fn last_device() -> Option<String> {
    std::fs::read_to_string(last_device_file())
        .ok()
        .map(|s| s.trim().to_owned())
}

fn save_last_device(id: &str) {
    if last_device().as_deref() == Some(id) {
        return;
    }
    let path = last_device_file();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let tmp = path.with_extension("part");
    let _ = std::fs::write(&tmp, id).and_then(|_| std::fs::rename(&tmp, &path));
}

/// A small window to answer a phone notification (a chat message) without opening the app.
async fn quick_reply(ui: &Rc<Ui>, device: &str, key: &str) {
    #[derive(serde::Deserialize)]
    struct Posted {
        key: String,
        #[serde(default)]
        app: String,
        #[serde(default)]
        title: String,
        #[serde(default)]
        text: String,
    }
    let Some(p) = ui.ctx.proxy() else { return };
    let list: Vec<Posted> = p
        .list_notifications(device)
        .await
        .ok()
        .and_then(|j| serde_json::from_str(&j).ok())
        .unwrap_or_default();
    let Some(n) = list.into_iter().find(|n| n.key == key) else {
        ui.ctx.toast("That message is no longer on your phone");
        return;
    };
    let header = adw::HeaderBar::builder()
        .title_widget(&adw::WindowTitle::new(&n.title, &n.app))
        .build();
    let message = gtk::Label::builder()
        .label(&n.text)
        .wrap(true)
        .wrap_mode(gtk::pango::WrapMode::WordChar)
        .xalign(0.0)
        .selectable(true)
        .lines(8)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .css_classes(["bubble", "in"])
        .halign(gtk::Align::Start)
        .build();
    let entry = gtk::Entry::builder()
        .placeholder_text("Reply…")
        .hexpand(true)
        .build();
    let send = gtk::Button::builder()
        .icon_name("mail-send-symbolic")
        .tooltip_text("Send")
        .css_classes(["circular", "suggested-action"])
        .sensitive(false)
        .build();
    let row = gtk::Box::builder().spacing(8).build();
    row.append(&entry);
    row.append(&send);
    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(14)
        .margin_top(6)
        .margin_bottom(16)
        .margin_start(16)
        .margin_end(16)
        .build();
    content.append(&message);
    content.append(&row);
    let view = adw::ToolbarView::new();
    view.add_top_bar(&header);
    view.set_content(Some(&content));
    let win = adw::Window::builder()
        .title(format!("Reply to {}", n.title))
        .default_width(420)
        .content(&view)
        .build();
    if let Some(app) = ui.window.application() {
        win.set_application(Some(&app));
    }
    let esc = gtk::ShortcutController::new();
    esc.add_shortcut(gtk::Shortcut::new(
        gtk::ShortcutTrigger::parse_string("Escape"),
        Some(gtk::CallbackAction::new(|w, _| {
            if let Some(w) = w.downcast_ref::<gtk::Window>() {
                w.close();
            }
            glib::Propagation::Stop
        })),
    ));
    win.add_controller(esc);
    {
        let send = send.clone();
        entry.connect_changed(move |e| send.set_sensitive(!e.text().trim().is_empty()));
    }
    let go = {
        let (win, entry, device, key) = (
            win.clone(),
            entry.clone(),
            device.to_owned(),
            key.to_owned(),
        );
        move || {
            let text = entry.text().trim().to_owned();
            if text.is_empty() {
                return;
            }
            entry.set_sensitive(false);
            let (win, entry, p, device, key) = (
                win.clone(),
                entry.clone(),
                p.clone(),
                device.clone(),
                key.clone(),
            );
            glib::spawn_future_local(async move {
                let body = serde_json::json!({ "key": key, "text": text }).to_string();
                match p.command(&device, "notif.reply", &body).await {
                    Ok(()) => win.close(),
                    Err(e) => {
                        entry.set_sensitive(true);
                        entry.set_placeholder_text(Some(&ctx::dbus_error(e).text()));
                    }
                }
            });
        }
    };
    let go2 = go.clone();
    entry.connect_activate(move |_| go());
    send.connect_clicked(move |_| go2());
    dev_snapshots(&win, "reply");
    win.present();
    entry.grab_focus();
}

/// Shows only the features the connected phone supports (all of them while it's offline, so
/// the pages can say why they're empty).
fn update_features(ui: &Rc<Ui>, d: &Device) {
    let (Some(p), id) = (ui.ctx.proxy(), d.id.clone()) else {
        return;
    };
    let ui = ui.clone();
    glib::spawn_future_local(async move {
        let caps = p.device_caps(&id).await.unwrap_or_default();
        if ui.ctx.device().map(|d| d.id) != Some(id) {
            return;
        }
        for (i, (name, _, _)) in PAGES.iter().enumerate() {
            let cap = page_cap(name);
            let shown = caps.is_empty() || cap.is_empty() || caps.iter().any(|c| c == cap);
            if let Some(row) = ui.nav.row_at_index(i as i32) {
                row.set_visible(shown);
            }
        }
        ui.ring
            .set_visible(caps.is_empty() || caps.iter().any(|c| c == "find"));
    });
}

fn ring_phone(ui: &Rc<Ui>) {
    let (Some(p), Some(d)) = (ui.ctx.proxy(), ui.ctx.device()) else {
        return;
    };
    let ui = ui.clone();
    glib::spawn_future_local(async move {
        if let Err(e) = p.command(&d.id, "find.ring", "{}").await {
            ui.ctx.toast(&ctx::dbus_error(e).text());
            return;
        }
        let toast = adw::Toast::builder()
            .title(format!("Ringing {}…", d.name))
            .button_label("Stop")
            .timeout(0)
            .build();
        let id = d.id.clone();
        toast.connect_button_clicked(move |_| {
            let (p, id) = (p.clone(), id.clone());
            glib::spawn_future_local(async move {
                let _ = p.command(&id, "find.stop", "{}").await;
            });
        });
        // The phone gives up after a minute by itself.
        let weak = toast.downgrade();
        glib::timeout_add_local_once(Duration::from_secs(60), move || {
            if let Some(t) = weak.upgrade() {
                t.dismiss();
            }
        });
        ui.toasts.add_toast(toast);
    });
}

/// A picture the user chose for a device (`~/.local/share/continuum/pictures/<id>.png`).
pub fn custom_picture_path(id: &str) -> std::path::PathBuf {
    let id: String = id.chars().filter(char::is_ascii_hexdigit).collect();
    glib::user_data_dir()
        .join("continuum")
        .join("pictures")
        .join(format!("{id}.png"))
}

fn device_picture(d: &Device) -> gtk::gdk::Texture {
    gtk::gdk::Texture::from_filename(custom_picture_path(&d.id))
        .unwrap_or_else(|_| device_texture(&d.ty))
}

async fn choose_picture(ui: &Rc<Ui>) {
    let Some(d) = ui.ctx.device() else { return };
    let filter = gtk::FileFilter::new();
    filter.add_pixbuf_formats();
    filter.set_name(Some("Images"));
    let filters = gtk::gio::ListStore::new::<gtk::FileFilter>();
    filters.append(&filter);
    let dialog = gtk::FileDialog::builder()
        .title(format!("Picture for {}", d.name))
        .filters(&filters)
        .build();
    let Ok(file) = dialog.open_future(Some(&ui.window)).await else {
        return;
    };
    let Some(path) = file.path() else { return };
    let Ok(texture) = gtk::gdk::Texture::from_filename(&path) else {
        ui.ctx.toast("That file isn't a picture");
        return;
    };
    let target = custom_picture_path(&d.id);
    let tmp = target.with_extension("part");
    let saved = target
        .parent()
        .map(std::fs::create_dir_all)
        .unwrap_or(Ok(()))
        .is_ok()
        && texture.save_to_png(&tmp).is_ok()
        && std::fs::rename(&tmp, &target).is_ok();
    if saved {
        update_hero(ui, &d);
    } else {
        ui.ctx.toast("Couldn't save the picture");
    }
}

fn device_texture(ty: &str) -> gtk::gdk::Texture {
    let png: &'static [u8] = match ty {
        "tablet" => include_bytes!("../data/device-tablet.png"),
        "laptop" => include_bytes!("../data/device-laptop.png"),
        "desktop" => include_bytes!("../data/device-desktop.png"),
        _ => include_bytes!("../data/device-phone.png"),
    };
    gtk::gdk::Texture::from_bytes(&glib::Bytes::from_static(png)).expect("bundled PNG")
}

async fn pick_and_send(ui: &Rc<Ui>, id: &str, name: &str) {
    let dialog = gtk::FileDialog::builder()
        .title(format!("Send to {name}"))
        .accept_label("Send")
        .build();
    let Ok(files) = dialog.open_multiple_future(Some(&ui.window)).await else {
        return; // cancelled
    };
    let paths: Vec<String> = (0..files.n_items())
        .filter_map(|i| files.item(i)?.downcast::<gtk::gio::File>().ok()?.path())
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    if !paths.is_empty() {
        send_paths(ui, id, name, paths).await;
    }
}

async fn send_paths(ui: &Rc<Ui>, id: &str, name: &str, paths: Vec<String>) {
    let refs: Vec<&str> = paths.iter().map(String::as_str).collect();
    let Some(proxy) = proxy(ui) else { return };
    let text = match proxy.send_files(id, &refs).await {
        Ok(_) => format!("Sending to {name}…"),
        Err(zbus::Error::MethodError(_, Some(msg), _)) => format!("Couldn't send: {msg}"),
        Err(e) => format!("Couldn't send: {e}"),
    };
    ui.toasts.add_toast(adw::Toast::new(&text));
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
