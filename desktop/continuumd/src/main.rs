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

const USAGE: &str = "usage: continuumd [--data-dir DIR] [--port N] [--no-dbus] [--no-mdns] [--print-pair-uri] [--download-dir DIR]\n       continuumd send [--to DEVICE-ID] FILE...\n       continuumd ping [--to PHONE] [--from APP] [--urgent] TITLE [MESSAGE]";

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
                // Started from a right-click menu: nobody sees stderr.
                if !std::io::IsTerminal::is_terminal(&std::io::stderr()) {
                    notify_failure(&e).await;
                }
                ExitCode::FAILURE
            }
        };
    }
    if let Some(uid) = login_screen_account() {
        // The login screen (gdm-greeter, sddm, …) is a session too and would start us from
        // the global user unit: it would hold the port with an unpaired identity until the
        // real user logs in, which delays every phone's connection.
        info!("not starting for system account {uid}");
        return ExitCode::SUCCESS;
    }
    if argv.first().map(String::as_str) == Some("ping") {
        return match ping_cli(&argv[1..]).await {
            Ok(msg) => {
                println!("{msg}");
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("continuumd ping: {e}");
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

const PING_USAGE: &str =
    "usage: continuum-ping [--to PHONE] [--from APP] [--urgent] TITLE [MESSAGE]
Shows a notification on your phone (every connected phone, or PHONE: its name or id).
  TITLE    the headline, e.g. \"Needs your password\"
  MESSAGE  a sentence or two of explanation
  --from   who is asking, e.g. \"Claude Code\" (shown as \"Claude Code on <this computer>\")
  --urgent rings louder (alarm category)
Exits with 0 if a phone showed it, 1 if none did (none connected, or switched off there).";

/// `continuumd ping` / `continuum-ping`: a notification on the phone, for scripts and tools.
async fn ping_cli(argv: &[String]) -> Result<String, String> {
    let (mut to, mut from, mut urgent, mut words) = (None, String::new(), false, Vec::new());
    let mut it = argv.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--to" => to = Some(it.next().ok_or(PING_USAGE)?.clone()),
            "--from" => from = it.next().ok_or(PING_USAGE)?.clone(),
            "--urgent" => urgent = true,
            "-h" | "--help" => return Ok(PING_USAGE.into()),
            _ => words.push(a.clone()),
        }
    }
    let (title, text) = match words.as_slice() {
        [] => return Err(PING_USAGE.into()),
        [t] => (t.clone(), String::new()),
        [t, rest @ ..] => (t.clone(), rest.join(" ")),
    };
    let conn = zbus::Connection::session()
        .await
        .map_err(|e| e.to_string())?;
    let rows: Vec<dbus::DeviceRow> = conn
        .call_method(
            Some(dbus::BUS_NAME),
            dbus::PATH,
            Some(dbus::IFACE),
            "ListDevices",
            &(),
        )
        .await
        .map_err(|_| "Continuum isn't running".to_owned())?
        .body()
        .deserialize()
        .map_err(|e| e.to_string())?;
    let id = match &to {
        None => String::new(),
        Some(want) => rows
            .iter()
            .find(|r| r.0 == *want || r.1.eq_ignore_ascii_case(want))
            .map(|r| r.0.clone())
            .ok_or_else(|| format!("no paired phone called {want}"))?,
    };
    let reply = conn
        .call_method(
            Some(dbus::BUS_NAME),
            dbus::PATH,
            Some(dbus::IFACE),
            "Ping",
            &(id, title, text, from, urgent),
        )
        .await
        .map_err(|e| match e {
            zbus::Error::MethodError(_, Some(msg), _) => msg,
            e => e.to_string(),
        })?;
    let shown: Vec<String> = reply.body().deserialize().map_err(|e| e.to_string())?;
    Ok(format!("shown on {}", shown.join(", ")))
}

/// A desktop notification for a failed `continuumd send`.
async fn notify_failure(reason: &str) {
    let Ok(conn) = zbus::Connection::session().await else {
        return;
    };
    let hints = std::collections::HashMap::<&str, zbus::Variant<'_>>::from([(
        "desktop-entry",
        "io.github.sharjeelmazhar.Continuum".into(),
    )]);
    let body = reason.replace('&', "&amp;").replace('<', "&lt;");
    let _ = conn
        .call_method(
            Some("org.freedesktop.Notifications"),
            "/org/freedesktop/Notifications",
            Some("org.freedesktop.Notifications"),
            "Notify",
            &(
                "Continuum",
                0u32,
                "io.github.sharjeelmazhar.Continuum",
                "Couldn't send",
                body.as_str(),
                Vec::<&str>::new(),
                hints,
                -1i32,
            ),
        )
        .await;
}

