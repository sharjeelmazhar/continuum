//! The phone as a data source: requests with replies (photos, messages), photo downloads and
//! the list of the phone's current notifications (see "Requests" in `docs/PROTOCOL.md`).

use std::collections::HashMap;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt};

use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use continuum_proto::message::{Message, PhoneNotification, PhotoFetchReply, PhotoPart};
use serde_json::Value;
use tokio::sync::oneshot;

pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);
/// A download fails when no part arrives for this long.
pub const FETCH_IDLE_TIMEOUT: Duration = Duration::from_secs(30);
pub const MAX_FETCH_BYTES: u64 = 4 * 1024 * 1024 * 1024;
const PART_BYTES: u64 = 512 * 1024;
/// Notifications kept per device; the oldest go first.
const MAX_NOTIFICATIONS: usize = 200;

/// Where a download goes: `<dir>/<prefix><name the phone gave>`.
#[derive(Debug, Clone)]
pub struct Destination {
    pub dir: PathBuf,
    pub prefix: String,
}

/// A download in progress: written to a hidden partial file, renamed when complete.
struct Download {
    meta: PhotoFetchReply,
    file: File,
    part_path: PathBuf,
    written: u64,
    track: crate::activity::Handle,
}

// Few live at once, so the size difference between variants doesn't matter.
#[allow(clippy::large_enum_variant)]
enum Pending {
    Reply {
        device: String,
        tx: oneshot::Sender<Message>,
    },
    Fetch {
        device: String,
        dest: Destination,
        download: Option<Download>,
        progress: Arc<AtomicU64>,
        tx: oneshot::Sender<Result<PathBuf, String>>,
        /// Set when the phone may stream the bytes raw on a data connection with this token.
        token: Option<String>,
    },
}

/// A raw download whose data connection has arrived (see [`Requests::take_raw`]).
pub struct RawFetch {
    dest: Destination,
    download: Download,
    progress: Arc<AtomicU64>,
    tx: oneshot::Sender<Result<PathBuf, String>>,
}

pub enum TakeRaw {
    Ready(Box<RawFetch>),
    /// The token is ours but the reply with the size hasn't been read yet.
    NotYet,
    Unknown,
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
    /// The finished file, and a counter that moves with every part (for the idle timeout).
    Fetch(oneshot::Receiver<Result<PathBuf, String>>, Arc<AtomicU64>),
}

