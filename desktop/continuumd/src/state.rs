//! Shared daemon state: paired devices, live connections and the active pairing secret.

use std::collections::{HashMap, VecDeque};
use std::net::IpAddr;
use std::sync::atomic::{AtomicU16, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use continuum_proto::identity::{Fingerprint, Identity};
use continuum_proto::message::{BatteryStatus, DeviceType, Message};
use continuum_proto::pairing::{PairUri, Secret, random_secret};
use tokio::sync::{Notify, broadcast, mpsc};

use crate::store::{PairedDevice, Store};

pub const PAIRING_TTL: Duration = Duration::from_secs(120);
const MAX_PAIR_FAILURES_PER_MINUTE: usize = 5;

/// Something the UI should know about.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    DevicesChanged,
    Paired { id: String, name: String },
}

/// A connected, paired peer.
pub struct Live {
    pub conn: u64,
    pub battery: Option<BatteryStatus>,
    pub tx: mpsc::Sender<Message>,
    pub kill: Arc<Notify>,
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
    ) -> Arc<Self> {
        let devices = store.load_devices();
        Arc::new(Self {
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

    fn emit(&self, e: Event) {
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
        self.emit(Event::DevicesChanged);
        Ok(live)
    }

    /// Registers a live connection; an older connection from the same device is closed.
    pub fn connected(&self, id: &str, live: Live, ip: IpAddr) {
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

    pub fn set_battery(&self, id: &str, b: BatteryStatus) {
        if let Some(l) = self.lock().live.get_mut(id) {
            l.battery = Some(b);
        }
        self.emit(Event::DevicesChanged);
    }
}
