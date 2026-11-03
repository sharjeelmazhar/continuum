//! Shared daemon state: paired devices, live connections and the active pairing secret.

use std::collections::{HashMap, VecDeque};
use std::net::IpAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU16, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use continuum_proto::identity::{Fingerprint, Identity};
use continuum_proto::message::{
    self as m, BatteryStatus, CAP_SHARE_RECEIVE, DeviceType, Message, SHARE_OFFER,
};
use continuum_proto::pairing::{PairUri, Secret, random_secret};
use tokio::sync::{Notify, broadcast, mpsc};

use crate::clipboard::ClipHub;
use crate::outbox::Outbox;
use crate::phone::{self, Destination, Notifications, Requests, Waiter};
use crate::store::{PairedDevice, Store};
use crate::transfer::{Progress, Transfers};

pub const PAIRING_TTL: Duration = Duration::from_secs(120);
const MAX_PAIR_FAILURES_PER_MINUTE: usize = 5;

/// Something the UI should know about.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    DevicesChanged,
    Paired {
        id: String,
        name: String,
    },
    Transfer(Progress),
    /// Something the phone reported on its own (`notif.*`, `sms.changed`), body as JSON.
    Phone {
        device: String,
        kind: String,
        body: String,
    },
}

/// Sender and capabilities of a connected peer.
pub type LiveLink = (mpsc::Sender<Message>, Vec<String>);

/// A connected, paired peer.
pub struct Live {
    pub conn: u64,
    pub battery: Option<BatteryStatus>,
    pub tx: mpsc::Sender<Message>,
    pub kill: Arc<Notify>,
    pub caps: Vec<String>,
}

struct Inner {
    devices: Vec<PairedDevice>,
    live: HashMap<String, Live>,
    pairing: Option<(Secret, Instant)>,
    pair_failures: VecDeque<(IpAddr, Instant)>,
}

/// One row of `ListDevices`.
#[derive(Debug, Clone, PartialEq)]
pub struct DeviceStatus {
    pub device: PairedDevice,
    pub connected: bool,
    pub battery: Option<BatteryStatus>,
}

pub struct Daemon {
    pub transfers: Transfers,
    pub outbox: Outbox,
    pub clip: ClipHub,
    pub requests: Requests,
    pub notifications: Notifications,
    pub identity: Identity,
    pub name: String,
    pub device_type: DeviceType,
    pub port: AtomicU16,
    store: Store,
    inner: Mutex<Inner>,
    next_conn: AtomicU64,
    pub events: broadcast::Sender<Event>,
}

impl Daemon {
    pub fn new(
        store: Store,
        identity: Identity,
        name: String,
        device_type: DeviceType,
        download_dir: PathBuf,
    ) -> Arc<Self> {
        let devices = store.load_devices();
        let transfers = Transfers::new(download_dir);
        transfers.clean_old_partials();
        Arc::new(Self {
            transfers,
            outbox: Outbox::default(),
            clip: ClipHub::default(),
            requests: Requests::default(),
            notifications: Notifications::default(),
            identity,
            name,
            device_type,
            port: AtomicU16::new(0),
            store,
            inner: Mutex::new(Inner {
                devices,
                live: HashMap::new(),
                pairing: None,
                pair_failures: VecDeque::new(),
            }),
            next_conn: AtomicU64::new(1),
            events: broadcast::channel(64).0,
        })
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        // A panic while holding the lock leaves plain data behind; keep serving.
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn emit(&self, e: Event) {
        let _ = self.events.send(e);
    }

    pub fn next_conn_id(&self) -> u64 {
        self.next_conn.fetch_add(1, Ordering::Relaxed)
    }

    pub fn paired_by_fp(&self, fp: &Fingerprint) -> Option<PairedDevice> {
        let hex = hex::encode(fp);
        self.lock().devices.iter().find(|d| d.fp == hex).cloned()
    }

    pub fn list(&self) -> Vec<DeviceStatus> {
        let g = self.lock();
        g.devices
            .iter()
            .map(|d| {
                let live = g.live.get(&d.id);
                DeviceStatus {
                    device: d.clone(),
                    connected: live.is_some(),
                    battery: live.and_then(|l| l.battery.clone()),
                }
            })
            .collect()
    }

    /// Starts (or restarts) pairing and returns the QR link.
    pub fn start_pairing(&self, hosts: Vec<String>) -> PairUri {
        let secret = random_secret();
        self.lock().pairing = Some((secret, Instant::now() + PAIRING_TTL));
        PairUri {
            hosts,
            port: self.port.load(Ordering::Relaxed),
            fp: self.identity.fingerprint(),
            secret,
            name: self.name.clone(),
        }
    }

    pub fn cancel_pairing(&self) {
        self.lock().pairing = None;
    }

    /// The active secret, if pairing is open and the address isn't rate-limited.
    pub fn pairing_secret(&self, ip: IpAddr) -> Result<Secret, &'static str> {
        let mut g = self.lock();
        let cutoff = Instant::now() - Duration::from_secs(60);
        while g.pair_failures.front().is_some_and(|(_, t)| *t < cutoff) {
            g.pair_failures.pop_front();
        }
        if g.pair_failures.iter().filter(|(a, _)| *a == ip).count() >= MAX_PAIR_FAILURES_PER_MINUTE
        {
            return Err("too many attempts, wait a minute");
        }
        match g.pairing {
            Some((s, until)) if Instant::now() < until => Ok(s),
            _ => Err("pairing is not open on the computer; show the QR code again"),
        }
    }