/// `continuumd send`: asks the running daemon to send files (used by file manager menus).
async fn send_cli(argv: &[String]) -> Result<String, String> {
    let mut to = None;
    let mut paths = Vec::new();
    let mut it = argv.iter();
    let mut only_paths = false;
    while let Some(a) = it.next() {
        match a.as_str() {
            "--to" if !only_paths => to = Some(it.next().ok_or(USAGE)?.clone()),
            "--" if !only_paths => only_paths = true,
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

    let listener = bind_preferred(args.port).await?;
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
        tokio::spawn(continuumd::menus::run(daemon.clone()));
        std::thread::spawn(|| continuumd::folders::clean_old(&continuumd::folders::outgoing_dir()));
    }

    let mut term = signal(SignalKind::terminate()).map_err(|e| e.to_string())?;
    let result = tokio::select! {
        res = server::serve(daemon, listener) => res.map_err(|e| format!("server stopped: {e}")),
        _ = tokio::signal::ctrl_c() => Ok(()),
        _ = term.recv() => Ok(()),
    };
    if args.dbus {
        continuumd::menus::clear();
    }
    result
}

/// Our uid if it isn't a regular login account (outside UID_MIN..=UID_MAX of login.defs):
/// display-manager greeters, system users.
fn login_screen_account() -> Option<u32> {
    use std::os::unix::fs::MetadataExt;
    let uid = std::fs::metadata("/proc/self").ok()?.uid();
    let defs = std::fs::read_to_string("/etc/login.defs").unwrap_or_default();
    (!regular_uid(uid, &defs)).then_some(uid)
}

fn regular_uid(uid: u32, login_defs: &str) -> bool {
    let value = |key: &str, default: u32| {
        login_defs
            .lines()
            .rev()
            .find_map(|l| {
                let mut w = l.split_whitespace();
                (w.next() == Some(key)).then(|| w.next()?.parse().ok())?
            })
            .unwrap_or(default)
    };
    // Developers may run as root on purpose; only skip unprivileged system accounts.
    uid == 0 || (value("UID_MIN", 1000)..=value("UID_MAX", 60000)).contains(&uid)
}

/// Listens on `port`, waiting for it if something holds it for a moment (at login another
/// program can briefly have it). Paired phones remember this port, so a random one is the last
/// resort: they then have to find the computer again over mDNS, which some phones block.
async fn bind_preferred(port: u16) -> Result<TcpListener, String> {
    const TRIES: u32 = 30;
    let mut last = None;
    for attempt in 0..TRIES {
        match TcpListener::bind(("0.0.0.0", port)).await {
            Ok(l) => return Ok(l),
            Err(e) if e.kind() == std::io::ErrorKind::AddrInUse && port != 0 => {
                if attempt == 0 {
                    warn!("port {port} in use; waiting for it");
                }
                last = Some(e);
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
            Err(e) => {
                last = Some(e);
                break;
            }
        }
    }
    warn!(
        "port {port} unavailable ({}); using a free port",
        last.map(|e| e.to_string()).unwrap_or_default()
    );
    TcpListener::bind(("0.0.0.0", 0))
        .await
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn greeter_accounts_are_not_regular() {
        let defs = "# comment\nUID_MIN 1000\nUID_MAX\t60000\n";
        assert!(regular_uid(1000, defs));
        assert!(regular_uid(0, defs));
        assert!(!regular_uid(60578, defs), "gdm-greeter");
        assert!(!regular_uid(975, defs), "gdm");
        assert!(regular_uid(1500, ""), "defaults");
        assert!(regular_uid(600, "UID_MIN 500"));
    }

    #[tokio::test]
    async fn waits_for_its_port_to_free_up() {
        let holder = std::net::TcpListener::bind(("0.0.0.0", 0)).unwrap();
        let port = holder.local_addr().unwrap().port();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(1500));
            drop(holder);
        });
        let l = bind_preferred(port).await.unwrap();
        assert_eq!(l.local_addr().unwrap().port(), port);
    }
}
