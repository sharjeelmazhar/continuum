//! TLS listener and per-connection protocol handling (see `docs/PROTOCOL.md`).

use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use continuum_proto::frame::{FrameDecoder, encode};
use continuum_proto::identity::{Fingerprint, fingerprint};
use continuum_proto::message::{
    self as m, BatteryStatus, DataHello, ErrorBody, Hello, Message, PairAccept, PairReject,
    PairRequest, ShareOffer, ShareReject,
};
use continuum_proto::pairing::verify_proof;
use continuum_proto::{PROTOCOL_VERSION, tls};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Notify, mpsc};
use tokio::time::timeout;
use tokio_rustls::TlsAcceptor;
use tracing::{debug, info, warn};

use crate::state::{Daemon, Event, Live};
use crate::store::PairedDevice;

const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);
const PAIR_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
/// No frame for this long (the phone pings every 20 s) means the connection is dead.
pub const IDLE_TIMEOUT: Duration = Duration::from_secs(60);

/// Accepts connections forever.
pub async fn serve(daemon: Arc<Daemon>, listener: TcpListener) -> std::io::Result<()> {
    let config = tls::server_config(&daemon.identity).map_err(std::io::Error::other)?;
    let acceptor = TlsAcceptor::from(Arc::new(config));
    loop {
        let (tcp, addr) = listener.accept().await?;
        let (daemon, acceptor) = (daemon.clone(), acceptor.clone());
        tokio::spawn(async move {
            if let Err(e) = handle(daemon, acceptor, tcp, addr).await {
                debug!("connection from {addr} ended: {e}");
            }
        });
    }
}

struct Reader<R> {
    rd: R,
    dec: FrameDecoder,
    buf: Vec<u8>,
}

impl<R: AsyncRead + Unpin> Reader<R> {
    async fn next(&mut self, wait: Duration) -> Result<Message, String> {
        loop {
            if let Some(msg) = self.dec.next_message().map_err(|e| e.to_string())? {
                return Ok(msg);
            }
            let n = timeout(wait, self.rd.read(&mut self.buf))
                .await
                .map_err(|_| "timed out".to_owned())?
                .map_err(|e| e.to_string())?;
            if n == 0 {
                return Err("closed by peer".into());
            }
            self.dec.push(&self.buf[..n]);
        }
    }
}

async fn write_loop<W: AsyncWrite + Unpin>(mut wr: W, mut rx: mpsc::Receiver<Message>) {
    while let Some(msg) = rx.recv().await {
        let Ok(bytes) = encode(&msg) else { continue };
        if wr.write_all(&bytes).await.is_err() {
            return;
        }
    }
    let _ = wr.shutdown().await;
}

async fn handle(
    daemon: Arc<Daemon>,
    acceptor: TlsAcceptor,
    tcp: TcpStream,
    addr: SocketAddr,
) -> Result<(), String> {
    let _ = tcp.set_nodelay(true);
    let stream = timeout(HANDSHAKE_TIMEOUT, acceptor.accept(tcp))
        .await
        .map_err(|_| "TLS handshake timed out".to_owned())?
        .map_err(|e| format!("TLS handshake failed: {e}"))?;
    let fp = match stream.get_ref().1.peer_certificates() {
        Some([cert, ..]) => fingerprint(cert),
        _ => return Err("no client certificate".into()),
    };
    let (rd, wr) = tokio::io::split(stream);
    let mut reader = Reader {
        rd,
        dec: FrameDecoder::new(),
        buf: vec![0; 16 * 1024],
    };
    let (tx, rx) = mpsc::channel(64);
    tokio::spawn(write_loop(wr, rx));

    let device = match daemon.paired_by_fp(&fp) {
        Some(d) => d,
        None => pair(&daemon, &mut reader, &tx, fp, addr.ip()).await?,
    };

    let first = reader.next(HANDSHAKE_TIMEOUT).await?;
    if first.kind == m::DATA_HELLO {
        // A data connection for a file transfer: raw bytes follow the frame.
        let token = first.body::<DataHello>().map_err(|e| e.to_string())?.token;
        let leftover = std::io::Cursor::new(reader.dec.take_remaining());
        let mut stream = leftover.chain(reader.rd);
        let events = daemon.clone();
        let result = daemon
            .transfers
            .receive(&device.id, &token, &mut stream, |p| {
                events.emit(Event::Transfer(p))
            })
            .await;
        match &result {
            Ok(()) => info!("received files from {}", device.name),
            Err(e) => warn!("transfer from {} failed: {e}", device.name),
        }
        // Closing our side (close_notify) tells the sender everything was read.
        drop(tx);
        return result;
    }
    if first.kind != m::HELLO {
        return Err(format!("expected hello, got {}", first.kind));
    }
    let hello: Hello = first.body().map_err(|e| e.to_string())?;
    let ours = Hello {
        v: PROTOCOL_VERSION,
        id: daemon.identity.device_id(),
        name: daemon.name.clone(),
        device_type: daemon.device_type,
        caps: m::CAPS.iter().map(|c| (*c).to_owned()).collect(),
    };
    send(&tx, Message::with_body(m::HELLO, &ours)).await?;

    let conn = daemon.next_conn_id();
    let kill = Arc::new(Notify::new());
    daemon.connected(
        &device.id,
        Live {
            conn,
            battery: None,
            tx: tx.clone(),
            kill: kill.clone(),
        },
        addr.ip(),
    );
    info!(
        "{} connected from {} (protocol v{})",
        device.name,
        addr.ip(),
        hello.v
    );

    let result = session(&daemon, &device, &mut reader, &tx, &kill).await;
    daemon.disconnected(&device.id, conn);
    info!("{} disconnected", device.name);
    result
}

