//! Desktop notifications for transfers (freedesktop Notifications over D-Bus), with
//! "Open" and "Show in Files" actions.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use futures_util::StreamExt;
use zbus::Variant as Value;
use zbus::{Connection, proxy};

use crate::state::Daemon;
use crate::transfer::Progress;
use continuum_proto::message::PhoneNotification;

const APP_ID: &str = "io.github.sharjeelmazhar.Continuum";

#[proxy(
    interface = "org.freedesktop.Notifications",
    default_service = "org.freedesktop.Notifications",
    default_path = "/org/freedesktop/Notifications"
)]
trait Notifications {
    #[allow(clippy::too_many_arguments)]
    fn notify(
        &self,
        app_name: &str,
        replaces_id: u32,
        app_icon: &str,
        summary: &str,
        body: &str,
        actions: &[&str],
        hints: HashMap<&str, Value<'_>>,
        expire_timeout: i32,
    ) -> zbus::Result<u32>;

    fn close_notification(&self, id: u32) -> zbus::Result<()>;

    #[zbus(signal)]
    fn action_invoked(&self, id: u32, action_key: &str) -> zbus::Result<()>;

    /// Sent just before `ActionInvoked`, so the window we open can take the focus.
    #[zbus(signal)]
    fn activation_token(&self, id: u32, activation_token: &str) -> zbus::Result<()>;
}

#[proxy(
    interface = "org.freedesktop.FileManager1",
    default_service = "org.freedesktop.FileManager1",
    default_path = "/org/freedesktop/FileManager1"
)]
trait FileManager1 {
    fn show_items(&self, uris: &[&str], startup_id: &str) -> zbus::Result<()>;
}

pub struct Notifier {
    conn: Connection,
    proxy: NotificationsProxy<'static>,
    /// Notification id → files it is about, for its action buttons.
    files: Mutex<HashMap<u32, Vec<PathBuf>>>,
    /// Device name → its "Receiving…" notification, replaced when the transfer ends.
    in_progress: Mutex<HashMap<String, u32>>,
    /// "device/key" of a phone notification → our popup for it.
    phone: Mutex<HashMap<String, u32>>,
    /// Popup → (device id, phone notification key), for its Reply and action buttons.
    popups: Mutex<HashMap<u32, (String, String)>>,
    tokens: Mutex<HashMap<u32, String>>,
    /// Live progress notifications we opened for transfers (closed when they end), and the
    /// bytes done at the last update, for the speed.
    live: Mutex<HashMap<String, (u32, u64)>>,
    daemon: Arc<Daemon>,
}

