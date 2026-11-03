//! Notifications page: what is in the phone's notification shade, with dismiss, quick reply
//! and the notification's own buttons.

use std::rc::Rc;

use adw::prelude::*;
use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use serde::Deserialize;

use crate::ctx::{self, Ctx};

#[derive(Debug, Clone, Deserialize)]
struct Notification {
    key: String,
    #[serde(default)]
    app: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    text: String,
    #[serde(default)]
    time: i64,
    #[serde(default)]
    icon: String,
    #[serde(default)]
    reply: bool,
    #[serde(default)]
    actions: Vec<String>,
}

pub struct Notifications {
    ctx: Rc<Ctx>,
    pub widget: gtk::Stack,
    list: gtk::Box,
    /// Number shown next to "Notifications" in the sidebar.
    pub badge: gtk::Label,
    pub clear: gtk::Button,
}

pub fn build(ctx: &Rc<Ctx>) -> Rc<Notifications> {
    let widget = gtk::Stack::builder()
        .transition_type(gtk::StackTransitionType::Crossfade)
        .build();
    let list = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(10)
        .margin_top(18)
        .margin_bottom(18)
        .margin_start(18)
        .margin_end(18)
        .build();
    let clamp = adw::Clamp::builder().maximum_size(720).child(&list).build();
    widget.add_named(
        &gtk::ScrolledWindow::builder()
            .child(&clamp)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vexpand(true)
            .build(),
        Some("list"),
    );
    widget.add_named(
        &ctx::status_page(
            "preferences-system-notifications-symbolic",
            "No Notifications",
            "New notifications on your phone show up here and on this computer.\n\nNothing showing up? Open Continuum on your phone and turn on Notifications under Features.",
        ),
        Some("empty"),
    );
    let clear = gtk::Button::builder()
        .label("Clear All")
        .visible(false)
        .build();
    let page = Rc::new(Notifications {
        ctx: ctx.clone(),
        widget,
        list,
        badge: gtk::Label::builder()
            .css_classes(["badge"])
            .visible(false)
            .build(),
        clear: clear.clone(),
    });
    let weak = Rc::downgrade(&page);
    clear.connect_clicked(move |_| {
        let Some(page) = weak.upgrade() else { return };
        glib::spawn_future_local(async move {
            for key in keys(&page).await {
                command(
                    &page.ctx,
                    "notif.dismiss",
                    serde_json::json!({ "key": key }),
                )
                .await;
            }
        });
    });
    page
}

async fn keys(page: &Notifications) -> Vec<String> {
    load(page).await.into_iter().map(|n| n.key).collect()
}

async fn load(page: &Notifications) -> Vec<Notification> {
    let (Some(p), Some(d)) = (page.ctx.proxy(), page.ctx.device()) else {
        return vec![];
    };
    p.list_notifications(&d.id)
        .await
        .ok()
        .and_then(|j| serde_json::from_str(&j).ok())
        .unwrap_or_default()
}

async fn command(ctx: &Ctx, kind: &str, body: serde_json::Value) {
    let (Some(p), Some(d)) = (ctx.proxy(), ctx.device()) else {
        return;
    };
    if let Err(e) = p.command(&d.id, kind, &body.to_string()).await {
        ctx.toast(&ctx::dbus_error(e).text());
    }
}

/// Rebuilds the list from the daemon (on open, and on every notif.* event).
pub fn reload(page: &Rc<Notifications>) {
    let page = page.clone();
    glib::spawn_future_local(async move {
        let mut all = load(&page).await;
        all.sort_by_key(|n| std::cmp::Reverse(n.time));
        ctx::clear_box(&page.list);
        for n in &all {
            page.list.append(&card(&page, n));
        }
        page.badge.set_label(&all.len().to_string());
        page.badge.set_visible(!all.is_empty());
        let showing = page
            .widget
            .parent()
            .and_downcast::<gtk::Stack>()
            .and_then(|s| s.visible_child_name());
        page.clear
            .set_visible(!all.is_empty() && showing.as_deref() == Some("notifications"));
        page.widget
            .set_visible_child_name(if all.is_empty() { "empty" } else { "list" });
    });
}

