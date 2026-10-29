//! End-to-end tests: a fake phone talks to a real daemon over TLS on loopback.

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use continuum_proto::frame::{FrameDecoder, encode};
use continuum_proto::identity::Identity;
use continuum_proto::message::{
    self as m, BatteryStatus, DeviceType, Hello, Message, PairAccept, PairRequest,
};
use continuum_proto::pairing::{PairUri, proof};
use continuum_proto::tls;
use continuumd::state::Daemon;
use continuumd::store::Store;
use rustls::pki_types::ServerName;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::TlsConnector;
use tokio_rustls::client::TlsStream;

struct Desktop {
    daemon: Arc<Daemon>,
    port: u16,
    _dir: tempfile::TempDir,
}

async fn desktop() -> Desktop {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().to_path_buf()).unwrap();
    let identity = store.identity().unwrap();
    let daemon = Daemon::new(store, identity, "Test Desktop".into(), DeviceType::Desktop);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    daemon.port.store(port, Ordering::Relaxed);
    tokio::spawn(continuumd::server::serve(daemon.clone(), listener));
    Desktop {
        daemon,
        port,
        _dir: dir,
    }
}

struct Phone {
    stream: TlsStream<TcpStream>,
    dec: FrameDecoder,
}

impl Phone {
    async fn connect(id: &Identity, d: &Desktop) -> Self {
        let cfg = tls::client_config(id, d.daemon.identity.fingerprint()).unwrap();
        let tcp = TcpStream::connect(("127.0.0.1", d.port)).await.unwrap();
        let stream = TlsConnector::from(Arc::new(cfg))
            .connect(ServerName::try_from("continuum.invalid").unwrap(), tcp)
            .await
            .unwrap();
        Self {
            stream,
            dec: FrameDecoder::new(),
        }
    }

    async fn send(&mut self, msg: Message) {
        self.stream.write_all(&encode(&msg).unwrap()).await.unwrap();
    }

    /// Next message, or `None` if the daemon closed the connection.
    async fn recv(&mut self) -> Option<Message> {
        let mut buf = [0u8; 4096];
        loop {
            if let Some(msg) = self.dec.next_message().unwrap() {
                return Some(msg);
            }
            let n = tokio::time::timeout(Duration::from_secs(5), self.stream.read(&mut buf))
                .await
                .expect("recv timed out")
                .ok()?;
            if n == 0 {
                return None;
            }
            self.dec.push(&buf[..n]);
        }
    }

    async fn pair(&mut self, id: &Identity, uri: &PairUri) -> Message {
        let req = PairRequest {
            name: "Pixel 8".into(),
            device_type: DeviceType::Phone,
            proof: hex::encode(proof(&uri.secret, &uri.fp, &id.fingerprint())),
        };
        self.send(Message::with_body(m::PAIR_REQUEST, &req).unwrap())
            .await;
        self.recv().await.expect("reply to pair.request")
    }

    async fn hello(&mut self, id: &Identity) -> Hello {
        let h = Hello {
            v: 1,
            id: id.device_id(),
            name: "Pixel 8".into(),
            device_type: DeviceType::Phone,
            caps: vec!["battery".into()],
        };
        self.send(Message::with_body(m::HELLO, &h).unwrap()).await;
        let reply = self.recv().await.expect("hello reply");
        assert_eq!(reply.kind, m::HELLO);
        reply.body().unwrap()
    }
}