fn locked<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl Notifier {
    pub async fn new(conn: &Connection, daemon: Arc<Daemon>) -> zbus::Result<Arc<Self>> {
        let proxy = NotificationsProxy::new(conn).await?;
        let me = Arc::new(Self {
            conn: conn.clone(),
            proxy: proxy.clone(),
            files: Mutex::new(HashMap::new()),
            in_progress: Mutex::new(HashMap::new()),
            phone: Mutex::new(HashMap::new()),
            popups: Mutex::new(HashMap::new()),
            tokens: Mutex::new(HashMap::new()),
            live: Mutex::new(HashMap::new()),
            daemon,
        });
        let weak = Arc::downgrade(&me);
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(PROGRESS_EVERY);
            loop {
                tick.tick().await;
                let Some(me) = weak.upgrade() else { break };
                me.update_progress().await;
            }
        });
        let mut tokens = proxy.receive_activation_token().await?;
        let weak = Arc::downgrade(&me);
        tokio::spawn(async move {
            while let Some(signal) = tokens.next().await {
                if let (Ok(args), Some(me)) = (signal.args(), weak.upgrade()) {
                    let mut g = locked(&me.tokens);
                    if g.len() > 50 {
                        g.clear();
                    }
                    g.insert(args.id, args.activation_token.to_owned());
                }
            }
        });
        let mut actions = proxy.receive_action_invoked().await?;
        let weak = Arc::downgrade(&me);
        tokio::spawn(async move {
            while let Some(signal) = actions.next().await {
                let (Ok(args), Some(me)) = (signal.args(), weak.upgrade()) else {
                    continue;
                };
                let files = me
                    .files
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .get(&args.id)
                    .cloned();
                if let Some(files) = files {
                    me.act(args.action_key, &files).await;
                } else {
                    me.phone_action(args.id, args.action_key).await;
                }
            }
        });
        Ok(me)
    }

    async fn act(&self, action: &str, files: &[PathBuf]) {
        match (action, files) {
            ("show", _) => {
                let uris: Vec<String> = files.iter().map(|p| file_uri(p)).collect();
                let uris: Vec<&str> = uris.iter().map(String::as_str).collect();
                if let Ok(fm) = FileManager1Proxy::new(&self.conn).await
                    && fm.show_items(&uris, "").await.is_ok()
                {
                    return;
                }
                if let Some(dir) = files.first().and_then(|p| p.parent()) {
                    open(dir);
                }
            }
            (_, [one]) => open(one),
            (_, many) => {
                if let Some(dir) = many.first().and_then(|p| p.parent()) {
                    open(dir);
                }
            }
        }
    }

    async fn show(
        &self,
        replaces: u32,
        summary: &str,
        body: &str,
        actions: &[&str],
    ) -> Option<u32> {
        let hints = HashMap::from([("desktop-entry", Value::from(APP_ID))]);
        self.proxy
            .notify(
                "Continuum",
                replaces,
                APP_ID,
                summary,
                body,
                actions,
                hints,
                -1,
            )
            .await
            .ok()
    }

    /// Keeps one quiet notification per running transfer (in the notification list, no popup)
    /// up to date: "45% · 60 MB of 188 MB · 8.1 MB/s · 15 s left".
    async fn update_progress(&self) {
        let rows = crate::activity::ACTIVITY.list();
        if rows.is_empty() && locked(&self.live).is_empty() {
            return;
        }
        let names: HashMap<String, String> = self
            .daemon
            .list()
            .into_iter()
            .map(|s| (s.device.id, s.device.name))
            .collect();
        let mut seen = Vec::new();
        for r in rows {
            let device = names.get(&r.device).cloned().unwrap_or_default();
            // Shares already have a "Receiving…"/"Sending…" notification: update that one.
            let share_key = if r.up {
                format!("send:{device}")
            } else {
                device.clone()
            };
            let key = format!("{}:{}:{}", r.up, r.device, r.name);
            seen.push(key.clone());
            let (mine, last) = locked(&self.live).get(&key).copied().unwrap_or((0, r.done));
            let theirs = locked(&self.in_progress).get(&share_key).copied();
            let rate = (r.done.saturating_sub(last)) as f64 / PROGRESS_EVERY.as_secs_f64();
            let summary = if r.up {
                format!("Sending {} to {device}", r.name)
            } else {
                format!("Receiving {} from {device}", r.name)
            };
            let body = progress_text(r.done, r.total, rate);
            let id = self
                .show_quiet(theirs.unwrap_or(mine), &summary, &body)
                .await;
            match (theirs, id) {
                (Some(_), _) => {}
                (None, Some(id)) => {
                    locked(&self.live).insert(key, (id, r.done));
                }
                (None, None) => {}
            }
        }
        let gone: Vec<u32> = {
            let mut live = locked(&self.live);
            let ids = live
                .iter()
                .filter(|(k, _)| !seen.contains(k))
                .map(|(_, (id, _))| *id)
                .collect();
            live.retain(|k, _| seen.contains(k));
            ids
        };
        for id in gone {
            let _ = self.proxy.close_notification(id).await;
        }
    }

    /// Low urgency: listed in the notification menu, no popup, and no sound on updates.
    async fn show_quiet(&self, replaces: u32, summary: &str, body: &str) -> Option<u32> {
        let hints = HashMap::from([
            ("desktop-entry", Value::from(APP_ID)),
            ("urgency", Value::from(0u8)),
            ("suppress-sound", Value::from(true)),
        ]);
        self.proxy
            .notify("Continuum", replaces, APP_ID, summary, body, &[], hints, 0)
            .await
            .ok()
    }

    fn take_in_progress(&self, key: &str) -> u32 {
        self.in_progress
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(key)
            .unwrap_or(0)
    }

    /// A button on a phone notification's popup: Reply, one of the phone's own buttons, or
    /// the popup itself (opens the Notifications page).
    async fn phone_action(&self, id: u32, action: &str) {
        let Some((device, key)) = locked(&self.popups).get(&id).cloned() else {
            return;
        };
        let token = locked(&self.tokens).remove(&id);
        if action == "reply" {
            self.activate_app(
                "quick-reply",
                Some(&format!("{device}\n{key}")),
                token,
                true,
            )
            .await;
        } else if let Some(i) = action
            .strip_prefix("act")
            .and_then(|i| i.parse::<u32>().ok())
        {
            let body = serde_json::json!({ "key": key, "index": i });
            if let Err(e) = self.daemon.command(&device, "notif.action", body).await {
                tracing::info!("notification action failed: {e}");
            }
        } else {
            self.activate_app("select-device", Some(&device), token.clone(), false)
                .await;
            self.activate_app("show-page", Some("notifications"), token, false)
                .await;
        }
    }

    /// Runs an action of the Continuum app (org.gtk.Actions), starting the app if needed;
    /// with `background`, a freshly started app shows no main window.
    async fn activate_app(
        &self,
        action: &str,
        param: Option<&str>,
        token: Option<String>,
        background: bool,
    ) {
        let params: Vec<Value<'_>> = param.map(Value::from).into_iter().collect();
        let mut platform = HashMap::<&str, Value<'_>>::new();
        if let Some(t) = &token {
            platform.insert("activation-token", Value::from(t.as_str()));
        }
        for attempt in 0..40 {
            let call = self
                .conn
                .call_method(
                    Some(APP_ID),
                    "/io/github/sharjeelmazhar/Continuum",
                    Some("org.gtk.Actions"),
                    "Activate",
                    &(action, &params, &platform),
                )
                .await;
            if call.is_ok() {
                return;
            }
            if attempt == 0 {
                let ui = std::env::current_exe()
                    .ok()
                    .and_then(|p| p.parent().map(|d| d.join("continuum-ui")))
                    .filter(|p| p.exists())
                    .map(PathBuf::into_os_string)
                    .unwrap_or_else(|| "continuum-ui".into());
                let mut cmd = std::process::Command::new(ui);
                if background {
                    cmd.env("CONTINUUM_BACKGROUND", "1");
                }
                if let Some(t) = &token {
                    cmd.env("XDG_ACTIVATION_TOKEN", t);
                }
                if cmd.spawn().is_err() {
                    return;
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        }
    }

    /// Closes our popup for a phone notification that went away on the phone.
    pub async fn on_phone_notification_removed(&self, device: &str, key: &str) {
        let id = self
            .phone
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&format!("{device}/{key}"));
        if let Some(id) = id {
            locked(&self.popups).remove(&id);
            let _ = self.proxy.close_notification(id).await;
        }
    }

    /// Pops up a notification that arrived on a phone (not the ones it lists after connecting,
    /// nor unchanged reposts), replacing our earlier popup for the same one.
    pub async fn on_phone_notification(
        &self,
        device_id: &str,
        device: &str,
        n: &PhoneNotification,
    ) {
        if n.silent {
            return;
        }
        let slot = format!("{device_id}/{}", n.key);
        let replaces = self
            .phone
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&slot)
            .copied()
            .unwrap_or(0);
        let icon = app_icon(n).unwrap_or_else(|| APP_ID.to_owned());
        let summary = match (n.app.as_str(), n.title.as_str()) {
            (app, "") => app.to_owned(),
            (_, title) => title.to_owned(),
        };
        let body = format!(
            "{} · {}\n{}",
            glib_escape(&n.app),
            glib_escape(device),
            glib_escape(&n.text)
        );
        let hints = HashMap::from([("desktop-entry", Value::from(APP_ID))]);
        let mut actions = vec!["default".to_owned(), "Open".to_owned()];
        if n.reply {
            actions.extend(["reply".to_owned(), "Reply".to_owned()]);
        }
        for (i, title) in n.actions.iter().enumerate() {
            actions.extend([format!("act{i}"), title.clone()]);
        }
        let actions: Vec<&str> = actions.iter().map(String::as_str).collect();
        if let Ok(id) = self
            .proxy
            .notify(
                "Continuum",
                replaces,
                &icon,
                &summary,
                &body,
                &actions,
                hints,
                -1,
            )
            .await
        {
            {
                let mut p = locked(&self.popups);
                if p.len() > 500 {
                    p.clear();
                }
                p.insert(id, (device_id.to_owned(), n.key.clone()));
            }
            let mut g = locked(&self.phone);
            if g.len() > 500 {
                g.clear();
            }
            g.insert(slot, id);
        }
    }

    pub async fn on_transfer(&self, p: &Progress) {
        match p {
            Progress::Started {
                device,
                files,
                bytes,
            } => {
                let summary = if *files == 1 {
                    format!("Receiving a file from {device}")
                } else {
                    format!("Receiving {files} files from {device}")
                };
                if let Some(id) = self.show(0, &summary, &human_size(*bytes), &[]).await {
                    self.in_progress
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .insert(device.clone(), id);
                }
            }
            Progress::Finished { device, saved } => {
                let replaces = self
                    .in_progress
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .remove(device)
                    .unwrap_or(0);
                let summary = match saved.as_slice() {
                    [one] => format!(
                        "Received {}",
                        one.file_name()
                            .map(|n| n.to_string_lossy())
                            .unwrap_or_default()
                    ),
                    many => format!("Received {} files", many.len()),
                };
                let folder = saved
                    .first()
                    .and_then(|p| p.parent())
                    .map(|d| d.display().to_string())
                    .unwrap_or_default();
                // Bodies are markup: names from the phone are escaped.
                let body = format!(
                    "From {} · saved in {}",
                    glib_escape(device),
                    glib_escape(&folder)
                );
                let actions = ["default", "Open", "open", "Open", "show", "Show in Files"];
                if let Some(id) = self.show(replaces, &summary, &body, &actions).await {
                    self.files
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .insert(id, saved.clone());
                }
            }
            Progress::SendQueued { device } => {
                self.show(
                    0,
                    &format!("Will send to {device} when it connects"),
                    "Files go out automatically as soon as the phone is back on this network.",
                    &[],
                )
                .await;
            }
            Progress::Sending { device, files } => {
                let summary = if *files == 1 {
                    format!("Sending a file to {device}")
                } else {
                    format!("Sending {files} files to {device}")
                };
                let replaces = self.take_in_progress(&format!("send:{device}"));
                if let Some(id) = self.show(replaces, &summary, "", &[]).await {
                    self.in_progress
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .insert(format!("send:{device}"), id);
                }
            }
            Progress::Sent { device, names } => {
                let replaces = self.take_in_progress(&format!("send:{device}"));
                let summary = match names.as_slice() {
                    [one] => format!("Sent {one}"),
                    many => format!("Sent {} files", many.len()),
                };
                self.show(
                    replaces,
                    &summary,
                    &format!("To {} · in Downloads/Continuum", glib_escape(device)),
                    &[],
                )
                .await;
            }
            Progress::SendFailed { device, reason } => {
                let replaces = self.take_in_progress(&format!("send:{device}"));
                self.show(
                    replaces,
                    &format!("Couldn't send files to {device}"),
                    &glib_escape(reason),
                    &[],
                )
                .await;
            }
            Progress::Failed { device, reason } => {
                let replaces = self
                    .in_progress
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .remove(device)
                    .unwrap_or(0);
                self.show(
                    replaces,
                    &format!("Couldn't receive files from {device}"),
                    &glib_escape(reason),
                    &[],
                )
                .await;
            }
        }
    }
}

