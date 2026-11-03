//! The phone as a data source: requests with replies (photos, messages), photo downloads and
//! the list of the phone's current notifications (see "Requests" in `docs/PROTOCOL.md`).

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use continuum_proto::message::{Message, PhoneNotification, PhotoFetchReply, PhotoPart};
use serde_json::Value;
use tokio::sync::oneshot;

pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);
pub const FETCH_TIMEOUT: Duration = Duration::from_secs(120);
pub const MAX_FETCH_BYTES: u64 = 50 * 1024 * 1024;
const PART_BYTES: u64 = 512 * 1024;
/// Notifications kept per device; the oldest go first.
const MAX_NOTIFICATIONS: usize = 200;

/// A downloaded photo.
#[derive(Debug, Clone, PartialEq)]
pub struct Fetched {
    pub name: String,
    pub mime: String,
    pub data: Vec<u8>,
}

enum Pending {
    Reply {
        device: String,
        tx: oneshot::Sender<Message>,
    },
    Fetch {
        device: String,
        meta: Option<PhotoFetchReply>,
        data: Vec<u8>,
        tx: oneshot::Sender<Result<Fetched, String>>,
    },
}

impl Pending {
    fn device(&self) -> &str {
        match self {
            Self::Reply { device, .. } | Self::Fetch { device, .. } => device,
        }
    }
}

#[derive(Default)]
pub struct Requests {
    next: AtomicU32,
    pending: Mutex<HashMap<u32, Pending>>,
}

/// What the caller waits on after registering a request.
pub enum Waiter {
    Reply(oneshot::Receiver<Message>),
    Fetch(oneshot::Receiver<Result<Fetched, String>>),
}

