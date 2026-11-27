//! Progress notifications against a fake notification server that behaves like GNOME Shell
//! (a replaced id that is gone opens a new notification with a new id), on a private D-Bus.
//! The user clearing a notification (one, "Clear all", many times) must bring back exactly one
//! notification while the transfer runs, with the real speed, and nothing once it ends.

use std::collections::{BTreeMap, HashMap};
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use continuum_proto::message::DeviceType;
use continuumd::activity::ACTIVITY;
use continuumd::notify::Notifier;
use continuumd::state::Daemon;
use continuumd::store::Store;
use continuumd::transfer::Progress;
use zbus::object_server::SignalEmitter;
use zbus::OwnedVariant as OwnedValue;
use zbus::{Connection, connection, interface};

const PATH: &str = "/org/freedesktop/Notifications";

#[derive(Clone, Debug)]
struct Note {
    summary: String,
    body: String,
    urgency: Option<u8>,
}

#[derive(Default)]
struct Shown {
    next: u32,
    notes: BTreeMap<u32, Note>,
    /// Notify calls that opened a new notification.
    opened: u32,
}

struct Fake(Arc<Mutex<Shown>>);

#[interface(name = "org.freedesktop.Notifications")]
impl Fake {
    #[allow(clippy::too_many_arguments)]
    fn notify(
        &self,
        _app_name: &str,
        replaces_id: u32,
        _app_icon: &str,
        summary: &str,
        body: &str,
        _actions: Vec<String>,
        hints: HashMap<String, OwnedValue>,
        _expire_timeout: i32,
    ) -> u32 {
        let mut s = self.0.lock().unwrap();
        let note = Note {
            summary: summary.into(),
            body: body.into(),
            urgency: hints.get("urgency").and_then(|v| u8::try_from(v).ok()),
        };
        // GNOME: replacing a notification that is still there keeps its id; otherwise new one.
        if replaces_id != 0 && s.notes.contains_key(&replaces_id) {
            s.notes.insert(replaces_id, note);
            return replaces_id;
        }
        s.next += 1;
        s.opened += 1;
        let id = s.next;
        s.notes.insert(id, note);
        id
    }

    async fn close_notification(
        &self,
        id: u32,
        #[zbus(signal_emitter)] e: SignalEmitter<'_>,
    ) -> zbus::fdo::Result<()> {
        let was = self.0.lock().unwrap().notes.remove(&id).is_some();
        if was {
            let _ = Self::notification_closed(&e, id, 3).await;
        }
        Ok(())
    }

    fn get_capabilities(&self) -> Vec<String> {
        vec!["body".into(), "actions".into()]
    }

