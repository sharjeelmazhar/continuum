//! Sending files to a phone (PROTOCOL.md, "File transfer"). The phone can't accept incoming
//! connections, so after it accepts an offer it opens the data connection itself and we stream
//! the bytes down it. Unfinished jobs are offered again when the phone reconnects.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use continuum_proto::message::{FileMeta, ShareAccept, ShareOffer};
use ring::digest::{Context, SHA256};
use tokio::io::{AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::transfer::MAX_FILES;

/// Give up on a job after this many failed attempts in a row, or after a day.
pub const MAX_ATTEMPTS: u32 = 5;
const JOB_TTL: Duration = Duration::from_secs(24 * 3600);
const CHUNK: usize = 256 << 10;

struct Job {
    device_id: String,
    paths: Vec<PathBuf>,
    offer: ShareOffer,
    /// Set by `share.accept`: the data connection's token and where each file resumes.
    accepted: Option<(String, Vec<u64>)>,
    attempts: u32,
    created: Instant,
}

#[derive(Default)]
pub struct Outbox {
    jobs: Mutex<HashMap<String, Job>>,
}

/// What a finished or failed job was, for notifications.
pub struct Done {
    pub names: Vec<String>,
    pub retry: Option<ShareOffer>,
}

impl Outbox {
    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, Job>> {
        self.jobs.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Checks the files and queues a job; the returned offer goes to the phone.
    pub fn add(&self, device_id: &str, paths: Vec<PathBuf>) -> Result<ShareOffer, String> {
        if paths.is_empty() || paths.len() > MAX_FILES {
            return Err(format!(
                "between 1 and {MAX_FILES} files can be sent at once"
            ));
        }
        let mut files = Vec::with_capacity(paths.len());
        for p in &paths {
            let meta = std::fs::metadata(p).map_err(|e| format!("{}: {e}", p.display()))?;
            if !meta.is_file() {
                return Err(format!(
                    "{} is not a file (folders can't be sent yet)",
                    p.display()
                ));
            }
            std::fs::File::open(p).map_err(|e| format!("{}: {e}", p.display()))?;
            let name = p
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "file".into());
            files.push(FileMeta {
                name,
                size: meta.len(),
                mime: String::new(),
            });
        }
        // Same files sent again → same id, so the phone resumes what it already has.
        let mut id = Context::new(&SHA256);
        for (p, f) in paths.iter().zip(&files) {
            let mtime = std::fs::metadata(p)
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_nanos());
            id.update(format!("{}\0{}\0{mtime}\n", p.display(), f.size).as_bytes());
        }
        let offer = ShareOffer {
            transfer_id: hex::encode(&id.finish().as_ref()[..16]),
            files,
        };
        let mut jobs = self.lock();
        jobs.retain(|_, j| j.created.elapsed() < JOB_TTL);
        if let Some(j) = jobs.get(&offer.transfer_id)
            && j.device_id == device_id
        {
            return Ok(j.offer.clone());
        }
        jobs.insert(
            offer.transfer_id.clone(),
            Job {
                device_id: device_id.to_owned(),
                paths,
                offer: offer.clone(),
                accepted: None,
                attempts: 0,
                created: Instant::now(),
            },
        );
        Ok(offer)
    }

    /// Offers for every unfinished job of a device (sent again after it reconnects).
    pub fn pending(&self, device_id: &str) -> Vec<ShareOffer> {
        let mut jobs = self.lock();
        jobs.retain(|_, j| j.created.elapsed() < JOB_TTL);
        jobs.values_mut()
            .filter(|j| j.device_id == device_id)
            .map(|j| {
                j.accepted = None;
                j.offer.clone()
            })
            .collect()
    }

    /// Drops every job for a device (it was unpaired).
    pub fn forget_device(&self, device_id: &str) {
        self.lock().retain(|_, j| j.device_id != device_id);
    }

    /// Records the phone's `share.accept`; returns the number of files.
    pub fn accepted(&self, device_id: &str, a: ShareAccept) -> Result<usize, String> {
        let mut jobs = self.lock();
        let job = jobs
            .get_mut(&a.transfer_id)
            .filter(|j| j.device_id == device_id)
            .ok_or("unknown transfer")?;
        let valid = a.token.len() == 32
            && a.offsets.len() == job.offer.files.len()
            && a.offsets
                .iter()
                .zip(&job.offer.files)
                .all(|(o, f)| *o <= f.size);
        if !valid {
            return Err("invalid share.accept".into());
        }
        job.accepted = Some((a.token, a.offsets));
        Ok(job.offer.files.len())
    }

    /// Ends a job on `share.result` / `share.reject`. A failed job is offered again until it
    /// has used up its attempts.
    pub fn finish(&self, device_id: &str, transfer_id: &str, ok: bool) -> Option<Done> {
        let mut jobs = self.lock();
        let job = jobs
            .get_mut(transfer_id)
            .filter(|j| j.device_id == device_id)?;
        let names = job.offer.files.iter().map(|f| f.name.clone()).collect();
        job.attempts += 1;
        job.accepted = None;
        if !ok && job.attempts < MAX_ATTEMPTS {
            return Some(Done {
                names,
                retry: Some(job.offer.clone()),
            });
        }
        jobs.remove(transfer_id);
        Some(Done { names, retry: None })
    }

    /// Streams a job down a data connection the phone opened with `token`.
    /// Returns `None` if the token isn't ours (it may belong to an incoming transfer).
    pub async fn stream<W: AsyncWrite + Unpin>(
        &self,
        device_id: &str,
        token: &str,
        out: &mut W,
    ) -> Option<Result<u64, String>> {
        let (paths, files, offsets) = {
            let mut jobs = self.lock();
            let job = jobs.values_mut().find(|j| {
                j.device_id == device_id && j.accepted.as_ref().is_some_and(|(t, _)| t == token)
            })?;
            let (_, offsets) = job.accepted.take()?;
            (job.paths.clone(), job.offer.files.clone(), offsets)
        };
        Some(send_files(&paths, &files, &offsets, out).await)
    }
}