async fn eventually(mut check: impl FnMut() -> bool) {
    for _ in 0..100 {
        if check() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("condition not reached");
}

#[tokio::test]
async fn pair_report_battery_then_reconnect_without_pairing() {
    let d = desktop().await;
    let phone_id = Identity::generate().unwrap();
    let uri = PairUri::parse(&d.daemon.start_pairing(vec!["127.0.0.1".into()]).to_uri()).unwrap();
    assert_eq!(uri.port, d.port);

    let mut p = Phone::connect(&phone_id, &d).await;
    let accept = p.pair(&phone_id, &uri).await;
    assert_eq!(accept.kind, m::PAIR_ACCEPT);
    let accept: PairAccept = accept.body().unwrap();
    assert_eq!(accept.id, d.daemon.identity.device_id());
    assert_eq!(p.hello(&phone_id).await.name, "Test Desktop");

    p.send(
        Message::with_body(
            m::BATTERY_STATUS,
            &BatteryStatus {
                level: 77,
                charging: true,
            },
        )
        .unwrap(),
    )
    .await;
    eventually(|| {
        d.daemon
            .list()
            .first()
            .is_some_and(|s| s.connected && s.battery.as_ref().is_some_and(|b| b.level == 77))
    })
    .await;
    p.send(Message::new(m::PING)).await;
    assert_eq!(p.recv().await.unwrap().kind, m::PONG);
    assert_eq!(d.daemon.list()[0].device.name, "Pixel 8");

    drop(p);
    eventually(|| !d.daemon.list()[0].connected).await;

    // Paired now: hello works straight away, and pairing data persisted.
    let mut p = Phone::connect(&phone_id, &d).await;
    p.hello(&phone_id).await;
    eventually(|| d.daemon.list()[0].connected).await;
}

#[tokio::test]
async fn secret_is_single_use() {
    let d = desktop().await;
    let uri = d.daemon.start_pairing(vec![]);
    let first = Identity::generate().unwrap();
    assert_eq!(
        Phone::connect(&first, &d)
            .await
            .pair(&first, &uri)
            .await
            .kind,
        m::PAIR_ACCEPT
    );
    let second = Identity::generate().unwrap();
    assert_eq!(
        Phone::connect(&second, &d)
            .await
            .pair(&second, &uri)
            .await
            .kind,
        m::PAIR_REJECT
    );
    assert_eq!(d.daemon.list().len(), 1);
}

#[tokio::test]
async fn unknown_device_cannot_skip_pairing() {
    let d = desktop().await;
    d.daemon.start_pairing(vec![]);
    let stranger = Identity::generate().unwrap();
    let mut p = Phone::connect(&stranger, &d).await;
    let h = Hello {
        v: 1,
        id: stranger.device_id(),
        name: "x".into(),
        device_type: DeviceType::Phone,
        caps: vec![],
    };
    p.send(Message::with_body(m::HELLO, &h).unwrap()).await;
    assert_eq!(p.recv().await.unwrap().kind, m::PAIR_REJECT);
    assert!(p.recv().await.is_none(), "connection must be closed");
    assert!(d.daemon.list().is_empty());
}

#[tokio::test]
async fn wrong_proofs_are_rejected_then_rate_limited() {
    let d = desktop().await;
    let uri = d.daemon.start_pairing(vec![]);
    let attacker = Identity::generate().unwrap();
    let mut guess = uri.clone();
    guess.secret = [7; 16];
    for _ in 0..5 {
        let reply = Phone::connect(&attacker, &d)
            .await
            .pair(&attacker, &guess)
            .await;
        assert_eq!(reply.kind, m::PAIR_REJECT);
    }
    // Even the right secret is refused from this address for a minute.
    let reply = Phone::connect(&attacker, &d)
        .await
        .pair(&attacker, &uri)
        .await;
    assert_eq!(reply.kind, m::PAIR_REJECT);
    assert!(reply.body.to_string().contains("too many"));
    assert!(d.daemon.list().is_empty());
}

#[tokio::test]
async fn pairing_closed_means_reject() {
    let d = desktop().await;
    let uri = d.daemon.start_pairing(vec![]);
    d.daemon.cancel_pairing();
    let id = Identity::generate().unwrap();
    assert_eq!(
        Phone::connect(&id, &d).await.pair(&id, &uri).await.kind,
        m::PAIR_REJECT
    );
}

#[tokio::test]
async fn phone_can_unpair_and_unknown_types_get_an_error() {
    let d = desktop().await;
    let uri = d.daemon.start_pairing(vec![]);
    let id = Identity::generate().unwrap();
    let mut p = Phone::connect(&id, &d).await;
    p.pair(&id, &uri).await;
    p.hello(&id).await;
    p.send(Message::new("future.thing")).await;
    let err = p.recv().await.unwrap();
    assert_eq!(err.kind, m::ERROR);
    p.send(Message::new(m::UNPAIR)).await;
    assert!(p.recv().await.is_none());
    eventually(|| d.daemon.list().is_empty()).await;
}

#[tokio::test]
async fn oversized_frame_closes_connection() {
    let d = desktop().await;
    let uri = d.daemon.start_pairing(vec![]);
    let id = Identity::generate().unwrap();
    let mut p = Phone::connect(&id, &d).await;
    p.pair(&id, &uri).await;
    p.hello(&id).await;
    p.stream.write_all(&u32::MAX.to_be_bytes()).await.unwrap();
    assert!(p.recv().await.is_none());
    eventually(|| !d.daemon.list()[0].connected).await;
}

#[tokio::test]
async fn startup_nudge_reaches_disconnected_phone_at_last_address() {
    let d = desktop().await;
    let uri = d.daemon.start_pairing(vec![]);
    let id = Identity::generate().unwrap();
    let mut p = Phone::connect(&id, &d).await;
    p.pair(&id, &uri).await;
    p.hello(&id).await;
    eventually(|| d.daemon.list()[0].connected).await;
    assert!(
        d.daemon.wake_targets().is_empty(),
        "connected phones need no nudge"
    );
    assert_eq!(
        d.daemon.list()[0].device.last_ip.as_deref(),
        Some("127.0.0.1")
    );
    drop(p);
    eventually(|| !d.daemon.list()[0].connected).await;

    let phone_udp = tokio::net::UdpSocket::bind(("127.0.0.1", continuum_proto::DEFAULT_PORT))
        .await
        .unwrap();
    tokio::spawn(continuumd::wake::announce(d.daemon.clone()));
    let mut buf = [0u8; 128];
    let (n, _) = tokio::time::timeout(Duration::from_secs(3), phone_udp.recv_from(&mut buf))
        .await
        .unwrap()
        .unwrap();
    let expected = format!(
        "{}{}",
        continuumd::wake::PREFIX,
        d.daemon.identity.device_id()
    );
    assert_eq!(&buf[..n], expected.as_bytes());
}
