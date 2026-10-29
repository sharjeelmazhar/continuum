//! mDNS advertisement so paired phones can find this computer after its address changes.

use continuum_proto::MDNS_SERVICE;
use mdns_sd::{ServiceDaemon, ServiceInfo};

use crate::state::Daemon;

/// Advertises `_continuum._tcp`. Keep the returned daemon alive for as long as it should run.
pub fn advertise(daemon: &Daemon) -> Result<ServiceDaemon, mdns_sd::Error> {
    let id = daemon.identity.device_id();
    let ty = serde_json::to_value(daemon.device_type)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default();
    let props = [
        ("v", "1"),
        ("id", id.as_str()),
        ("name", daemon.name.as_str()),
        ("type", ty.as_str()),
    ];
    let port = daemon.port.load(std::sync::atomic::Ordering::Relaxed);
    let info = ServiceInfo::new(
        MDNS_SERVICE,
        &id,
        &format!("continuum-{id}.local."),
        "",
        port,
        &props[..],
    )?
    .enable_addr_auto();
    let mdns = ServiceDaemon::new()?;
    mdns.register(info)?;
    Ok(mdns)
}
