//! What every page shares: the window, the daemon connection and the selected phone, plus
//! small helpers (requests with friendly errors, dates, status pages).

use std::cell::RefCell;
use std::path::PathBuf;

use adw::prelude::*;
use serde_json::Value;

use crate::daemon::DaemonProxy;

/// One paired device, as `ListDevices` reports it.
#[derive(Debug, Clone, PartialEq)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub ty: String,
    pub connected: bool,
    pub battery: i32,
    pub charging: bool,
}

/// Why a request to the phone failed, in the terms the UI shows.
#[derive(Debug, Clone, PartialEq)]
pub enum ReqError {
    /// Access has to be allowed in the Continuum app on the phone.
    Permission,
    Offline,
    /// The phone's Continuum is too old for this feature.
    Update,
    Other(String),
}

impl ReqError {
    pub fn text(&self) -> String {
        match self {
            Self::Permission => "Allow access in Continuum on your phone".into(),
            Self::Offline => "Your phone isn't connected".into(),
            Self::Update => "Update Continuum on your phone to use this".into(),
            Self::Other(e) => e.clone(),
        }
    }
}

pub struct Ctx {
    pub window: adw::ApplicationWindow,
    pub toasts: adw::ToastOverlay,
    pub proxy: RefCell<Option<DaemonProxy<'static>>>,
    pub device: RefCell<Option<Device>>,
}

impl Ctx {
    pub fn proxy(&self) -> Option<DaemonProxy<'static>> {
        self.proxy.borrow().clone()
    }

    pub fn device(&self) -> Option<Device> {
        self.device.borrow().clone()
    }

    pub fn toast(&self, text: &str) {
        self.toasts.add_toast(crate::ctx::plain_toast(text));
    }

    /// A toast that stays until dismissed: dismiss it as soon as the work is done, so the
    /// result shows at once instead of queueing behind it.
    pub fn busy_toast(&self, text: &str) -> adw::Toast {
        let t = adw::Toast::builder()
            .use_markup(false)
            .title(text)
            .timeout(0)
            .build();
        self.toasts.add_toast(t.clone());
        t
    }

    /// Sends a request (see "Requests" in docs/PROTOCOL.md) to the selected phone.
    pub async fn request(&self, kind: &str, body: Value) -> Result<Value, ReqError> {
        let (Some(p), Some(d)) = (self.proxy(), self.device()) else {
            return Err(ReqError::Offline);
        };
        if !d.connected {
            return Err(ReqError::Offline);
        }
        let reply = p
            .request(&d.id, kind, &body.to_string())
            .await
            .map_err(dbus_error)?;
        serde_json::from_str(&reply).map_err(|e| ReqError::Other(e.to_string()))
    }

    pub async fn fetch_photo(&self, photo: i64) -> Result<PathBuf, ReqError> {
        let (Some(p), Some(d)) = (self.proxy(), self.device()) else {
            return Err(ReqError::Offline);
        };
        p.fetch_photo(&d.id, photo)
            .await
            .map(PathBuf::from)
            .map_err(dbus_error)
    }
}

pub fn dbus_error(e: zbus::Error) -> ReqError {
    let msg = match e {
        zbus::Error::MethodError(_, Some(msg), _) => msg,
        zbus::Error::FDO(e) => match *e {
            zbus::fdo::Error::Failed(msg) => msg,
            e => e.to_string(),
        },
        e => e.to_string(),
    };
    if msg == "permission" {
        ReqError::Permission
    } else if msg.starts_with("update Continuum") {
        ReqError::Update
    } else if msg.contains("isn't connected") || msg.contains("disconnected") {
        ReqError::Offline
    } else {
        ReqError::Other(msg)
    }
}

/// "3:41 PM" today, "Yesterday", "Monday" this week, else "3 Nov 2026".
pub fn short_time(ms: i64) -> String {
    let Ok(t) = glib::DateTime::from_unix_local(ms / 1000) else {
        return String::new();
    };
    let Ok(now) = glib::DateTime::now_local() else {
        return String::new();
    };
    let days = now.difference(&t).as_seconds() / 86_400;
    let fmt = if t.ymd() == now.ymd() {
        "%-l:%M %p"
    } else if days < 2 && now.add_days(-1).is_ok_and(|y| y.ymd() == t.ymd()) {
        return "Yesterday".into();
    } else if days < 7 {
        "%A"
    } else {
        "%-d %b %Y"
    };
    t.format(fmt).map(|s| s.to_string()).unwrap_or_default()
}

/// "3:41 PM" or "3 Nov, 3:41 PM", for message bubbles and notifications.
pub fn long_time(ms: i64) -> String {
    let (Ok(t), Ok(now)) = (
        glib::DateTime::from_unix_local(ms / 1000),
        glib::DateTime::now_local(),
    ) else {
        return String::new();
    };
    let fmt = if t.ymd() == now.ymd() {
        "%-l:%M %p"
    } else {
        "%-d %b, %-l:%M %p"
    };
    t.format(fmt).map(|s| s.to_string()).unwrap_or_default()
}

/// A full-page message with an optional button, for empty, offline and permission states.
pub fn status_page(icon: &str, title: &str, text: &str) -> adw::StatusPage {
    adw::StatusPage::builder()
        .icon_name(icon)
        .title(title)
        .description(text)
        .vexpand(true)
        .build()
}

/// The status page for a failed request.
pub fn error_page(e: &ReqError, what: &str) -> adw::StatusPage {
    match e {
        ReqError::Permission => status_page(
            "dialog-password-symbolic",
            &format!("Allow {what} on Your Phone"),
            &format!(
                "Open Continuum on your phone and turn on {what} under Features. Then try again."
            ),
        ),
        ReqError::Offline => status_page(
            "network-offline-symbolic",
            "Phone Not Connected",
            "Make sure Continuum is open on your phone and both devices are on the same Wi-Fi.",
        ),
        ReqError::Update => status_page(
            "software-update-available-symbolic",
            "Update Continuum on Your Phone",
            &format!(
                "The Continuum app on your phone is too old to show {what}. Open it and install the update."
            ),
        ),
        // The phone's own words: shown as text, not markup.
        ReqError::Other(msg) => status_page(
            "dialog-warning-symbolic",
            "Something Went Wrong",
            &glib::markup_escape_text(msg),
        ),
    }
}

/// Cache folder for a device (`~/.cache/continuum/<what>/<device id>`).
pub fn cache_dir(what: &str, device: &str) -> PathBuf {
    let id: String = device.chars().filter(char::is_ascii_hexdigit).collect();
    glib::user_cache_dir().join("continuum").join(what).join(id)
}

/// The first icon of `names` the theme has (Yaru and Adwaita differ).
pub fn icon(names: &[&'static str]) -> &'static str {
    let theme = gtk::gdk::Display::default().map(|d| gtk::IconTheme::for_display(&d));
    names
        .iter()
        .find(|n| theme.as_ref().is_some_and(|t| t.has_icon(n)))
        .copied()
        .unwrap_or(names[names.len() - 1])
}

pub fn clear_box(b: &gtk::Box) {
    while let Some(c) = b.first_child() {
        b.remove(&c);
    }
}

/// A toast that shows `text` as it is: names from the phone may contain `<` or `&`.
pub fn plain_toast(text: &str) -> adw::Toast {
    adw::Toast::builder().use_markup(false).title(text).build()
}
