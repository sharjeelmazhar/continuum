//! Client side of the daemon's D-Bus API, plus starting the daemon if it isn't running.

use std::time::Duration;

use zbus::{Connection, proxy};

/// (id, name, type, connected, battery level or -1, charging)
pub type DeviceRow = (String, String, String, bool, i32, bool);

#[proxy(
    interface = "io.github.sharjeelmazhar.Continuum.Daemon1",
    default_service = "io.github.sharjeelmazhar.Continuum.Daemon",
    default_path = "/io/github/sharjeelmazhar/Continuum/Daemon"
)]
pub trait Daemon {
    fn list_devices(&self) -> zbus::Result<Vec<DeviceRow>>;
    fn start_pairing(&self) -> zbus::Result<(String, u32)>;
    fn cancel_pairing(&self) -> zbus::Result<()>;
    fn unpair(&self, id: &str) -> zbus::Result<()>;
    fn send_files(&self, id: &str, paths: &[&str]) -> zbus::Result<String>;
    #[zbus(property)]
    fn name(&self) -> zbus::Result<String>;
    #[zbus(signal)]
    fn devices_changed(&self) -> zbus::Result<()>;
    #[zbus(signal)]
    fn paired(&self, id: &str, name: &str) -> zbus::Result<()>;
}

/// Connects to the daemon. If nobody owns its name (not installed as a service, or a dev build),
/// starts the `continuumd` next to this executable, or from PATH, and waits for it.
pub async fn connect() -> Result<DaemonProxy<'static>, String> {
    let conn = Connection::session().await.map_err(|e| e.to_string())?;
    let proxy = DaemonProxy::new(&conn).await.map_err(|e| e.to_string())?;
    if proxy.list_devices().await.is_ok() {
        return Ok(proxy);
    }
    let sibling = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("continuumd")));
    let program = sibling
        .filter(|p| p.exists())
        .map(|p| p.into_os_string())
        .unwrap_or_else(|| "continuumd".into());
    std::process::Command::new(&program).spawn().map_err(|e| {
        format!(
            "could not start the Continuum service ({}): {e}",
            program.to_string_lossy()
        )
    })?;
    for _ in 0..50 {
        glib::timeout_future(Duration::from_millis(100)).await;
        if proxy.list_devices().await.is_ok() {
            return Ok(proxy);
        }
    }
    Err("the Continuum service did not start".into())
}
