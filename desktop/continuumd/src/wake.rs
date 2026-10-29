//! "I'm back" nudge. When the daemon starts (PC switched on, service restarted), paired phones
//! may be sleeping between reconnect attempts. A small UDP packet to each phone's last address
//! makes it reconnect at once. Best effort, and it carries nothing secret: the phone still only
//! connects to its pinned computer, so a spoofed nudge can at most cause one extra attempt.

use std::sync::Arc;
use std::time::Duration;

use continuum_proto::DEFAULT_PORT;
use tokio::net::UdpSocket;

use crate::state::Daemon;

pub const PREFIX: &str = "continuum-wake 1 ";

/// Seconds after startup to send nudges; repeated because the network may still be coming up.
const SCHEDULE: [u64; 6] = [0, 2, 3, 5, 10, 20];

pub async fn announce(daemon: Arc<Daemon>) {
    let msg = format!("{PREFIX}{}", daemon.identity.device_id());
    for wait in SCHEDULE {
        tokio::time::sleep(Duration::from_secs(wait)).await;
        let targets = daemon.wake_targets();
        if targets.is_empty() {
            return;
        }
        let Ok(sock) = UdpSocket::bind(("0.0.0.0", 0)).await else {
            continue;
        };
        for ip in targets {
            let _ = sock.send_to(msg.as_bytes(), (ip, DEFAULT_PORT)).await;
        }
    }
}