async fn send_files<W: AsyncWrite + Unpin>(
    paths: &[PathBuf],
    files: &[FileMeta],
    offsets: &[u64],
    out: &mut W,
) -> Result<u64, String> {
    let mut buf = vec![0u8; CHUNK];
    let mut sent = 0u64;
    for ((path, meta), &offset) in paths.iter().zip(files).zip(offsets) {
        let mut f = tokio::fs::File::open(path)
            .await
            .map_err(|e| format!("{}: {e}", meta.name))?;
        let mut hash = Context::new(&SHA256);
        let mut pos = 0u64;
        loop {
            let n = f.read(&mut buf).await.map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            hash.update(&buf[..n]);
            // Bytes the phone already has are only hashed.
            let skip = offset.saturating_sub(pos).min(n as u64) as usize;
            if n > skip {
                out.write_all(&buf[skip..n])
                    .await
                    .map_err(|e| e.to_string())?;
                sent += (n - skip) as u64;
            }
            pos += n as u64;
            if pos > meta.size {
                break;
            }
        }
        if pos != meta.size {
            return Err(format!("{} changed while sending", meta.name));
        }
        out.write_all(hash.finish().as_ref())
            .await
            .map_err(|e| e.to_string())?;
    }
    out.flush().await.map_err(|e| e.to_string())?;
    Ok(sent)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn streams_from_offsets_with_whole_file_hashes() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.txt");
        std::fs::write(&a, b"hello world").unwrap();
        let ob = Outbox::default();
        let offer = ob.add("dev", vec![a.clone()]).unwrap();
        assert_eq!(offer.files[0].size, 11);
        assert!(ob.add("dev", vec![dir.path().to_path_buf()]).is_err());
        let token = "t".repeat(32);
        let bad = ShareAccept {
            transfer_id: offer.transfer_id.clone(),
            token: token.clone(),
            offsets: vec![12],
        };
        assert!(ob.accepted("dev", bad).is_err());
        let ok = ShareAccept {
            transfer_id: offer.transfer_id.clone(),
            token: token.clone(),
            offsets: vec![6],
        };
        assert!(ob.accepted("other", ok.clone()).is_err());
        ob.accepted("dev", ok).unwrap();
        assert!(ob.stream("other", &token, &mut Vec::new()).await.is_none());
        let mut out = Vec::new();
        assert_eq!(ob.stream("dev", &token, &mut out).await, Some(Ok(5)));
        assert_eq!(&out[..5], b"world");
        assert_eq!(
            &out[5..],
            ring::digest::digest(&SHA256, b"hello world").as_ref()
        );
        // The token is single use.
        assert!(ob.stream("dev", &token, &mut Vec::new()).await.is_none());
        // Failures are retried, then given up.
        for _ in 1..MAX_ATTEMPTS {
            assert!(
                ob.finish("dev", &offer.transfer_id, false)
                    .unwrap()
                    .retry
                    .is_some()
            );
        }
        assert!(
            ob.finish("dev", &offer.transfer_id, false)
                .unwrap()
                .retry
                .is_none()
        );
        assert!(ob.finish("dev", &offer.transfer_id, true).is_none());
    }
}
