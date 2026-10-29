//! Receiving files (PROTOCOL.md, "File transfer"). Offers from paired devices are accepted
//! automatically; bytes arrive on a separate data connection so they never delay control
//! messages (ADR-0007). Partial files survive crashes and power cuts and are resumed.

use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime};

use continuum_proto::filename;
use continuum_proto::message::{Message, SHARE_RESULT, ShareAccept, ShareOffer, ShareResult};
use continuum_proto::pairing::random_secret;
use ring::digest::{Context, SHA256};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::sync::mpsc;

use crate::store::write_atomic;

pub const MAX_FILES: usize = 1000;
const TOKEN_TTL: Duration = Duration::from_secs(60);
/// Verified progress is recorded (after fsync) this often, so a power cut loses at most this much.
pub const CHECKPOINT_BYTES: u64 = 8 << 20;
const CHUNK: usize = 256 << 10;
/// Keep this much disk free after a transfer.
const DISK_RESERVE: u64 = 256 << 20;
const PARTIAL_MAX_AGE: Duration = Duration::from_secs(7 * 24 * 3600);

/// Progress note for the UI/notifications.
#[derive(Debug, Clone, PartialEq)]
pub enum Progress {
    Started {
        device: String,
        files: usize,
        bytes: u64,
    },
    Finished {
        device: String,
        saved: Vec<PathBuf>,
    },
    Failed {
        device: String,
        reason: String,
    },
}

struct Pending {
    device_id: String,
    device_name: String,
    offer: ShareOffer,
    offsets: Vec<u64>,
    expires: Instant,
    control: mpsc::Sender<Message>,
}

#[derive(Serialize, Deserialize)]
struct PartMeta {
    size: u64,
    offset: u64,
}

pub struct Transfers {
    dir: PathBuf,
    pending: Mutex<HashMap<String, Pending>>,
}

impl Transfers {
    /// `dir` is where finished files go (e.g. `~/Downloads/Continuum`).
    pub fn new(dir: PathBuf) -> Self {
        Self {
            dir,
            pending: Mutex::new(HashMap::new()),
        }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn partial_dir(&self) -> PathBuf {
        self.dir.join(".partial")
    }

    fn part_paths(&self, transfer_id: &str, i: usize) -> (PathBuf, PathBuf) {
        let base = self.partial_dir().join(format!("{transfer_id}-{i}"));
        (base.with_extension("part"), base.with_extension("json"))
    }

    /// Validates an offer and returns the accept message, or a plain-language reason to reject.
    pub fn prepare(
        &self,
        device_id: &str,
        device_name: &str,
        offer: ShareOffer,
        control: mpsc::Sender<Message>,
    ) -> Result<ShareAccept, String> {
        if offer.transfer_id.len() != 32
            || !offer.transfer_id.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err("invalid transfer id".into());
        }
        if offer.files.is_empty() || offer.files.len() > MAX_FILES {
            return Err(format!(
                "between 1 and {MAX_FILES} files can be sent at once"
            ));
        }
        let partial = self.partial_dir();
        fs::create_dir_all(&partial)
            .map_err(|e| format!("cannot create {}: {e}", partial.display()))?;
        let offsets: Vec<u64> = (0..offer.files.len())
            .map(|i| self.resume_offset(&offer.transfer_id, i, offer.files[i].size))
            .collect();
        let remaining: u64 = offer
            .files
            .iter()
            .zip(&offsets)
            .map(|(f, o)| f.size - o)
            .sum();
        if let Ok(st) = rustix::fs::statvfs(&partial)
            && st.f_bavail.saturating_mul(st.f_frsize) < remaining.saturating_add(DISK_RESERVE)
        {
            return Err("not enough free space on the computer".into());
        }
        let token = hex::encode(random_secret());
        let accept = ShareAccept {
            transfer_id: offer.transfer_id.clone(),
            token: token.clone(),
            offsets: offsets.clone(),
        };
        let mut pending = self.pending.lock().unwrap_or_else(|e| e.into_inner());
        pending.retain(|_, p| p.expires > Instant::now());
        pending.insert(
            token,
            Pending {
                device_id: device_id.to_owned(),
                device_name: device_name.to_owned(),
                offer,
                offsets,
                expires: Instant::now() + TOKEN_TTL,
                control,
            },
        );
        Ok(accept)
    }

    /// Bytes already received and verified for file `i` of a transfer (0 if none).
    fn resume_offset(&self, transfer_id: &str, i: usize, size: u64) -> u64 {
        let (part, meta) = self.part_paths(transfer_id, i);
        let Ok(meta) = fs::read(&meta)
            .map_err(|_| ())
            .and_then(|b| serde_json::from_slice::<PartMeta>(&b).map_err(|_| ()))
        else {
            return 0;
        };
        let on_disk = fs::metadata(&part).map(|m| m.len()).unwrap_or(0);
        if meta.size != size {
            0
        } else {
            meta.offset.min(on_disk).min(size)
        }
    }

