//! D-Bus API for the UI and other desktop clients (see `docs/design/02-continuumd.md`).

use std::sync::Arc;

use continuum_proto::message::{Message, NotifKey, UNPAIR};
use tokio::sync::broadcast::error::RecvError;
use zbus::fdo::{RequestNameFlags, RequestNameReply};
use zbus::object_server::SignalEmitter;
use zbus::{connection, interface};

use crate::notify::Notifier;
use crate::state::{Daemon, Event, PAIRING_TTL};
use crate::sysinfo;

pub const BUS_NAME: &str = "io.github.sharjeelmazhar.Continuum.Daemon";
pub const PATH: &str = "/io/github/sharjeelmazhar/Continuum/Daemon";
pub const IFACE: &str = "io.github.sharjeelmazhar.Continuum.Daemon1";

/// (id, name, type, connected, battery level or -1, charging)
pub type DeviceRow = (String, String, String, bool, i32, bool);

pub struct Api {
    daemon: Arc<Daemon>,
    /// zbus runs methods on its own executor; work that needs Tokio's timers goes here.
    rt: tokio::runtime::Handle,
}

impl Api {
    async fn on_tokio<T: Send + 'static>(
        &self,
        f: impl Future<Output = Result<T, String>> + Send + 'static,
    ) -> zbus::fdo::Result<T> {
        self.rt
            .spawn(f)
            .await
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?
            .map_err(zbus::fdo::Error::Failed)
    }
}

#[interface(name = "io.github.sharjeelmazhar.Continuum.Daemon1")]
impl Api {
    fn list_devices(&self) -> Vec<DeviceRow> {
        self.daemon
            .list()
            .into_iter()
            .map(|s| {
                let ty = serde_json::to_value(s.device.device_type)
                    .ok()
                    .and_then(|v| v.as_str().map(str::to_owned))
                    .unwrap_or_default();
                let (level, charging) = s
                    .battery
                    .map_or((-1, false), |b| (i32::from(b.level), b.charging));
                (s.device.id, s.device.name, ty, s.connected, level, charging)
            })
            .collect()
    }

    /// Opens pairing for two minutes. Returns the QR link and its lifetime in seconds.
    fn start_pairing(&self) -> (String, u32) {
        let uri = self.daemon.start_pairing(sysinfo::lan_addresses());
        (uri.to_uri(), PAIRING_TTL.as_secs() as u32)
    }

    fn cancel_pairing(&self) {
        self.daemon.cancel_pairing();
    }

    async fn unpair(&self, id: &str) -> zbus::fdo::Result<()> {
        let live = self
            .daemon
            .unpair(id)
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
        if let Some(live) = live {
            let _ = live.tx.send(Message::new(UNPAIR)).await;
            live.kill.notify_one();
        }
        Ok(())
    }

    /// Sends files (absolute paths) to a paired device; queued if it isn't connected.
    /// Returns the transfer id.
    async fn send_files(&self, id: &str, paths: Vec<String>) -> zbus::fdo::Result<String> {
        let paths = paths.into_iter().map(Into::into).collect();
        self.daemon
            .send_files(id, paths)
            .await
            .map_err(zbus::fdo::Error::Failed)
    }

    /// Asks a phone for data (`photos.list`, `sms.threads`, `sms.send`, …; see PROTOCOL.md).
    /// `body` and the result are JSON objects. A failed request's error message is the text to
    /// show, or `permission` (the user has to allow access on the phone).
    async fn request(&self, id: &str, kind: &str, body: &str) -> zbus::fdo::Result<String> {
        let body =
            serde_json::from_str(body).map_err(|e| zbus::fdo::Error::InvalidArgs(e.to_string()))?;
        let (daemon, id, kind) = (self.daemon.clone(), id.to_owned(), kind.to_owned());
        let reply = self
            .on_tokio(async move { daemon.request(&id, &kind, body).await })
            .await?;
        Ok(reply.to_string())
    }

    /// Downloads a photo (MediaStore id) into the cache and returns its path.
    async fn fetch_photo(&self, id: &str, photo: i64) -> zbus::fdo::Result<String> {
        let dir = sysinfo::cache_dir().join("photos").join(hex_only(id));
        if let Some(hit) = cached(&dir, photo) {
            return Ok(hit);
        }
        let (daemon, device) = (self.daemon.clone(), id.to_owned());
        let f = self
            .on_tokio(async move { daemon.fetch_photo(&device, photo).await })
            .await?;
        let name = continuum_proto::filename::sanitize(&f.name);
        let path = dir.join(format!("{photo}-{name}"));
        std::fs::create_dir_all(&dir)
            .and_then(|_| crate::store::write_atomic(&path, &f.data))
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;
        Ok(path.to_string_lossy().into_owned())
    }