impl Requests {
    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<u32, Pending>> {
        self.pending.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Registers a request to `device` and returns its `req` id.
    pub fn register(&self, device: &str, fetch: bool) -> (u32, Waiter) {
        let req = self.next.fetch_add(1, Ordering::Relaxed).wrapping_add(1);
        let (pending, waiter) = if fetch {
            let (tx, rx) = oneshot::channel();
            (
                Pending::Fetch {
                    device: device.to_owned(),
                    meta: None,
                    data: Vec::new(),
                    tx,
                },
                Waiter::Fetch(rx),
            )
        } else {
            let (tx, rx) = oneshot::channel();
            (
                Pending::Reply {
                    device: device.to_owned(),
                    tx,
                },
                Waiter::Reply(rx),
            )
        };
        self.lock().insert(req, pending);
        (req, waiter)
    }

    pub fn cancel(&self, req: u32) {
        self.lock().remove(&req);
    }

    /// Routes a `*.reply` or `photos.part` from `device`. Anything that doesn't match an open
    /// request from that device (late, forged, inconsistent) is dropped.
    pub fn deliver(&self, device: &str, msg: Message) {
        let Some(req) = msg
            .body
            .get("req")
            .and_then(Value::as_u64)
            .and_then(|r| u32::try_from(r).ok())
        else {
            return;
        };
        let mut g = self.lock();
        if g.get(&req).is_none_or(|p| p.device() != device) {
            return;
        }
        match g.remove(&req) {
            Some(Pending::Reply { tx, .. }) => {
                let _ = tx.send(msg);
            }
            Some(Pending::Fetch {
                device: d,
                meta,
                mut data,
                tx,
            }) => match step(meta, &mut data, &msg) {
                Ok(Some(meta)) if data.len() as u64 == meta.size => {
                    let _ = tx.send(Ok(Fetched {
                        name: meta.name,
                        mime: meta.mime,
                        data,
                    }));
                }
                Ok(meta) => {
                    g.insert(
                        req,
                        Pending::Fetch {
                            device: d,
                            meta,
                            data,
                            tx,
                        },
                    );
                }
                Err(e) => {
                    let _ = tx.send(Err(e));
                }
            },
            None => {}
        }
    }
}

/// One step of a photo download: the `photos.fetch.reply`, then its parts in order.
fn step(
    meta: Option<PhotoFetchReply>,
    data: &mut Vec<u8>,
    msg: &Message,
) -> Result<Option<PhotoFetchReply>, String> {
    match meta {
        None => {
            let r: PhotoFetchReply = msg.body().map_err(|e| e.to_string())?;
            if let Some(e) = r.error {
                return Err(e);
            }
            if r.size > MAX_FETCH_BYTES || u64::from(r.parts) != r.size.div_ceil(PART_BYTES) {
                return Err("the phone sent an inconsistent photo".into());
            }
            data.reserve_exact(r.size as usize);
            Ok(Some(r))
        }
        Some(meta) => {
            let p: PhotoPart = msg.body().map_err(|e| e.to_string())?;
            let expected = (data.len() as u64 / PART_BYTES) as u32;
            let bytes = B64.decode(p.data).map_err(|e| e.to_string())?;
            let left = meta.size - data.len() as u64;
            if p.part != expected || bytes.len() as u64 != left.min(PART_BYTES) {
                return Err("the phone sent a photo part out of order".into());
            }
            data.extend_from_slice(&bytes);
            Ok(Some(meta))
        }
    }
}

/// Current phone notifications per device, newest last.
#[derive(Default)]
pub struct Notifications {
    by_device: Mutex<HashMap<String, Vec<PhoneNotification>>>,
}

impl Notifications {
    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, Vec<PhoneNotification>>> {
        self.by_device.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Stores it; true if it's new or its title or text changed (worth popping up).
    pub fn posted(&self, device: &str, n: PhoneNotification) -> bool {
        let mut g = self.lock();
        let list = g.entry(device.to_owned()).or_default();
        let changed = list
            .iter()
            .find(|o| o.key == n.key)
            .is_none_or(|o| o.title != n.title || o.text != n.text);
        list.retain(|o| o.key != n.key);
        list.push(n);
        if list.len() > MAX_NOTIFICATIONS {
            list.remove(0);
        }
        changed
    }

    /// True if it was there.
    pub fn removed(&self, device: &str, key: &str) -> bool {
        let mut g = self.lock();
        let Some(list) = g.get_mut(device) else {
            return false;
        };
        let before = list.len();
        list.retain(|o| o.key != key);
        list.len() != before
    }

    /// The phone resends everything after connecting.
    pub fn clear(&self, device: &str) {
        self.lock().remove(device);
    }

    pub fn list(&self, device: &str) -> Vec<PhoneNotification> {
        self.lock().get(device).cloned().unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reply(kind: &str, body: Value) -> Message {
        Message {
            kind: kind.into(),
            body,
        }
    }

    #[tokio::test]
    async fn replies_reach_only_their_request() {
        let r = Requests::default();
        let (req, w) = r.register("phone", false);
        let Waiter::Reply(rx) = w else { panic!() };
        // Another device can't answer for it, nor can a wrong id.
        r.deliver(
            "other",
            reply("sms.threads.reply", serde_json::json!({"req": req})),
        );
        r.deliver(
            "phone",
            reply("x.reply", serde_json::json!({"req": req + 1})),
        );
        r.deliver(
            "phone",
            reply(
                "sms.threads.reply",
                serde_json::json!({"req": req, "threads": []}),
            ),
        );
        assert_eq!(rx.await.unwrap().kind, "sms.threads.reply");
        assert!(r.lock().is_empty());
    }

    #[tokio::test]
    async fn photo_download_assembles_parts() {
        let r = Requests::default();
        let (req, w) = r.register("phone", true);
        let Waiter::Fetch(rx) = w else { panic!() };
        let bytes: Vec<u8> = (0..(PART_BYTES as usize + 10)).map(|i| i as u8).collect();
        let meta = serde_json::json!({"req": req, "name": "a.jpg", "mime": "image/jpeg", "size": bytes.len(), "parts": 2});
        r.deliver("phone", reply("photos.fetch.reply", meta));
        for (i, chunk) in bytes.chunks(PART_BYTES as usize).enumerate() {
            let p = serde_json::json!({"req": req, "part": i, "data": B64.encode(chunk)});
            r.deliver("phone", reply("photos.part", p));
        }
        let got = rx.await.unwrap().unwrap();
        assert_eq!((got.name.as_str(), got.data), ("a.jpg", bytes));
    }

    #[tokio::test]
    async fn bad_photo_parts_fail() {
        let r = Requests::default();
        let (req, w) = r.register("phone", true);
        let Waiter::Fetch(rx) = w else { panic!() };
        let meta = serde_json::json!({"req": req, "name": "a", "mime": "image/png", "size": 10, "parts": 1});
        r.deliver("phone", reply("photos.fetch.reply", meta));
        let p = serde_json::json!({"req": req, "part": 0, "data": B64.encode([0u8; 11])});
        r.deliver("phone", reply("photos.part", p));
        assert!(rx.await.unwrap().is_err());

        // Too big, or a part count that doesn't match the size.
        for (size, parts) in [(MAX_FETCH_BYTES + 1, 101), (10, 2)] {
            let (req, w) = r.register("phone", true);
            let Waiter::Fetch(rx) = w else { panic!() };
            let meta = serde_json::json!({"req": req, "size": size, "parts": parts});
            r.deliver("phone", reply("photos.fetch.reply", meta));
            assert!(rx.await.unwrap().is_err());
        }

        let (req, w) = r.register("phone", true);
        let Waiter::Fetch(rx) = w else { panic!() };
        r.deliver(
            "phone",
            reply(
                "photos.fetch.reply",
                serde_json::json!({"req": req, "error": "permission"}),
            ),
        );
        assert_eq!(rx.await.unwrap().unwrap_err(), "permission");
    }

    #[test]
    fn notifications_replace_and_cap() {
        let n = Notifications::default();
        let mk = |key: &str, title: &str| PhoneNotification {
            key: key.into(),
            pkg: String::new(),
            app: String::new(),
            title: title.into(),
            text: String::new(),
            time: 0,
            icon: String::new(),
            reply: false,
            actions: vec![],
            silent: false,
        };
        assert!(n.posted("p", mk("a", "1")));
        assert!(!n.posted("p", mk("a", "1")));
        assert!(n.posted("p", mk("a", "2")));
        assert_eq!(n.list("p").len(), 1);
        assert_eq!(n.list("p")[0].title, "2");
        assert!(n.removed("p", "a"));
        assert!(!n.removed("p", "a"));
        for i in 0..=MAX_NOTIFICATIONS {
            n.posted("p", mk(&i.to_string(), ""));
        }
        assert_eq!(n.list("p").len(), MAX_NOTIFICATIONS);
        assert_eq!(n.list("p")[0].key, "1");
    }
}