impl Requests {
    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<u32, Pending>> {
        self.pending.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Registers a request to `device` and returns its `req` id. With `dest`, the reply is a
    /// download (`*.fetch`) saved there.
    pub fn register(&self, device: &str, dest: Option<Destination>) -> (u32, Waiter) {
        self.register_with(device, dest, None)
    }

    /// Like [`register`](Self::register) for a download the phone may stream raw on a data
    /// connection opened with `token`.
    pub fn register_with(
        &self,
        device: &str,
        dest: Option<Destination>,
        token: Option<String>,
    ) -> (u32, Waiter) {
        let req = self.next.fetch_add(1, Ordering::Relaxed).wrapping_add(1);
        let (pending, waiter) = match dest {
            Some(dest) => {
                let (tx, rx) = oneshot::channel();
                let progress = Arc::new(AtomicU64::new(0));
                (
                    Pending::Fetch {
                        device: device.to_owned(),
                        dest,
                        download: None,
                        progress: progress.clone(),
                        tx,
                        token,
                    },
                    Waiter::Fetch(rx, progress),
                )
            }
            None => {
                let (tx, rx) = oneshot::channel();
                (
                    Pending::Reply {
                        device: device.to_owned(),
                        tx,
                    },
                    Waiter::Reply(rx),
                )
            }
        };
        self.lock().insert(req, pending);
        (req, waiter)
    }

    pub fn cancel(&self, req: u32) {
        if let Some(Pending::Fetch {
            download: Some(d), ..
        }) = self.lock().remove(&req)
        {
            let _ = std::fs::remove_file(d.part_path);
        }
    }

    /// Hands over the raw download `device` opened a data connection for. Only that device's
    /// own token, once, and only after its reply said how big the file is.
    pub fn take_raw(&self, device: &str, token: &str) -> TakeRaw {
        let mut g = self.lock();
        let Some(req) = g.iter().find_map(|(req, p)| match p {
            Pending::Fetch {
                device: d,
                token: Some(t),
                ..
            } if d == device && t == token => Some(*req),
            _ => None,
        }) else {
            return TakeRaw::Unknown;
        };
        if let Some(Pending::Fetch { download, .. }) = g.get(&req)
            && !download.as_ref().is_some_and(|d| d.meta.raw)
        {
            return TakeRaw::NotYet;
        }
        match g.remove(&req) {
            Some(Pending::Fetch {
                dest,
                download: Some(download),
                progress,
                tx,
                ..
            }) => TakeRaw::Ready(Box::new(RawFetch {
                dest,
                download,
                progress,
                tx,
            })),
            _ => TakeRaw::Unknown,
        }
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
                dest,
                mut download,
                progress,
                tx,
                token,
            }) => {
                progress.fetch_add(1, Ordering::Relaxed);
                match step(&d, &dest, req, &mut download, &msg, token.is_some()) {
                    Ok(Some(path)) => {
                        let _ = tx.send(Ok(path));
                    }
                    Ok(None) => {
                        g.insert(
                            req,
                            Pending::Fetch {
                                device: d,
                                dest,
                                download,
                                progress,
                                tx,
                                token,
                            },
                        );
                    }
                    Err(e) => {
                        if let Some(d) = download {
                            let _ = std::fs::remove_file(d.part_path);
                        }
                        let _ = tx.send(Err(e));
                    }
                }
            }
            None => {}
        }
    }
}

/// One step of a download: the `*.fetch.reply`, then its parts in order. Returns the final
/// path once the last part is written.
fn step(
    device: &str,
    dest: &Destination,
    req: u32,
    download: &mut Option<Download>,
    msg: &Message,
    raw_allowed: bool,
) -> Result<Option<PathBuf>, String> {
    let Some(d) = download else {
        let r: PhotoFetchReply = msg.body().map_err(|e| e.to_string())?;
        if let Some(e) = r.error {
            return Err(e);
        }
        if r.raw && (!raw_allowed || r.size == 0) {
            return Err("the phone sent an inconsistent file".into());
        }
        if r.size > MAX_FETCH_BYTES || u64::from(r.parts) != r.size.div_ceil(PART_BYTES) {
            return Err("the phone sent an inconsistent file".into());
        }
        std::fs::create_dir_all(&dest.dir).map_err(|e| e.to_string())?;
        let part_path = dest.dir.join(format!(".download-{req}.part"));
        let file = File::create(&part_path).map_err(|e| e.to_string())?;
        let track = crate::activity::ACTIVITY.start(device, &r.name, false, r.size, 0);
        *download = Some(Download {
            meta: r,
            file,
            part_path,
            written: 0,
            track,
        });
        return finish_if_done(dest, download);
    };
    if d.meta.raw {
        return Err("the phone sent parts for a raw download".into());
    }
    let p: PhotoPart = msg.body().map_err(|e| e.to_string())?;
    let bytes = B64.decode(p.data).map_err(|e| e.to_string())?;
    let left = d.meta.size - d.written;
    if u64::from(p.part) != d.written / PART_BYTES || bytes.len() as u64 != left.min(PART_BYTES) {
        return Err("the phone sent a part out of order".into());
    }
    d.file.write_all(&bytes).map_err(|e| e.to_string())?;
    d.written += bytes.len() as u64;
    d.track.add(bytes.len() as u64);
    finish_if_done(dest, download)
}

fn finish_if_done(
    dest: &Destination,
    download: &mut Option<Download>,
) -> Result<Option<PathBuf>, String> {
    let Some(d) = download else { return Ok(None) };
    if d.written < d.meta.size {
        return Ok(None);
    }
    d.file.sync_all().map_err(|e| e.to_string())?;
    let name = continuum_proto::filename::sanitize(&d.meta.name);
    let path = dest.dir.join(format!("{}{name}", dest.prefix));
    std::fs::rename(&d.part_path, &path).map_err(|e| e.to_string())?;
    Ok(Some(path))
}

