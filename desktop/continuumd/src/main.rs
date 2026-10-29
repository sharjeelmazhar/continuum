//! Continuum daemon (headless). UIs talk to it over D-Bus.

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::atomic::Ordering;

use continuum_proto::DEFAULT_PORT;
use continuumd::state::Daemon;
use continuumd::store::Store;
use continuumd::{clipboard, dbus, mdns, server, sysinfo, wake};
use tokio::net::TcpListener;
use tokio::signal::unix::{SignalKind, signal};
use tracing::{error, info, warn};

const USAGE: &str = "usage: continuumd [--data-dir DIR] [--port N] [--no-dbus] [--no-mdns] [--print-pair-uri] [--download-dir DIR]\n       continuumd send [--to DEVICE-ID] FILE...";

struct Args {
    data_dir: PathBuf,
    port: u16,
    dbus: bool,
    mdns: bool,
    print_pair_uri: bool,
    download_dir: PathBuf,
}

fn parse_args() -> Result<Args, String> {
    let mut a = Args {
        data_dir: Store::default_dir(),
        port: DEFAULT_PORT,
        dbus: true,
        mdns: true,
        print_pair_uri: false,
        download_dir: sysinfo::download_dir().join("Continuum"),
    };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--data-dir" => a.data_dir = it.next().ok_or(USAGE)?.into(),
            "--port" => a.port = it.next().and_then(|p| p.parse().ok()).ok_or(USAGE)?,
            "--no-dbus" => a.dbus = false,
            "--no-mdns" => a.mdns = false,
            "--print-pair-uri" => a.print_pair_uri = true,
            "--download-dir" => a.download_dir = it.next().ok_or(USAGE)?.into(),
            "--version" => {
                println!(
                    "continuumd {} (protocol v{})",
                    env!("CARGO_PKG_VERSION"),
                    continuum_proto::PROTOCOL_VERSION
                );
                std::process::exit(0);
            }
            _ => return Err(USAGE.into()),
        }
    }
    Ok(a)
}

#[tokio::main]
async fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .with_target(false)
        .init();
    let argv: Vec<String> = std::env::args().skip(1).collect();
    if argv.first().map(String::as_str) == Some("send") {
        return match send_cli(&argv[1..]).await {
            Ok(msg) => {
                println!("{msg}");
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("continuumd send: {e}");
                ExitCode::FAILURE
            }
        };
    }
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(2);
        }
    };
    match run(args).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            error!("{e}");
            ExitCode::FAILURE
        }
    }
}

/// `continuumd send`: asks the running daemon to send files (used by file manager menus).
async fn send_cli(argv: &[String]) -> Result<String, String> {
    let mut to = None;
    let mut paths = Vec::new();
    let mut it = argv.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--to" => to = Some(it.next().ok_or(USAGE)?.clone()),
            p => paths.push(
                std::fs::canonicalize(p)
                    .map_err(|e| format!("{p}: {e}"))?
                    .to_string_lossy()
                    .into_owned(),
            ),
        }
    }
    if paths.is_empty() {
        return Err(USAGE.into());
    }
    let conn = zbus::Connection::session()
        .await
        .map_err(|e| e.to_string())?;
    let call = |method: &'static str, body: Option<(String, Vec<String>)>| {
        let conn = conn.clone();
        async move {
            let reply = match body {
                Some(b) => {
                    conn.call_method(
                        Some(dbus::BUS_NAME),
                        dbus::PATH,
                        Some(dbus::IFACE),
                        method,
                        &b,
                    )
                    .await
                }
                None => {
                    conn.call_method(
                        Some(dbus::BUS_NAME),
                        dbus::PATH,
                        Some(dbus::IFACE),
                        method,
                        &(),
                    )
                    .await
                }
            };
            reply.map_err(|e| match e {
                zbus::Error::MethodError(_, Some(msg), _) => msg,
                e => e.to_string(),
            })
        }
    };
    let id = match to {
        Some(id) => id,
        None => {
            let rows: Vec<dbus::DeviceRow> = call("ListDevices", None)
                .await?
                .body()
                .deserialize()
                .map_err(|e| e.to_string())?;
            // The only paired device, or the only connected one.
            let pick = match rows.as_slice() {
                [one] => Some(one),
                many => {
                    let live: Vec<_> = many.iter().filter(|r| r.3).collect();
                    (live.len() == 1).then(|| live[0])
                }
            };
            pick.ok_or("pass --to DEVICE-ID (several or no phones paired)")?
                .0
                .clone()
        }
    };
    call("SendFiles", Some((id, paths))).await?;
    Ok("sending".into())
}

async fn run(args: Args) -> Result<(), String> {
    let store = Store::open(args.data_dir.clone())
        .map_err(|e| format!("cannot open {}: {e}", args.data_dir.display()))?;
    let identity = store
        .identity()
        .map_err(|e| format!("cannot load identity: {e}"))?;
    let daemon = Daemon::new(
        store,
        identity,
        sysinfo::device_name(),
        sysinfo::device_type(),
        args.download_dir.clone(),
    );

    // The D-Bus name doubles as a single-instance lock, so claim it before the port.
    let _bus = if args.dbus {
        Some(
            dbus::run(daemon.clone())
                .await
                .map_err(|e| format!("D-Bus: {e} (is continuumd already running?)"))?,
        )
    } else {
        None
    };

    let listener = match TcpListener::bind(("0.0.0.0", args.port)).await {
        Ok(l) => l,
        Err(e) => {
            warn!("port {} unavailable ({e}); using a free port", args.port);
            TcpListener::bind(("0.0.0.0", 0))
                .await
                .map_err(|e| e.to_string())?
        }
    };
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    daemon.port.store(port, Ordering::Relaxed);
    info!(
        "{} ({}) listening on port {port}, id {}",
        daemon.name,
        args.data_dir.display(),
        daemon.identity.device_id()
    );

    let _mdns = if args.mdns {
        mdns::advertise(&daemon)
            .map_err(|e| warn!("mDNS advertising unavailable: {e}"))
            .ok()
    } else {
        None
    };
    if args.print_pair_uri {
        println!(
            "PAIR-URI {}",
            daemon.start_pairing(sysinfo::lan_addresses()).to_uri()
        );
    }

    tokio::spawn(wake::announce(daemon.clone()));
    if args.dbus {
        // Only a real desktop session syncs the clipboard (tests run with --no-dbus).
        clipboard::start(daemon.clone());
    }

    let mut term = signal(SignalKind::terminate()).map_err(|e| e.to_string())?;
    tokio::select! {
        res = server::serve(daemon, listener) => res.map_err(|e| format!("server stopped: {e}")),
        _ = tokio::signal::ctrl_c() => Ok(()),
        _ = term.recv() => Ok(()),
    }
}