    /// Runs one data connection: `stream` starts right after the `data.hello` frame.
    /// Returns the progress events to show; the result is also sent to the sender.
    pub async fn receive<R: AsyncRead + Unpin>(
        &self,
        device_id: &str,
        token: &str,
        stream: &mut R,
        progress: impl Fn(Progress),
    ) -> Result<(), String> {
        let p = {
            let mut pending = self.pending.lock().unwrap_or_else(|e| e.into_inner());
            match pending.remove(token) {
                Some(p) if p.device_id == device_id && p.expires > Instant::now() => p,
                Some(p) if p.device_id != device_id => {
                    pending.insert(token.to_owned(), p);
                    return Err("token belongs to another device".into());
                }
                _ => return Err("unknown or expired token".into()),
            }
        };
        let bytes = p.offer.files.iter().map(|f| f.size).sum();
        progress(Progress::Started {
            device: p.device_name.clone(),
            files: p.offer.files.len(),
            bytes,
        });

        let mut saved = Vec::new();
        let result = async {
            for (i, f) in p.offer.files.iter().enumerate() {
                saved.push(
                    self.receive_file(
                        &p.offer.transfer_id,
                        i,
                        f.size,
                        p.offsets[i],
                        &f.name,
                        stream,
                    )
                    .await?,
                );
            }
            Ok::<_, String>(())
        }
        .await;

        let names = saved
            .iter()
            .filter_map(|s: &PathBuf| s.file_name().map(|n| n.to_string_lossy().into_owned()))
            .collect();
        let report = ShareResult {
            transfer_id: p.offer.transfer_id.clone(),
            ok: result.is_ok(),
            saved: names,
            error: result.clone().err(),
        };
        if let Ok(msg) = Message::with_body(SHARE_RESULT, &report) {
            let _ = p.control.send(msg).await;
        }
        match &result {
            Ok(()) => progress(Progress::Finished {
                device: p.device_name,
                saved,
            }),
            Err(e) => progress(Progress::Failed {
                device: p.device_name,
                reason: e.clone(),
            }),
        }
        result
    }

    async fn receive_file<R: AsyncRead + Unpin>(
        &self,
        transfer_id: &str,
        i: usize,
        size: u64,
        offset: u64,
        name: &str,
        stream: &mut R,
    ) -> Result<PathBuf, String> {
        let (part_path, meta_path) = self.part_paths(transfer_id, i);
        let io = |e: std::io::Error| match e.raw_os_error() {
            Some(28) => "the computer ran out of disk space".to_owned(),
            _ => e.to_string(),
        };
        let mut part = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&part_path)
            .map_err(io)?;
        // Re-hash what we already have, then continue exactly where the verified data ends.
        part.set_len(offset).map_err(io)?;
        let mut hash = Context::new(&SHA256);
        let mut buf = vec![0u8; CHUNK];
        let mut left = offset;
        while left > 0 {
            let n = part
                .read(&mut buf[..(left.min(CHUNK as u64) as usize)])
                .map_err(io)?;
            if n == 0 {
                return Err("partial file shrank".into());
            }
            hash.update(&buf[..n]);
            left -= n as u64;
        }
        part.seek(SeekFrom::Start(offset)).map_err(io)?;

        let mut done = offset;
        let mut since_checkpoint = 0u64;
        while done < size {
            let want = (size - done).min(CHUNK as u64) as usize;
            let n = stream
                .read(&mut buf[..want])
                .await
                .map_err(|e| e.to_string())?;
            if n == 0 {
                part.sync_data().map_err(io)?;
                checkpoint(&meta_path, size, done)?;
                return Err("connection closed mid-transfer".into());
            }
            part.write_all(&buf[..n]).map_err(io)?;
            hash.update(&buf[..n]);
            done += n as u64;
            since_checkpoint += n as u64;
            if since_checkpoint >= CHECKPOINT_BYTES {
                part.sync_data().map_err(io)?;
                checkpoint(&meta_path, size, done)?;
                since_checkpoint = 0;
            }
        }
        let mut expected = [0u8; 32];
        stream
            .read_exact(&mut expected)
            .await
            .map_err(|e| e.to_string())?;
        if hash.finish().as_ref() != expected {
            let _ = fs::remove_file(&part_path);
            let _ = fs::remove_file(&meta_path);
            return Err(format!("{name} was damaged on the way; send it again"));
        }
        part.sync_all().map_err(io)?;
        drop(part);

        let safe = filename::sanitize(name);
        let dir = &self.dir;
        let final_name = filename::unique(&safe, |n| dir.join(n).exists());
        let final_path = dir.join(final_name);
        fs::rename(&part_path, &final_path).map_err(io)?;
        let _ = fs::remove_file(&meta_path);
        if let Ok(d) = File::open(dir) {
            let _ = d.sync_all();
        }
        Ok(final_path)
    }

    /// Deletes partial files nobody resumed for a week.
    pub fn clean_old_partials(&self) {
        let Ok(entries) = fs::read_dir(self.partial_dir()) else {
            return;
        };
        for e in entries.flatten() {
            let old = e
                .metadata()
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| SystemTime::now().duration_since(t).ok())
                .is_some_and(|age| age > PARTIAL_MAX_AGE);
            if old {
                let _ = fs::remove_file(e.path());
            }
        }
    }
}

fn checkpoint(meta: &Path, size: u64, offset: u64) -> Result<(), String> {
    let json = serde_json::to_vec(&PartMeta { size, offset }).map_err(|e| e.to_string())?;
    write_atomic(meta, &json).map_err(|e| e.to_string())
}