/// Where the icon of the phone app `pkg` is cached.
pub fn icon_path(pkg: &str) -> Option<std::path::PathBuf> {
    let name: String = pkg
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '.' || *c == '_')
        .collect();
    let name = name.trim_start_matches('.');
    if name.is_empty() {
        return None;
    }
    Some(
        crate::sysinfo::cache_dir()
            .join("icons")
            .join(format!("{name}.png")),
    )
}

/// Saves the app icon the phone sent (once per app) and returns its path. Newer phones send
/// each app's icon once per connection: then the saved one is used.
fn app_icon(n: &PhoneNotification) -> Option<String> {
    use base64::Engine;
    let path = icon_path(&n.pkg)?;
    if n.icon.is_empty() {
        return path.exists().then(|| path.to_string_lossy().into_owned());
    }
    if !path.exists() {
        let png = base64::engine::general_purpose::STANDARD
            .decode(&n.icon)
            .ok()?;
        if !png.starts_with(b"\x89PNG") {
            return None;
        }
        std::fs::create_dir_all(path.parent()?).ok()?;
        crate::store::write_atomic(&path, &png).ok()?;
    }
    Some(path.to_string_lossy().into_owned())
}

/// Notification bodies may contain markup; phone text is shown as plain text.
fn glib_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn open(path: &Path) {
    let opened = std::process::Command::new("gio")
        .arg("open")
        .arg(path)
        .spawn()
        .is_ok();
    if !opened {
        let _ = std::process::Command::new("xdg-open").arg(path).spawn();
    }
}

