//! D-Bus API for the UI and other desktop clients (see `docs/design/02-continuumd.md`).

use std::sync::Arc;

use continuum_proto::message::{Message, UNPAIR};
use tokio::sync::broadcast::error::RecvError;
use zbus::fdo::{RequestNameFlags, RequestNameReply};
use zbus::object_server::SignalEmitter;
use zbus::{connection, interface};

use crate::notify::Notifier;
use crate::state::{Daemon, Event, PAIRING_TTL};
use crate::sysinfo;

pub const BUS_NAME: &str = "io.github.sharjeelmazhar.Continuum.Daemon";
pub const PATH: &str = "/io/github/sharjeelmazhar/Continuum/Daemon";

/// (id, name, type, connected, battery level or -1, charging)
pub type DeviceRow = (String, String, String, bool, i32, bool);

pub struct Api {
    daemon: Arc<Daemon>,
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

    #[zbus(property)]
    fn name(&self) -> String {
        self.daemon.name.clone()
    }

    #[zbus(signal)]
    async fn devices_changed(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn paired(emitter: &SignalEmitter<'_>, id: &str, name: &str) -> zbus::Result<()>;
}

/// Claims the bus name (fails if another daemon already runs) and forwards events as signals.
pub async fn run(daemon: Arc<Daemon>) -> zbus::Result<connection::Connection> {
    let mut events = daemon.events.subscribe();
    let conn = connection::Builder::session()
        .serve_at(PATH, Api { daemon })
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
                Err(RecvError::Closed) => return,
            };
            if let Err(e) = res {
                tracing::warn!("could not emit D-Bus signal: {e}");
            }
        }
    });
    Ok(conn)
}
