//! Transfers in progress (sending, receiving, downloads from phone storage), for the app's
//! transfers button: name, direction, bytes done and how long it has been running.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Instant;

use serde::Serialize;

struct Entry {
    device: String,
    name: String,
    up: bool,
    total: u64,
    done: Arc<AtomicU64>,
    started: Instant,
}

#[derive(Default)]
pub struct Activities {
    next: AtomicU64,
    live: Mutex<HashMap<u64, Entry>>,
}

/// One for the whole daemon.
pub static ACTIVITY: LazyLock<Activities> = LazyLock::new(Activities::default);

/// A running transfer; it disappears from the list when dropped.
pub struct Handle {
    id: u64,
    done: Arc<AtomicU64>,
}

impl Handle {
    pub fn add(&self, n: u64) {
        self.done.fetch_add(n, Ordering::Relaxed);
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        ACTIVITY.lock().remove(&self.id);
    }
}

#[derive(Debug, Serialize, PartialEq)]
pub struct Row {
    pub device: String,
    pub name: String,
    /// true: computer to phone.
    pub up: bool,
    pub total: u64,
    pub done: u64,
    pub elapsed_ms: u64,
}

impl Activities {
    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<u64, Entry>> {
        self.live.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Starts tracking; `done` is where it begins (resumed transfers start part-way).
    pub fn start(&self, device: &str, name: &str, up: bool, total: u64, done: u64) -> Handle {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let counter = Arc::new(AtomicU64::new(done));
        self.lock().insert(
            id,
            Entry {
                device: device.to_owned(),
                name: name.to_owned(),
                up,
                total,
                done: counter.clone(),
                started: Instant::now(),
            },
        );
        Handle { id, done: counter }
    }

    pub fn list(&self) -> Vec<Row> {
        let mut rows: Vec<(u64, Row)> = self
            .lock()
            .iter()
            .map(|(id, e)| {
                (
                    *id,
                    Row {
                        device: e.device.clone(),
                        name: e.name.clone(),
                        up: e.up,
                        total: e.total,
                        done: e.done.load(Ordering::Relaxed).min(e.total),
                        elapsed_ms: e.started.elapsed().as_millis() as u64,
                    },
                )
            })
            .collect();
        rows.sort_by_key(|(id, _)| *id);
        rows.into_iter().map(|(_, r)| r).collect()
    }
}

/// "photo.jpg", or "photo.jpg and 3 more".
pub fn label(names: &[String]) -> String {
    match names {
        [] => String::new(),
        [one] => one.clone(),
        [first, rest @ ..] => format!("{first} and {} more", rest.len()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracks_until_dropped() {
        let h = ACTIVITY.start("d1", "a.bin", true, 100, 10);
        h.add(40);
        let row = ACTIVITY
            .list()
            .into_iter()
            .find(|r| r.device == "d1")
            .unwrap();
        assert_eq!((row.done, row.total, row.up), (50, 100, true));
        drop(h);
        assert!(ACTIVITY.list().iter().all(|r| r.device != "d1"));
    }

    #[test]
    fn labels() {
        assert_eq!(label(&["a".into()]), "a");
        assert_eq!(label(&["a".into(), "b".into(), "c".into()]), "a and 2 more");
    }
}