/// `file://` URI with everything but unreserved characters and `/` percent-encoded.
pub(crate) fn file_uri(path: &Path) -> String {
    let mut out = String::from("file://");
    for b in path.to_string_lossy().bytes() {
        if b.is_ascii_alphanumeric() || b"-._~/".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

const PROGRESS_EVERY: std::time::Duration = std::time::Duration::from_secs(1);

/// "45% · 60 MB of 188 MB · 8.1 MB/s · 15 s left" (speed and time once it's moving).
fn progress_text(done: u64, total: u64, rate: f64) -> String {
    let pct = (done * 100).checked_div(total).unwrap_or(0).min(100);
    let mut s = format!("{pct}% · {} of {}", human_size(done), human_size(total));
    if rate >= 1.0 {
        let left = total.saturating_sub(done) as f64 / rate;
        s.push_str(&format!(
            " · {}/s · {} left",
            human_size(rate as u64),
            time_left(left as u64)
        ));
    }
    s
}

fn time_left(secs: u64) -> String {
    match secs {
        0..=59 => format!("{secs} s"),
        60..=3599 => format!("{} min {} s", secs / 60, secs % 60),
        _ => format!("{} h {} min", secs / 3600, secs / 60 % 60),
    }
}

fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["KB", "MB", "GB", "TB"];
    if bytes < 1000 {
        return format!("{bytes} bytes");
    }
    let mut v = bytes as f64;
    let mut unit = "";
    for u in UNITS {
        v /= 1000.0;
        unit = u;
        if v < 1000.0 {
            break;
        }
    }
    format!("{v:.1} {unit}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uris_and_sizes() {
        assert_eq!(
            file_uri(Path::new("/home/u/My File#1.jpg")),
            "file:///home/u/My%20File%231.jpg"
        );
        assert_eq!(human_size(512), "512 bytes");
        assert_eq!(human_size(1_500_000), "1.5 MB");
        assert_eq!(human_size(4_200_000_000), "4.2 GB");
    }
}
