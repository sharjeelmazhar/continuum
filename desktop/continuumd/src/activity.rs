//! Transfers in progress (sending, receiving, downloads from phone storage), for the app's
//! transfers button and progress notifications: name, direction, bytes done, how long it has
//! been running and its current speed.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;

struct Entry {
    device: String,
    name: String,
    up: bool,
    total: u64,
    done: Arc<AtomicU64>,
    started: Instant,
    speed: Speed,
}

/// The real speed right now: bytes over the last few seconds, measured against the clock.
/// The average since the start would hide stalls and include bytes resumed from an earlier try;
/// a single interval jumps with every burst of the network or the socket buffers.
#[derive(Debug)]
pub struct Speed {
    samples: VecDeque<(Instant, u64)>,
}

impl Speed {
    /// Speed over about this much time.
    pub const WINDOW: Duration = Duration::from_secs(3);
    /// No number before this much has been measured (the first burst only fills buffers).
    pub const MIN_SPAN: Duration = Duration::from_millis(900);
    const MIN_GAP: Duration = Duration::from_millis(100);

    pub fn new(at: Instant, done: u64) -> Self {
        Self {
            samples: VecDeque::from([(at, done)]),
        }
    }

    /// Records `done` bytes at `at` and returns bytes per second (0 until measurable or while
    /// nothing moves).
    pub fn sample(&mut self, at: Instant, done: u64) -> u64 {
        let Some(&(last_at, last_done)) = self.samples.back() else {
            self.samples.push_back((at, done));
            return 0;
        };
        if done < last_done {
            // Counted again from an earlier point (a retry): start measuring afresh.
            self.samples.clear();
            self.samples.push_back((at, done));
            return 0;
        }
        if at.saturating_duration_since(last_at) >= Self::MIN_GAP {
            self.samples.push_back((at, done));
        }
        // Keep one sample at or before the window's start, so the span covers the window.
        while self.samples.len() > 2
            && self.samples[1].0 <= at.checked_sub(Self::WINDOW).unwrap_or(self.samples[1].0)
        {
            self.samples.pop_front();
        }
        let (first_at, first_done) = self.samples[0];
        let span = at.saturating_duration_since(first_at);
        if span < Self::MIN_SPAN {
            return 0;
        }
        (done.saturating_sub(first_done) as f64 / span.as_secs_f64()) as u64
    }
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
    /// Bytes per second over the last few seconds; 0 while nothing moves.
    pub rate: u64,
}

impl Activities {
    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<u64, Entry>> {
        self.live.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Starts tracking; `done` is where it begins (resumed transfers start part-way).
    pub fn start(&self, device: &str, name: &str, up: bool, total: u64, done: u64) -> Handle {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let counter = Arc::new(AtomicU64::new(done));
        let now = Instant::now();
        self.lock().insert(
            id,
            Entry {
                device: device.to_owned(),
                name: name.to_owned(),
                up,
                total,
                done: counter.clone(),
                started: now,
                speed: Speed::new(now, done),
            },
        );
        Handle { id, done: counter }
    }

    pub fn list(&self) -> Vec<Row> {
        let now = Instant::now();
        let mut rows: Vec<(u64, Row)> = self
            .lock()
            .iter_mut()
            .map(|(id, e)| {
                let done = e.done.load(Ordering::Relaxed).min(e.total);
                (
                    *id,
                    Row {
                        device: e.device.clone(),
                        name: e.name.clone(),
                        up: e.up,
                        total: e.total,
                        done,
                        elapsed_ms: now.saturating_duration_since(e.started).as_millis() as u64,
                        rate: e.speed.sample(now, done),
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

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn speed_is_measured_against_the_clock() {
        let t = Instant::now();
        let mut s = Speed::new(t, 0);
        // Nothing until enough time has passed, even after a big first burst.
        assert_eq!(s.sample(t + ms(300), 4_000_000), 0);
        // Polled at uneven times: still 10 MB/s.
        let mut done = 4_000_000;
        for at in [1000, 1700, 2100, 3500, 4000, 5200, 6000] {
            done = 4_000_000 + (at - 300) * 10_000;
            let r = s.sample(t + ms(at), done);
            if at >= 3500 {
                assert!((9_900_000..=10_100_000).contains(&r), "{at}: {r}");
            }
        }
        // It slows down to 1 MB/s: the number follows within the window.
        for at in (6500..=10_000).step_by(500) {
            done += 500_000;
            s.sample(t + ms(at), done);
        }
        let r = s.sample(t + ms(10_000), done);
        assert!((900_000..=1_100_000).contains(&r), "{r}");
    }

    #[test]
    fn stalls_show_zero_and_resumed_bytes_do_not_count() {
        let t = Instant::now();
        // Resumed at 900 MB: only the new bytes count.
        let mut s = Speed::new(t, 900_000_000);
        assert_eq!(s.sample(t + ms(2000), 902_000_000), 1_000_000);
        // Nothing arrives for longer than the window.
        for at in [3000, 4000, 5000, 6000] {
            s.sample(t + ms(at), 902_000_000);
        }
        assert_eq!(s.sample(t + ms(6000), 902_000_000), 0);
        // Counted again from an earlier point: measured afresh, no negative or huge number.
        assert_eq!(s.sample(t + ms(6100), 100), 0);
        assert_eq!(s.sample(t + ms(7100), 2_000_100), 2_000_000);
    }

    #[test]
    fn rapid_polls_do_not_pile_up_samples() {
        let t = Instant::now();
        let mut s = Speed::new(t, 0);
        for i in 0..10_000u64 {
            s.sample(t + ms(i), i * 1000);
        }
        assert!(s.samples.len() <= 40, "{}", s.samples.len());
        assert_eq!(s.sample(t + ms(10_000), 10_000_000), 1_000_000);
    }

    #[test]
    fn labels() {
        assert_eq!(label(&["a".into()]), "a");
        assert_eq!(label(&["a".into(), "b".into(), "c".into()]), "a and 2 more");
    }
}
