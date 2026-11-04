//! Phone mirroring: the phone streams its screen (H.264) on a data connection, and we hand the
//! bytes to the UI through one end of a socket pair, so the UI never touches TLS or tokens.

use std::collections::HashMap;
use std::os::fd::OwnedFd;
use std::os::unix::net::UnixStream as StdUnixStream;
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use continuum_proto::pairing::random_secret;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;
use tokio::time::timeout;

/// The phone has this long to connect after `mirror.start` (the user may have to allow it).
const PENDING_TTL: Duration = Duration::from_secs(120);

/// A live stream sends at least a frame every 100 ms (repeated frames on a still screen), so
/// silence this long means the phone is gone.
const STALL: Duration = Duration::from_secs(15);

struct Pending {
    token: String,
    sink: StdUnixStream,
    at: Instant,
}

/// Mirroring sessions waiting for the phone's data connection, at most one per device.
#[derive(Default)]
pub struct Mirrors {
    pending: Mutex<HashMap<String, Pending>>,
}

impl Mirrors {
    fn lock(&self) -> MutexGuard<'_, HashMap<String, Pending>> {
        self.pending.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Starts a session for `device`, replacing an older one that never connected. Returns the
    /// single-use token for the phone and the UI's end of the socket.
    pub fn open(&self, device: &str) -> std::io::Result<(String, OwnedFd)> {
        let (ours, theirs) = StdUnixStream::pair()?;
        let token = hex::encode(random_secret());
        let mut g = self.lock();
        g.retain(|_, p| p.at.elapsed() < PENDING_TTL);
        g.insert(
            device.to_owned(),
            Pending {
                token: token.clone(),
                sink: ours,
                at: Instant::now(),
            },
        );
        Ok((token, theirs.into()))
    }

    pub fn cancel(&self, device: &str) {
        self.lock().remove(device);
    }

    /// The UI's socket, if `token` is the one handed out for `device` (once, while fresh).
    pub fn take(&self, device: &str, token: &str) -> Option<StdUnixStream> {
        let mut g = self.lock();
        match g.get(device) {
            Some(p) if p.token == token && p.at.elapsed() < PENDING_TTL => {
                g.remove(device).map(|p| p.sink)
            }
            _ => None,
        }
    }
}

/// Copies the stream to the UI until the phone stops, the UI closes its end, or the phone goes
/// silent. Returns the bytes copied.
pub async fn pipe<R: AsyncRead + Unpin>(src: &mut R, sink: StdUnixStream) -> Result<u64, String> {
    sink.set_nonblocking(true).map_err(|e| e.to_string())?;
    let mut sink = UnixStream::from_std(sink).map_err(|e| e.to_string())?;
    let mut buf = vec![0u8; 256 * 1024];
    let mut total = 0u64;
    loop {
        let n = match timeout(STALL, src.read(&mut buf)).await {
            Ok(Ok(0)) => return Ok(total),
            Ok(Ok(n)) => n,
            Ok(Err(e)) => return Err(e.to_string()),
            Err(_) => return Err("the phone stopped sending".into()),
        };
        // The UI closed the window: stop reading, so the phone sees the connection close.
        if sink.write_all(&buf[..n]).await.is_err() {
            return Ok(total);
        }
        total += n as u64;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    #[test]
    fn token_is_single_use_and_per_device() {
        let m = Mirrors::default();
        let (token, _fd) = m.open("phone").unwrap();
        assert_eq!(token.len(), 32);
        assert!(m.take("other", &token).is_none());
        assert!(m.take("phone", "0".repeat(32).as_str()).is_none());
        assert!(m.take("phone", &token).is_some());
        assert!(m.take("phone", &token).is_none());
    }

    #[test]
    fn a_new_session_replaces_the_old_token() {
        let m = Mirrors::default();
        let (old, _a) = m.open("phone").unwrap();
        let (new, _b) = m.open("phone").unwrap();
        assert!(m.take("phone", &old).is_none());
        assert!(m.take("phone", &new).is_some());
        let (t, _c) = m.open("phone").unwrap();
        m.cancel("phone");
        assert!(m.take("phone", &t).is_none());
    }

    #[tokio::test]
    async fn pipe_copies_until_the_phone_closes() {
        let m = Mirrors::default();
        let (token, fd) = m.open("phone").unwrap();
        let sink = m.take("phone", &token).unwrap();
        let data: Vec<u8> = (0..700_000u32).map(|i| i as u8).collect();
        let reader = std::thread::spawn(move || {
            let mut ui = StdUnixStream::from(fd);
            let mut got = Vec::new();
            ui.read_to_end(&mut got).unwrap();
            got
        });
        let mut src = &data[..];
        assert_eq!(pipe(&mut src, sink).await.unwrap(), data.len() as u64);
        assert_eq!(reader.join().unwrap(), data);
    }

    #[tokio::test]
    async fn pipe_ends_when_the_ui_closes() {
        let m = Mirrors::default();
        let (token, fd) = m.open("phone").unwrap();
        let sink = m.take("phone", &token).unwrap();
        drop(fd);
        let (mut phone, mut src) = tokio::io::duplex(1024);
        let writer = tokio::spawn(async move {
            // Keeps sending; the copy must stop on its own.
            while phone.write_all(&[1u8; 512]).await.is_ok() {}
        });
        assert!(pipe(&mut src, sink).await.is_ok());
        drop(src);
        writer.await.unwrap();
    }
}
