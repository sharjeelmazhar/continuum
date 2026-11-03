//! Desktop notifications for transfers (freedesktop Notifications over D-Bus), with
//! "Open" and "Show in Files" actions.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use futures_util::StreamExt;
use zbus::Variant as Value;
use zbus::{Connection, proxy};

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
}

impl Notifier {
    pub async fn new(conn: &Connection) -> zbus::Result<Arc<Self>> {
        let proxy = NotificationsProxy::new(conn).await?;
        let me = Arc::new(Self {
            conn: conn.clone(),
            proxy: proxy.clone(),
            files: Mutex::new(HashMap::new()),
            in_progress: Mutex::new(HashMap::new()),
            phone: Mutex::new(HashMap::new()),
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
                } else if me.is_phone_popup(args.id) {
                    me.open_app_page("notifications").await;
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

    fn take_in_progress(&self, key: &str) -> u32 {
        self.in_progress
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(key)
            .unwrap_or(0)
    }

    fn is_phone_popup(&self, id: u32) -> bool {
        let g = self.phone.lock().unwrap_or_else(|e| e.into_inner());
        g.values().any(|v| *v == id)
    }

    /// Shows a page of the Continuum window, starting the app if needed.
    async fn open_app_page(&self, page: &str) {
        let arg = Value::from(page);
        let call = self
            .conn
            .call_method(
                Some(APP_ID),
                "/io/github/sharjeelmazhar/Continuum",
                Some("org.gtk.Actions"),
                "Activate",
                &("show-page", vec![arg], HashMap::<&str, Value<'_>>::new()),
            )
            .await;
        if call.is_err() {
            let ui = std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(|d| d.join("continuum-ui")))
                .filter(|p| p.exists())
                .map(PathBuf::into_os_string)
                .unwrap_or_else(|| "continuum-ui".into());
            let _ = std::process::Command::new(ui).spawn();
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
            "{} · {device}\n{}",
            glib_escape(&n.app),
            glib_escape(&n.text)
        );
        let hints = HashMap::from([("desktop-entry", Value::from(APP_ID))]);
        let actions = ["default", "Open"];
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
            let mut g = self.phone.lock().unwrap_or_else(|e| e.into_inner());
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
                let body = format!("From {device} · saved in {folder}");
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
                    &format!("To {device} · in Downloads/Continuum"),
                    &[],
                )
                .await;
            }
            Progress::SendFailed { device, reason } => {
                let replaces = self.take_in_progress(&format!("send:{device}"));
                self.show(
                    replaces,
                    &format!("Couldn't send files to {device}"),
                    reason,
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
                    reason,
                    &[],
                )
                .await;
            }
        }
    }
}

/// Saves the app icon the phone sent (once per app) and returns its path.
fn app_icon(n: &PhoneNotification) -> Option<String> {
    use base64::Engine;
    if n.icon.is_empty() || n.pkg.is_empty() {
        return None;
    }
    let name: String = n
        .pkg
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '.' || *c == '_')
        .collect();
    let dir = crate::sysinfo::cache_dir().join("icons");
    let path = dir.join(format!("{}.png", name.trim_start_matches('.')));
    if !path.exists() {
        let png = base64::engine::general_purpose::STANDARD
            .decode(&n.icon)
            .ok()?;
        if !png.starts_with(b"\x89PNG") {
            return None;
        }
        std::fs::create_dir_all(&dir).ok()?;
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
fn file_uri(path: &Path) -> String {
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
