//! Desktop notifications for transfers (freedesktop Notifications over D-Bus), with
//! "Open" and "Show in Files" actions.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use futures_util::StreamExt;
use zbus::Variant as Value;
use zbus::{Connection, proxy};

use crate::transfer::Progress;

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
}

impl Notifier {
    pub async fn new(conn: &Connection) -> zbus::Result<Arc<Self>> {
        let proxy = NotificationsProxy::new(conn).await?;
        let me = Arc::new(Self {
            conn: conn.clone(),
            proxy: proxy.clone(),
            files: Mutex::new(HashMap::new()),
            in_progress: Mutex::new(HashMap::new()),
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
