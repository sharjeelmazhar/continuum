//! Gallery preload: when a phone connects, the newest page of its gallery and their
//! thumbnails are saved where the app looks first (`~/.cache/continuum/gallery/<id>/list.json`
//! and `thumbs-480/<id>/<photo>.jpg`), so opening the app shows them at once.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use base64::Engine;
use serde::Deserialize;

use crate::state::Daemon;

/// Same as the app's first page and tile size.
const PAGE: usize = 120;
const THUMB_PX: u32 = 480;
const PER_REQUEST: usize = 24;

#[derive(Deserialize)]
struct List {
    items: Vec<Item>,
}

#[derive(Deserialize)]
struct Item {
    id: i64,
}

#[derive(Deserialize)]
struct Thumbs {
    items: Vec<Thumb>,
}

#[derive(Deserialize)]
struct Thumb {
    id: i64,
    data: String,
}

fn dir(what: &str, device: &str) -> PathBuf {
    let id: String = device
        .chars()
        .filter(char::is_ascii_hexdigit)
        .take(64)
        .collect();
    crate::sysinfo::cache_dir().join(what).join(id)
}

/// Writes `bytes` to `dir/name` through a temporary file, so a power cut never leaves half.
fn write_atomic(dir: &Path, name: &str, bytes: &[u8]) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let tmp = dir.join(format!("{name}.part"));
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(tmp, dir.join(name))
}

/// Photos whose thumbnail isn't saved yet.
fn missing(thumbs: &Path, ids: &[i64]) -> Vec<i64> {
    ids.iter()
        .copied()
        .filter(|id| !thumbs.join(format!("{id}.jpg")).exists())
        .collect()
}

/// Runs a few seconds after a phone connects (first things first: the phone sends its
/// notifications and battery). Quiet on any error: no access, older phone, gone again.
pub async fn gallery(daemon: Arc<Daemon>, device: String) {
    tokio::time::sleep(Duration::from_secs(3)).await;
    if !daemon.is_connected(&device) {
        return;
    }
    let body = serde_json::json!({ "offset": 0, "limit": PAGE });
    let Ok(reply) = daemon.request(&device, "photos.list", body).await else {
        return;
    };
    let Ok(list) = serde_json::from_value::<List>(reply.clone()) else {
        return;
    };
    if let Ok(bytes) = serde_json::to_vec(&reply) {
        let _ = write_atomic(&dir("gallery", &device), "list.json", &bytes);
    }
    let thumbs = dir("thumbs-480", &device);
    let ids: Vec<i64> = list.items.iter().map(|i| i.id).collect();
    let mut todo = missing(&thumbs, &ids);
    let mut fetched = 0;
    while !todo.is_empty() {
        let batch: Vec<i64> = todo.iter().copied().take(PER_REQUEST).collect();
        let body = serde_json::json!({ "ids": batch, "size": THUMB_PX });
        let Ok(reply) = daemon.request(&device, "photos.thumbs", body).await else {
            break;
        };
        let Ok(got) = serde_json::from_value::<Thumbs>(reply) else {
            break;
        };
        if got.items.is_empty() {
            break;
        }
        for t in got.items {
            if !batch.contains(&t.id) {
                continue;
            }
            if let Ok(jpeg) = base64::engine::general_purpose::STANDARD.decode(t.data) {
                let _ = write_atomic(&thumbs, &format!("{}.jpg", t.id), &jpeg);
                fetched += 1;
            }
        }
        // Left out (over the phone's size budget): asked again in the next round.
        todo = missing(&thumbs, &todo);
    }
    tracing::debug!("preloaded the gallery list and {fetched} thumbnails");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_missing_thumbnails_are_asked_for() {
        let tmp = tempfile::tempdir().unwrap();
        write_atomic(tmp.path(), "2.jpg", b"x").unwrap();
        assert_eq!(missing(tmp.path(), &[1, 2, 3]), vec![1, 3]);
        assert!(!tmp.path().join("2.jpg.part").exists());
    }

    #[test]
    fn device_ids_never_leave_the_cache() {
        let d = dir("gallery", "../../etc");
        assert!(d.ends_with("gallery/ec"), "{}", d.display());
    }
}