    #[zbus(signal)]
    async fn notification_closed(e: &SignalEmitter<'_>, id: u32, reason: u32) -> zbus::Result<()>;
}

struct Bus {
    child: Child,
    address: String,
}

impl Drop for Bus {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// A private session bus, or None where dbus-daemon isn't installed.
fn private_bus() -> Option<Bus> {
    let mut child = Command::new("dbus-daemon")
        .args(["--session", "--nofork", "--print-address=1"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut line = String::new();
    BufReader::new(child.stdout.take()?).read_line(&mut line).ok()?;
    Some(Bus {
        child,
        address: line.trim().to_owned(),
    })
}

struct Server {
    shown: Arc<Mutex<Shown>>,
    conn: Connection,
}

impl Server {
    fn notes(&self) -> BTreeMap<u32, Note> {
        self.shown.lock().unwrap().notes.clone()
    }

    fn progress(&self) -> Vec<(u32, Note)> {
        self.notes()
            .into_iter()
            .filter(|(_, n)| n.body.contains('%'))
            .collect()
    }

    /// The user clears notifications in the date menu (one, or "Clear all" with None).
    async fn user_clears(&self, id: Option<u32>) {
        let ids: Vec<u32> = {
            let mut s = self.shown.lock().unwrap();
            let ids: Vec<u32> = match id {
                Some(id) => vec![id],
                None => s.notes.keys().copied().collect(),
            };
            for id in &ids {
                s.notes.remove(id);
            }
            ids
        };
        let e = SignalEmitter::new(&self.conn, PATH).unwrap();
        for id in ids {
            Fake::notification_closed(&e, id, 2).await.unwrap();
        }
    }
}

async fn eventually(what: &str, mut check: impl FnMut() -> bool) {
    let until = Instant::now() + Duration::from_secs(8);
    while Instant::now() < until {
        if check() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("timed out: {what}");
}

/// Adds bytes like a transfer at `per_sec` until stopped (in 50 ms steps).
fn feed(h: Arc<continuumd::activity::Handle>, per_sec: u64) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_millis(50));
        loop {
            tick.tick().await;
            h.add(per_sec / 20);
        }
    })
}

/// "… · 9.8 MB/s · …" → 9.8e6.
fn speed(body: &str) -> Option<f64> {
    let part = body.split(" · ").find(|p| p.ends_with("/s"))?;
    let (num, unit) = part.trim_end_matches("/s").split_once(' ')?;
    let mult = match unit {
        "bytes" => 1.0,
        "KB" => 1e3,
        "MB" => 1e6,
        "GB" => 1e9,
        _ => return None,
    };
    Some(num.parse::<f64>().ok()? * mult)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cleared_progress_comes_back_once_with_real_speed() {
    let Some(bus) = private_bus() else {
        eprintln!("dbus-daemon not installed: skipped");
        return;
    };
    let shown = Arc::new(Mutex::new(Shown::default()));
    let server_conn = connection::Builder::address(bus.address.as_str())
        .name("org.freedesktop.Notifications")
        .serve_at(PATH, Fake(shown.clone()))
        .build()
        .await
        .unwrap();
    let server = Server {
        shown,
        conn: server_conn,
    };

    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().to_path_buf()).unwrap();
    let identity = store.identity().unwrap();
    let daemon = Daemon::new(
        store,
        identity,
        "Test Desktop".into(),
        DeviceType::Desktop,
        dir.path().join("Downloads"),
    );
    let phone = daemon
        .complete_pairing(&[7u8; 32], "Pixel 8".into(), DeviceType::Phone)
        .unwrap();
    let client = connection::Builder::address(bus.address.as_str())
        .build()
        .await
        .unwrap();
    let notifier = Notifier::new(&client, daemon.clone()).await.unwrap();

    // 1. A download from the phone's storage (no share notification): one quiet notification.
    let h = Arc::new(ACTIVITY.start(&phone.id, "movie.mp4", false, 2_000_000_000, 0));
    let fed = feed(h.clone(), 10_000_000);
    eventually("progress shown", || server.progress().len() == 1).await;
    let (first, note) = server.progress()[0].clone();
    assert_eq!(note.summary, "Receiving movie.mp4 from Pixel 8");
    assert_eq!(note.urgency, Some(0), "progress must not pop up");

    // Real speed: 10 MB/s once measured (about two updates in).
    eventually("speed shown", || {
        server
            .progress()
            .first()
            .and_then(|(_, n)| speed(&n.body))
            .is_some_and(|s| (8e6..=12e6).contains(&s))
    })
    .await;
    assert!(server.progress()[0].1.body.contains(" left"));

    // 2. Cleared by the user: back within about a second, as one new notification.
    let mut last = first;
    for round in 0..3 {
        server.user_clears(Some(last)).await;
        eventually("progress back", || server.progress().len() == 1).await;
        let id = server.progress()[0].0;
        assert_ne!(id, last, "round {round}: must be a new notification");
        last = id;
    }
    // "Clear all" too.
    server.user_clears(None).await;
    eventually("progress back after Clear all", || {
        server.progress().len() == 1
    })
    .await;
    last = server.progress()[0].0;

    // Never more than one, update after update.
    let opened = server.shown.lock().unwrap().opened;
    tokio::time::sleep(Duration::from_millis(3500)).await;
    assert_eq!(server.progress().len(), 1);
    assert_eq!(server.progress()[0].0, last, "updates replace in place");
    assert_eq!(server.shown.lock().unwrap().opened, opened, "no new ones");

    // 3. It stalls: the speed goes away instead of showing an old number.
    fed.abort();
    let _ = fed.await;
    eventually("stall shown", || {
        server
            .progress()
            .first()
            .is_some_and(|(_, n)| speed(&n.body).is_none())
    })
    .await;

    // 4. It ends: the notification goes away and doesn't come back.
    drop(Arc::into_inner(h).unwrap());
    eventually("progress closed", || server.progress().is_empty()).await;
    tokio::time::sleep(Duration::from_millis(2500)).await;
    assert!(server.notes().is_empty(), "{:?}", server.notes());

    // 5. A share from the phone: its "Receiving…" notification carries the progress, comes
    //    back when cleared, and becomes "Received …" at the end (one notification throughout).
    notifier
        .on_transfer(&Progress::Started {
            device: "Pixel 8".into(),
            files: 1,
            bytes: 50_000_000,
        })
        .await;
    let popup = *server.notes().keys().next().unwrap();
    let h = Arc::new(ACTIVITY.start(&phone.id, "photo.jpg", false, 50_000_000, 0));
    let fed = feed(h.clone(), 5_000_000);
    eventually("share shows progress", || {
        server.notes().get(&popup).is_some_and(|n| n.body.contains('%'))
    })
    .await;
    server.user_clears(Some(popup)).await;
    eventually("share progress back", || {
        let n = server.notes();
        n.len() == 1 && !n.contains_key(&popup) && n.values().all(|n| n.body.contains('%'))
    })
    .await;
    fed.abort();
    let _ = fed.await;
    drop(Arc::into_inner(h).unwrap());
    let saved = dir.path().join("Downloads/photo.jpg");
    notifier
        .on_transfer(&Progress::Finished {
            device: "Pixel 8".into(),
            saved: vec![saved],
        })
        .await;
    eventually("received shown", || {
        let n = server.notes();
        n.len() == 1 && n.values().all(|n| n.summary == "Received photo.jpg")
    })
    .await;
    tokio::time::sleep(Duration::from_millis(2500)).await;
    assert_eq!(server.notes().len(), 1, "{:?}", server.notes());

    // 6. Two transfers at once: one notification each, cleared together, both come back.
    server.user_clears(None).await;
    let a = Arc::new(ACTIVITY.start(&phone.id, "a.bin", true, 1_000_000_000, 0));
    let b = Arc::new(ACTIVITY.start(&phone.id, "b.bin", false, 1_000_000_000, 0));
    let (fa, fb) = (feed(a.clone(), 20_000_000), feed(b.clone(), 2_000_000));
    eventually("two shown", || server.progress().len() == 2).await;
    server.user_clears(None).await;
    eventually("two back", || server.progress().len() == 2).await;
    eventually("each its own speed", || {
        let p = server.progress();
        let s = |name: &str| {
            p.iter()
                .find(|(_, n)| n.summary.contains(name))
                .and_then(|(_, n)| speed(&n.body))
        };
        s("a.bin").is_some_and(|v| (16e6..=24e6).contains(&v))
            && s("b.bin").is_some_and(|v| (1.6e6..=2.4e6).contains(&v))
    })
    .await;
    let sending = server
        .progress()
        .into_iter()
        .find(|(_, n)| n.summary.contains("a.bin"))
        .unwrap()
        .1
        .summary;
    assert_eq!(sending, "Sending a.bin to Pixel 8");
    fa.abort();
    fb.abort();
    let _ = (fa.await, fb.await);
    drop((Arc::into_inner(a).unwrap(), Arc::into_inner(b).unwrap()));
    eventually("both closed", || server.progress().is_empty()).await;
    drop(bus);
}