    pub fn record_pair_failure(&self, ip: IpAddr) {
        self.lock().pair_failures.push_back((ip, Instant::now()));
    }

    /// Stores a newly paired device and burns the secret.
    pub fn complete_pairing(
        &self,
        fp: &Fingerprint,
        name: String,
        device_type: DeviceType,
    ) -> std::io::Result<PairedDevice> {
        let dev = PairedDevice {
            id: continuum_proto::identity::device_id(fp),
            name,
            device_type,
            fp: hex::encode(fp),
            paired_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            last_ip: None,
        };
        {
            let mut g = self.lock();
            g.pairing = None;
            g.devices.retain(|d| d.id != dev.id);
            g.devices.push(dev.clone());
            self.store.save_devices(&g.devices)?;
        }
        self.emit(Event::Paired {
            id: dev.id.clone(),
            name: dev.name.clone(),
        });
        self.emit(Event::DevicesChanged);
        Ok(dev)
    }

    /// Removes a pairing. Returns the live connection's sender so the caller can say goodbye.
    pub fn unpair(&self, id: &str) -> std::io::Result<Option<Live>> {
        let live = {
            let mut g = self.lock();
            let before = g.devices.len();
            g.devices.retain(|d| d.id != id);
            if g.devices.len() == before {
                return Ok(None);
            }
            self.store.save_devices(&g.devices)?;
            g.live.remove(id)
        };
        self.outbox.forget_device(id);
        self.emit(Event::DevicesChanged);
        Ok(live)
    }

    /// Registers a live connection; an older connection from the same device is closed.
    pub fn connected(&self, id: &str, live: Live, ip: IpAddr) {
        // The phone sends its current notifications again right after hello.
        self.notifications.clear(id);
        let mut g = self.lock();
        if let Some(old) = g.live.insert(id.to_owned(), live) {
            old.kill.notify_one();
        }
        let ip = ip.to_string();
        if let Some(d) = g.devices.iter_mut().find(|d| d.id == id)
            && d.last_ip.as_deref() != Some(ip.as_str())
        {
            d.last_ip = Some(ip);
            if let Err(e) = self.store.save_devices(&g.devices) {
                tracing::warn!("could not save last address: {e}");
            }
        }
        drop(g);
        self.emit(Event::DevicesChanged);
    }

    /// Last known addresses of paired devices that aren't connected right now.
    pub fn wake_targets(&self) -> Vec<IpAddr> {
        let g = self.lock();
        g.devices
            .iter()
            .filter(|d| !g.live.contains_key(&d.id))
            .filter_map(|d| d.last_ip.as_deref()?.parse().ok())
            .collect()
    }

    pub fn disconnected(&self, id: &str, conn: u64) {
        let mut g = self.lock();
        if g.live.get(id).is_some_and(|l| l.conn == conn) {
            g.live.remove(id);
            drop(g);
            self.emit(Event::DevicesChanged);
        }
    }

    /// Name, sender and capabilities of a paired device; the sender is `None` when offline.
    pub fn peer(&self, id: &str) -> Option<(String, Option<LiveLink>)> {
        let g = self.lock();
        let d = g.devices.iter().find(|d| d.id == id)?;
        let live = g.live.get(id).map(|l| (l.tx.clone(), l.caps.clone()));
        Some((d.name.clone(), live))
    }

    /// Senders of connected devices that have a capability.
    pub fn live_with(&self, cap: &str) -> Vec<mpsc::Sender<Message>> {
        self.lock()
            .live
            .values()
            .filter(|l| l.caps.iter().any(|c| c == cap))
            .map(|l| l.tx.clone())
            .collect()
    }

    /// Queues files for a device and offers them now if it's connected.
    pub async fn send_files(&self, id: &str, paths: Vec<PathBuf>) -> Result<String, String> {
        let (name, live) = self.peer(id).ok_or("that device isn't paired")?;
        if let Some((_, caps)) = &live
            && !caps.iter().any(|c| c == CAP_SHARE_RECEIVE)
        {
            return Err(format!("update Continuum on {name} to send files to it"));
        }
        let offer = self.outbox.add(id, paths)?;
        let tid = offer.transfer_id.clone();
        match live {
            Some((tx, _)) => {
                let msg = Message::with_body(SHARE_OFFER, &offer).map_err(|e| e.to_string())?;
                // If the connection just dropped, the offer is repeated on reconnect.
                let _ = tx.send(msg).await;
            }
            None => self.emit(Event::Transfer(Progress::SendQueued { device: name })),
        }
        Ok(tid)
    }