fn app_icon(n: &Notification) -> gtk::Widget {
    let texture = B64
        .decode(&n.icon)
        .ok()
        .and_then(|png| gtk::gdk::Texture::from_bytes(&glib::Bytes::from(&png)).ok());
    match texture {
        Some(t) => {
            let img = gtk::Image::from_paintable(Some(&t));
            img.set_pixel_size(28);
            img.upcast()
        }
        None => adw::Avatar::new(28, Some(&n.app), true).upcast(),
    }
}

fn card(page: &Rc<Notifications>, n: &Notification) -> gtk::Widget {
    let app = gtk::Label::builder()
        .label(format!("{} · {}", n.app, ctx::short_time(n.time)))
        .xalign(0.0)
        .hexpand(true)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .css_classes(["dim-label", "caption"])
        .build();
    let dismiss = gtk::Button::builder()
        .icon_name("window-close-symbolic")
        .tooltip_text("Dismiss")
        .valign(gtk::Align::Center)
        .css_classes(["flat", "circular"])
        .build();
    let top = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    top.append(&app_icon(n));
    top.append(&app);
    top.append(&dismiss);
    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(4)
        .margin_top(12)
        .margin_bottom(12)
        .margin_start(14)
        .margin_end(10)
        .build();
    content.append(&top);
    if !n.title.is_empty() {
        content.append(
            &gtk::Label::builder()
                .label(&n.title)
                .xalign(0.0)
                .wrap(true)
                .css_classes(["heading"])
                .margin_start(38)
                .build(),
        );
    }
    if !n.text.is_empty() {
        content.append(
            &gtk::Label::builder()
                .label(&n.text)
                .xalign(0.0)
                .wrap(true)
                .wrap_mode(gtk::pango::WrapMode::WordChar)
                .selectable(true)
                .lines(6)
                .ellipsize(gtk::pango::EllipsizeMode::End)
                .margin_start(38)
                .build(),
        );
    }

    let ctx = page.ctx.clone();
    let key = n.key.clone();
    {
        let (ctx, key) = (ctx.clone(), key.clone());
        dismiss.connect_clicked(move |_| {
            let (ctx, key) = (ctx.clone(), key.clone());
            glib::spawn_future_local(async move {
                command(&ctx, "notif.dismiss", serde_json::json!({ "key": key })).await;
            });
        });
    }
    if !n.actions.is_empty() || n.reply {
        let buttons = gtk::Box::builder()
            .spacing(6)
            .margin_start(38)
            .margin_top(6)
            .build();
        for (i, title) in n.actions.iter().enumerate() {
            let b = gtk::Button::builder()
                .label(title)
                .css_classes(["pill", "small"])
                .build();
            let (ctx, key) = (ctx.clone(), key.clone());
            b.connect_clicked(move |_| {
                let (ctx, key) = (ctx.clone(), key.clone());
                glib::spawn_future_local(async move {
                    let body = serde_json::json!({ "key": key, "index": i });
                    command(&ctx, "notif.action", body).await;
                });
            });
            buttons.append(&b);
        }
        content.append(&buttons);
        if n.reply {
            let entry = gtk::Entry::builder()
                .placeholder_text("Reply…")
                .hexpand(true)
                .secondary_icon_name("mail-send-symbolic")
                .build();
            let send = {
                let (ctx, key) = (ctx.clone(), key.clone());
                move |e: &gtk::Entry| {
                    let text = e.text().trim().to_owned();
                    if text.is_empty() {
                        return;
                    }
                    e.set_text("");
                    let (ctx, key) = (ctx.clone(), key.clone());
                    glib::spawn_future_local(async move {
                        let body = serde_json::json!({ "key": key, "text": text });
                        command(&ctx, "notif.reply", body).await;
                        ctx.toast("Reply sent");
                    });
                }
            };
            let send2 = send.clone();
            entry.connect_activate(move |e| send(e));
            entry.connect_icon_press(move |e, _| send2(e));
            buttons.append(&entry);
        }
    }
    let frame = gtk::Box::builder()
        .css_classes(["card", "notif-card"])
        .build();
    frame.append(&content);
    content.set_hexpand(true);
    frame.upcast()
}
