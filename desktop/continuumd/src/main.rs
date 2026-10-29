//! Continuum daemon (headless). UIs talk to it over D-Bus.

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::atomic::Ordering;

use continuum_proto::DEFAULT_PORT;
use continuumd::state::Daemon;
use continuumd::store::Store;
use continuumd::{dbus, mdns, server, sysinfo};
use tokio::net::TcpListener;
use tokio::signal::unix::{SignalKind, signal};
use tracing::{error, info, warn};

const USAGE: &str =
    "usage: continuumd [--data-dir DIR] [--port N] [--no-dbus] [--no-mdns] [--print-pair-uri]";

struct Args {
    data_dir: PathBuf,
    port: u16,
    dbus: bool,
    mdns: bool,
    print_pair_uri: bool,
}

fn parse_args() -> Result<Args, String> {
    let mut a = Args {
        data_dir: Store::default_dir(),
        port: DEFAULT_PORT,
        dbus: true,
        mdns: true,
        print_pair_uri: false,
    };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--data-dir" => a.data_dir = it.next().ok_or(USAGE)?.into(),
            "--port" => a.port = it.next().and_then(|p| p.parse().ok()).ok_or(USAGE)?,
            "--no-dbus" => a.dbus = false,
            "--no-mdns" => a.mdns = false,
            "--print-pair-uri" => a.print_pair_uri = true,
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

    let mut term = signal(SignalKind::terminate()).map_err(|e| e.to_string())?;
    tokio::select! {
        res = server::serve(daemon, listener) => res.map_err(|e| format!("server stopped: {e}")),
        _ = tokio::signal::ctrl_c() => Ok(()),
        _ = term.recv() => Ok(()),
    }
}