    /// The sender for a connected device that has `cap`, or a message for the user.
    fn link_with(&self, id: &str, cap: &str) -> Result<mpsc::Sender<Message>, String> {
        let (name, live) = self.peer(id).ok_or("that device isn't paired")?;
        let (tx, caps) = live.ok_or_else(|| format!("{name} isn't connected"))?;
        if !caps.iter().any(|c| c == cap) {
            return Err(format!("update Continuum on {name} to use this"));
        }
        Ok(tx)
    }

    /// Sends a request (`photos.list`, `sms.send`, …) and waits for its reply body.
    /// Errors are user-facing text, or `"permission"` (allow access on the phone).
    pub async fn request(
        &self,
        id: &str,
        kind: &str,
        mut body: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let cap = m::REQUESTS
            .iter()
            .find(|(k, _)| *k == kind && !kind.ends_with(".fetch"))
            .map(|(_, c)| *c)
            .ok_or_else(|| format!("unknown request {kind}"))?;
        let tx = self.link_with(id, cap)?;
        let obj = body
            .as_object_mut()
            .ok_or("the request body must be an object")?;
        let (req, waiter) = self.requests.register(id, None);
        obj.insert("req".into(), req.into());
        let Waiter::Reply(rx) = waiter else {
            unreachable!()
        };
        let msg = Message {
            kind: kind.to_owned(),
            body,
        };
        if tx.send(msg).await.is_err() {
            self.requests.cancel(req);
            return Err("the phone disconnected".into());
        }
        let reply = match tokio::time::timeout(phone::REQUEST_TIMEOUT, rx).await {
            Ok(Ok(r)) => r,
            Ok(Err(_)) => return Err("the phone disconnected".into()),
            Err(_) => {
                self.requests.cancel(req);
                return Err("the phone didn't answer in time".into());
            }
        };
        match reply.body.get("error").and_then(|e| e.as_str()) {
            Some(e) => Err(e.to_owned()),
            None => Ok(reply.body),
        }
    }

    /// Downloads a photo, video (`photos.fetch`) or file (`files.fetch`) into `dest`.
    pub async fn fetch(
        &self,
        id: &str,
        kind: &str,
        mut body: serde_json::Value,
        dest: Destination,
    ) -> Result<std::path::PathBuf, String> {
        let cap = match kind {
            "photos.fetch" => m::CAP_PHOTOS,
            "files.fetch" => m::CAP_FILES,
            _ => return Err(format!("unknown download {kind}")),
        };
        let tx = self.link_with(id, cap)?;
        let obj = body
            .as_object_mut()
            .ok_or("the request body must be an object")?;
        let (req, waiter) = self.requests.register(id, Some(dest));
        obj.insert("req".into(), req.into());
        let Waiter::Fetch(mut rx, progress) = waiter else {
            unreachable!()
        };
        let msg = Message {
            kind: kind.to_owned(),
            body,
        };
        if tx.send(msg).await.is_err() {
            self.requests.cancel(req);
            return Err("the phone disconnected".into());
        }
        // Big videos take a while: only a stall counts as a failure.
        let mut seen = 0;
        loop {
            match tokio::time::timeout(phone::FETCH_IDLE_TIMEOUT, &mut rx).await {
                Ok(Ok(r)) => return r,
                Ok(Err(_)) => return Err("the phone disconnected".into()),
                Err(_) => {
                    let now = progress.load(std::sync::atomic::Ordering::Relaxed);
                    if now == seen {
                        self.requests.cancel(req);
                        return Err("the download stalled".into());
                    }
                    seen = now;
                }
            }
        }
    }

    /// Fire-and-forget commands: dismiss/reply/act on a phone notification, ring the phone.
    pub async fn command(
        &self,
        id: &str,
        kind: &str,
        body: serde_json::Value,
    ) -> Result<(), String> {
        let cap = m::COMMANDS
            .iter()
            .find(|(k, _)| *k == kind)
            .map(|(_, c)| *c)
            .ok_or_else(|| format!("unknown command {kind}"))?;
        if !body.is_object() {
            return Err("the command body must be an object".into());
        }
        let tx = self.link_with(id, cap)?;
        let msg = Message {
            kind: kind.to_owned(),
            body,
        };
        tx.send(msg)
            .await
            .map_err(|_| "the phone disconnected".to_owned())
    }

    /// Capabilities of a connected device (empty when offline).
    pub fn caps(&self, id: &str) -> Vec<String> {
        self.peer(id)
            .and_then(|(_, live)| live)
            .map(|(_, caps)| caps)
            .unwrap_or_default()
    }

    pub fn set_battery(&self, id: &str, b: BatteryStatus) {
        if let Some(l) = self.lock().live.get_mut(id) {
            l.battery = Some(b);
        }
        self.emit(Event::DevicesChanged);
    }
}
