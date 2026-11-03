//! Keepalive pacing: fast pings while the phone is in use or something is running, slow pings
//! otherwise, so an idle phone's Wi-Fi can stay in power save (docs/PROTOCOL.md, "Session").

use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::time::{Duration, Instant};

use tokio::sync::Notify;

/// Fast pace: a phone that drops off Wi-Fi shows as gone within seconds.
pub const FAST_PING: Duration = Duration::from_secs(3);
pub const FAST_TIMEOUT: Duration = Duration::from_secs(9);
/// Slow pace, while the phone says nobody is using it and nothing runs.
pub const SLOW_PING: Duration = Duration::from_secs(30);
pub const SLOW_TIMEOUT: Duration = Duration::from_secs(75);
/// When the user acts on the desktop at the slow pace, the phone must answer an immediate ping
/// this quickly (a dead phone is still noticed at once).
pub const PROBE_TIMEOUT: Duration = Duration::from_secs(4);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pace {
    Fast,
    Slow,
}

impl Pace {
    /// `presence`: what the phone last said (`None`: it never did, e.g. an older app).
    /// `busy`: a request, download or transfer to or from it is running.
    pub fn choose(presence: Option<bool>, busy: bool) -> Self {
        if busy || presence != Some(false) {
            Self::Fast
        } else {
            Self::Slow
        }
    }

    pub fn ping_every(self) -> Duration {
        match self {
            Self::Fast => FAST_PING,
            Self::Slow => SLOW_PING,
        }
    }

    pub fn timeout(self) -> Duration {
        match self {
            Self::Fast => FAST_TIMEOUT,
            Self::Slow => SLOW_TIMEOUT,
        }
    }
}

/// When a session must give up: no frame for the pace's timeout, or no answer to a probe.
pub fn deadline(last_rx: Instant, pace: Pace, probe_until: Option<Instant>) -> Instant {
    let idle = last_rx + pace.timeout();
    probe_until.map_or(idle, |p| p.min(idle))
}

/// Shared by a session and the daemon (which pokes it when the user acts).
#[derive(Default)]
pub struct Keepalive {
    /// 0: never said, 1: active, 2: idle.
    presence: AtomicU8,
    probe: AtomicBool,
    /// Wakes the session to re-check its pace.
    pub wake: Notify,
}

impl Keepalive {
    pub fn set_presence(&self, active: bool) {
        self.presence
            .store(if active { 1 } else { 2 }, Ordering::Relaxed);
        self.wake.notify_one();
    }

    pub fn presence(&self) -> Option<bool> {
        match self.presence.load(Ordering::Relaxed) {
            1 => Some(true),
            2 => Some(false),
            _ => None,
        }
    }

    /// The desktop user did something with this phone: check it's still there.
    pub fn poke(&self) {
        self.probe.store(true, Ordering::Relaxed);
        self.wake.notify_one();
    }

    pub fn take_probe(&self) -> bool {
        self.probe.swap(false, Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pace_follows_presence_and_work() {
        assert_eq!(Pace::choose(None, false), Pace::Fast, "older phones");
        assert_eq!(Pace::choose(Some(true), false), Pace::Fast);
        assert_eq!(Pace::choose(Some(false), false), Pace::Slow);
        assert_eq!(Pace::choose(Some(false), true), Pace::Fast);
        assert!(Pace::Slow.ping_every() < Pace::Slow.timeout());
        assert!(Pace::Fast.ping_every() * 3 <= Pace::Fast.timeout());
    }

    #[test]
    fn deadline_takes_the_probe_into_account() {
        let t = Instant::now();
        assert_eq!(deadline(t, Pace::Slow, None), t + SLOW_TIMEOUT);
        assert_eq!(deadline(t, Pace::Fast, None), t + FAST_TIMEOUT);
        let p = t + PROBE_TIMEOUT;
        assert_eq!(deadline(t, Pace::Slow, Some(p)), p);
        // A probe can't extend the idle limit.
        assert_eq!(
            deadline(t, Pace::Fast, Some(t + Duration::from_secs(60))),
            t + FAST_TIMEOUT
        );
    }

    #[test]
    fn keepalive_state() {
        let k = Keepalive::default();
        assert_eq!(k.presence(), None);
        k.set_presence(false);
        assert_eq!(k.presence(), Some(false));
        k.set_presence(true);
        assert_eq!(k.presence(), Some(true));
        assert!(!k.take_probe());
        k.poke();
        assert!(k.take_probe());
        assert!(!k.take_probe());
    }
}