/// Reads a raw download from its data connection: exactly `size` bytes, then their SHA-256.
/// The file is kept only if both check out.
pub async fn receive_raw<R: AsyncRead + Unpin>(f: RawFetch, stream: &mut R) -> Result<(), String> {
    let RawFetch {
        dest,
        download,
        progress,
        tx,
    } = f;
    let part_path = download.part_path.clone();
    let result = read_raw(&dest, download, &progress, stream).await;
    if result.is_err() {
        let _ = std::fs::remove_file(&part_path);
    }
    let _ = tx.send(result.clone().map(|p| p.unwrap_or_default()));
    result.map(|_| ())
}

async fn read_raw<R: AsyncRead + Unpin>(
    dest: &Destination,
    d: Download,
    progress: &AtomicU64,
    stream: &mut R,
) -> Result<Option<PathBuf>, String> {
    const CHUNK: usize = 1024 * 1024;
    let mut download = Some(d);
    let d = download.as_mut().expect("just set");
    let mut hash = ring::digest::Context::new(&ring::digest::SHA256);
    let mut buf = vec![0u8; CHUNK];
    while d.written < d.meta.size {
        let want = (d.meta.size - d.written).min(CHUNK as u64) as usize;
        let n = stream
            .read(&mut buf[..want])
            .await
            .map_err(|e| e.to_string())?;
        if n == 0 {
            return Err("the phone stopped sending".into());
        }
        d.file.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        hash.update(&buf[..n]);
        d.written += n as u64;
        d.track.add(n as u64);
        progress.fetch_add(1, Ordering::Relaxed);
    }
    let mut sum = [0u8; 32];
    stream
        .read_exact(&mut sum)
        .await
        .map_err(|_| "the phone didn't finish the file".to_owned())?;
    if hash.finish().as_ref() != sum {
        return Err("the file was damaged on the way".into());
    }
    finish_if_done(dest, &mut download)
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
        let (req, w) = r.register("phone", None);
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

    fn dest(dir: &tempfile::TempDir) -> Option<Destination> {
        Some(Destination {
            dir: dir.path().to_owned(),
            prefix: "7-".into(),
        })
    }

    const TOKEN: &str = "0123456789abcdef0123456789abcdef";

    fn raw_reply(req: u32, size: u64) -> Message {
        reply(
            "files.fetch.reply",
            serde_json::json!({ "req": req, "name": "big.bin", "mime": "", "size": size,
                "parts": size.div_ceil(PART_BYTES), "raw": true }),
        )
    }

    fn with_hash(data: &[u8]) -> Vec<u8> {
        let mut v = data.to_vec();
        v.extend_from_slice(ring::digest::digest(&ring::digest::SHA256, data).as_ref());
        v
    }

    #[tokio::test]
    async fn raw_download_lands_after_size_and_hash_check() {
        let dir = tempfile::tempdir().unwrap();
        let r = Requests::default();
        let (req, w) = r.register_with("phone", dest(&dir), Some(TOKEN.into()));
        let Waiter::Fetch(rx, _) = w else { panic!() };
        // Data connection before the reply: wait.
        assert!(matches!(r.take_raw("phone", TOKEN), TakeRaw::NotYet));
        let data: Vec<u8> = (0..3_000_000u32).map(|i| (i % 251) as u8).collect();
        r.deliver("phone", raw_reply(req, data.len() as u64));
        // Another device can't use the token.
        assert!(matches!(r.take_raw("other", TOKEN), TakeRaw::Unknown));
        let TakeRaw::Ready(f) = r.take_raw("phone", TOKEN) else {
            panic!()
        };
        // Single use.
        assert!(matches!(r.take_raw("phone", TOKEN), TakeRaw::Unknown));
        receive_raw(*f, &mut &with_hash(&data)[..]).await.unwrap();
        let path = rx.await.unwrap().unwrap();
        assert_eq!(path, dir.path().join("7-big.bin"));
        assert_eq!(std::fs::read(path).unwrap(), data);
    }

    #[tokio::test]
    async fn damaged_raw_download_is_thrown_away() {
        let dir = tempfile::tempdir().unwrap();
        let r = Requests::default();
        let (req, w) = r.register_with("phone", dest(&dir), Some(TOKEN.into()));
        let Waiter::Fetch(rx, _) = w else { panic!() };
        let data = vec![7u8; 1000];
        r.deliver("phone", raw_reply(req, 1000));
        let TakeRaw::Ready(f) = r.take_raw("phone", TOKEN) else {
            panic!()
        };
        let mut bytes = with_hash(&data);
        bytes[10] ^= 1;
        assert!(receive_raw(*f, &mut &bytes[..]).await.is_err());
        assert!(rx.await.unwrap().is_err());
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    #[tokio::test]
    async fn raw_reply_without_a_token_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let r = Requests::default();
        let (req, w) = r.register("phone", dest(&dir));
        let Waiter::Fetch(rx, _) = w else { panic!() };
        r.deliver("phone", raw_reply(req, 1000));
        assert!(rx.await.unwrap().is_err());
    }

    #[tokio::test]
    async fn download_streams_parts_to_a_file() {
        let dir = tempfile::tempdir().unwrap();
        let r = Requests::default();
        let (req, w) = r.register("phone", dest(&dir));
        let Waiter::Fetch(rx, progress) = w else {
            panic!()
        };
        let bytes: Vec<u8> = (0..(PART_BYTES as usize + 10)).map(|i| i as u8).collect();
        let meta = serde_json::json!({"req": req, "name": "../a.mp4", "mime": "video/mp4", "size": bytes.len(), "parts": 2});
        r.deliver("phone", reply("photos.fetch.reply", meta));
        for (i, chunk) in bytes.chunks(PART_BYTES as usize).enumerate() {
            let p = serde_json::json!({"req": req, "part": i, "data": B64.encode(chunk)});
            r.deliver("phone", reply("photos.part", p));
        }
        let path = rx.await.unwrap().unwrap();
        assert_eq!(path, dir.path().join("7-a.mp4"));
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        assert_eq!(progress.load(Ordering::Relaxed), 3);
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[tokio::test]
    async fn empty_file_finishes_at_once() {
        let dir = tempfile::tempdir().unwrap();
        let r = Requests::default();
        let (req, w) = r.register("phone", dest(&dir));
        let Waiter::Fetch(rx, _) = w else { panic!() };
        let meta = serde_json::json!({"req": req, "name": "e.txt", "size": 0, "parts": 0});
        r.deliver("phone", reply("files.fetch.reply", meta));
        assert_eq!(std::fs::read(rx.await.unwrap().unwrap()).unwrap(), b"");
    }

    #[tokio::test]
    async fn bad_downloads_fail_and_leave_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let r = Requests::default();
        let (req, w) = r.register("phone", dest(&dir));
        let Waiter::Fetch(rx, _) = w else { panic!() };
        let meta = serde_json::json!({"req": req, "name": "a", "mime": "image/png", "size": 10, "parts": 1});
        r.deliver("phone", reply("photos.fetch.reply", meta));
        let p = serde_json::json!({"req": req, "part": 0, "data": B64.encode([0u8; 11])});
        r.deliver("phone", reply("photos.part", p));
        assert!(rx.await.unwrap().is_err());
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);

        // Too big, or a part count that doesn't match the size.
        for (size, parts) in [(MAX_FETCH_BYTES + 1, 8193), (10, 2)] {
            let (req, w) = r.register("phone", dest(&dir));
            let Waiter::Fetch(rx, _) = w else { panic!() };
            let meta = serde_json::json!({"req": req, "size": size, "parts": parts});
            r.deliver("phone", reply("photos.fetch.reply", meta));
            assert!(rx.await.unwrap().is_err());
        }

        let (req, w) = r.register("phone", dest(&dir));
        let Waiter::Fetch(rx, _) = w else { panic!() };
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