    /// Current notifications on a phone, as a JSON array of `notif.posted` bodies (oldest first).
    fn list_notifications(&self, id: &str) -> String {
        serde_json::to_string(&self.daemon.notifications.list(id)).unwrap_or_else(|_| "[]".into())
    }

    /// `notif.dismiss`, `notif.reply` or `notif.action` with a JSON body.
    async fn notification_command(
        &self,
        id: &str,
        kind: &str,
        body: &str,
    ) -> zbus::fdo::Result<()> {
        let body =
            serde_json::from_str(body).map_err(|e| zbus::fdo::Error::InvalidArgs(e.to_string()))?;
        if kind == "notif.dismiss"
            && let Some(key) = serde_json::Value::get(&body, "key").and_then(|k| k.as_str())
            && self.daemon.notifications.removed(id, key)
        {
            self.daemon.emit(Event::Phone {
                device: id.to_owned(),
                kind: "notif.removed".into(),
                body: serde_json::json!({ "key": key }).to_string(),
            });
        }
        self.daemon
            .notification_command(id, kind, body)
            .await
            .map_err(zbus::fdo::Error::Failed)
    }

    #[zbus(property)]
    fn name(&self) -> String {
        self.daemon.name.clone()
    }

    #[zbus(signal)]
    async fn devices_changed(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;

    /// Something a phone reported: `notif.posted`, `notif.removed` or `sms.changed`.
    #[zbus(signal)]
    async fn phone_event(
        emitter: &SignalEmitter<'_>,
        id: &str,
        kind: &str,
        body: &str,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn paired(emitter: &SignalEmitter<'_>, id: &str, name: &str) -> zbus::Result<()>;
}

/// Claims the bus name (fails if another daemon already runs) and forwards events as signals.
pub async fn run(daemon: Arc<Daemon>) -> zbus::Result<connection::Connection> {
    let mut events = daemon.events.subscribe();
    let names = daemon.clone();
    let conn = connection::Builder::session()
        .serve_at(
            PATH,
            Api {
                daemon,
                rt: tokio::runtime::Handle::current(),
            },
        )
        .build()
        .await?;
    // DO_NOT_QUEUE: a second daemon must fail instead of silently waiting for the name.
    match conn
        .request_name_with_flags(BUS_NAME, RequestNameFlags::DO_NOT_QUEUE)
        .await?
    {
        RequestNameReply::PrimaryOwner | RequestNameReply::AlreadyOwner => {}
        _ => return Err(zbus::Error::NameTaken),
    }
    let emitter = SignalEmitter::new(&conn, PATH)?.into_owned();
    let notifier = Notifier::new(&conn)
        .await
        .map_err(|e| tracing::warn!("desktop notifications unavailable: {e}"))
        .ok();
    tokio::spawn(async move {
        loop {
            let res = match events.recv().await {
                Ok(Event::DevicesChanged) | Err(RecvError::Lagged(_)) => {
                    Api::devices_changed(&emitter).await
                }
                Ok(Event::Paired { id, name }) => Api::paired(&emitter, &id, &name).await,
                Ok(Event::Transfer(p)) => {
                    if let Some(n) = &notifier {
                        n.on_transfer(&p).await;
                    }
                    Ok(())
                }
                Ok(Event::Phone { device, kind, body }) => {
                    if let Some(n) = &notifier {
                        if kind == "notif.posted"
                            && let Ok(posted) = serde_json::from_str(&body)
                        {
                            let name = names.peer(&device).map(|p| p.0).unwrap_or_default();
                            n.on_phone_notification(&device, &name, &posted).await;
                        } else if kind == "notif.removed"
                            && let Ok(k) = serde_json::from_str::<NotifKey>(&body)
                        {
                            n.on_phone_notification_removed(&device, &k.key).await;
                        }
                    }
                    Api::phone_event(&emitter, &device, &kind, &body).await
                }
                Err(RecvError::Closed) => return,
            };
            if let Err(e) = res {
                tracing::warn!("could not emit D-Bus signal: {e}");
            }
        }
    });
    Ok(conn)
}

/// Device ids are hex; anything else never reaches a path.
fn hex_only(id: &str) -> String {
    id.chars()
        .filter(char::is_ascii_hexdigit)
        .take(64)
        .collect()
}

/// A photo downloaded earlier (`<photo id>-<name>` in `dir`).
fn cached(dir: &std::path::Path, photo: i64) -> Option<String> {
    let prefix = format!("{photo}-");
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .find(|e| e.file_name().to_string_lossy().starts_with(&prefix))
        .map(|e| e.path().to_string_lossy().into_owned())
}