async fn pair<R: AsyncRead + Unpin>(
    daemon: &Daemon,
    reader: &mut Reader<R>,
    tx: &mpsc::Sender<Message>,
    fp: Fingerprint,
    ip: IpAddr,
) -> Result<PairedDevice, String> {
    let msg = reader.next(PAIR_REQUEST_TIMEOUT).await?;
    let reject = |reason: &str| {
        Message::with_body(
            m::PAIR_REJECT,
            &PairReject {
                reason: reason.to_owned(),
            },
        )
    };
    if msg.kind == m::HELLO {
        // A phone that still thinks it's paired (we removed it): tell it, so it can forget us.
        send(tx, reject("this phone is not paired with this computer")).await?;
        return Err("unpaired device sent hello".into());
    }
    if msg.kind != m::PAIR_REQUEST {
        return Err(format!("unpaired device sent {}", msg.kind));
    }
    let req: PairRequest = msg.body().map_err(|e| e.to_string())?;
    let secret = match daemon.pairing_secret(ip) {
        Ok(s) => s,
        Err(reason) => {
            send(tx, reject(reason)).await?;
            return Err(reason.into());
        }
    };
    if !verify_proof(&secret, &daemon.identity.fingerprint(), &fp, &req.proof) {
        daemon.record_pair_failure(ip);
        warn!("pairing attempt from {ip} with a wrong proof");
        send(
            tx,
            reject("this QR code doesn't belong to this computer; scan it again"),
        )
        .await?;
        return Err("wrong pairing proof".into());
    }
    let dev = daemon
        .complete_pairing(&fp, m::clean_name(&req.name), req.device_type)
        .map_err(|e| format!("could not save pairing: {e}"))?;
    let accept = PairAccept {
        id: daemon.identity.device_id(),
        name: daemon.name.clone(),
        device_type: daemon.device_type,
    };
    send(tx, Message::with_body(m::PAIR_ACCEPT, &accept)).await?;
    info!("paired with {} ({})", dev.name, dev.id);
    Ok(dev)
}

async fn session<R: AsyncRead + Unpin>(
    daemon: &Daemon,
    device: &PairedDevice,
    reader: &mut Reader<R>,
    tx: &mpsc::Sender<Message>,
    kill: &Notify,
) -> Result<(), String> {
    loop {
        let msg = tokio::select! {
            msg = reader.next(IDLE_TIMEOUT) => msg?,
            _ = kill.notified() => return Ok(()),
        };
        match msg.kind.as_str() {
            m::PING => send(tx, Ok(Message::new(m::PONG))).await?,
            m::PONG | m::ERROR => {}
            m::BATTERY_STATUS => match msg.body::<BatteryStatus>() {
                Ok(b) if b.level <= 100 => {
                    info!(
                        "{} battery {}%{}",
                        device.name,
                        b.level,
                        if b.charging { " (charging)" } else { "" }
                    );
                    daemon.set_battery(&device.id, b);
                }
                _ => warn!("{} sent an invalid battery.status", device.name),
            },
            m::SHARE_OFFER => {
                let reply = match msg.body::<ShareOffer>() {
                    Ok(offer) => {
                        let id = offer.transfer_id.clone();
                        info!("{} offers {} file(s)", device.name, offer.files.len());
                        match daemon
                            .transfers
                            .prepare(&device.id, &device.name, offer, tx.clone())
                        {
                            Ok(accept) => Message::with_body(m::SHARE_ACCEPT, &accept),
                            Err(reason) => Message::with_body(
                                m::SHARE_REJECT,
                                &ShareReject {
                                    transfer_id: id,
                                    reason,
                                },
                            ),
                        }
                    }
                    Err(e) => Message::with_body(
                        m::ERROR,
                        &ErrorBody {
                            code: "bad_offer".into(),
                            msg: e.to_string(),
                        },
                    ),
                };
                send(tx, reply).await?;
            }
            m::UNPAIR => {
                info!("{} removed the pairing", device.name);
                daemon.unpair(&device.id).map_err(|e| e.to_string())?;
                return Ok(());
            }
            other => {
                let body = ErrorBody {
                    code: "unsupported".into(),
                    msg: format!("unknown type {other}"),
                };
                send(tx, Message::with_body(m::ERROR, &body)).await?;
            }
        }
    }
}

async fn send(
    tx: &mpsc::Sender<Message>,
    msg: Result<Message, continuum_proto::ProtoError>,
) -> Result<(), String> {
    let msg = msg.map_err(|e| e.to_string())?;
    tx.send(msg)
        .await
        .map_err(|_| "connection closed".to_owned())
}
