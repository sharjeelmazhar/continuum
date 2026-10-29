//! Facts about this computer: display name, laptop or desktop, and LAN addresses.

use std::fs;
use std::net::IpAddr;

use continuum_proto::message::DeviceType;

/// The "pretty" hostname set in GNOME Settings (About → Device Name), else the kernel hostname.
pub fn device_name() -> String {
    let pretty = fs::read_to_string("/etc/machine-info").ok().and_then(|s| {
        s.lines()
            .find_map(|l| l.strip_prefix("PRETTY_HOSTNAME="))
            .map(|v| v.trim().trim_matches('"').to_owned())
            .filter(|v| !v.is_empty())
    });
    pretty
        .or_else(|| {
            fs::read_to_string("/proc/sys/kernel/hostname")
                .ok()
                .map(|s| s.trim().to_owned())
        })
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "Linux computer".to_owned())
}

/// Laptop if the firmware says so (SMBIOS chassis types 8, 9, 10, 14, 31, 32), else desktop.
pub fn device_type() -> DeviceType {
    let chassis = fs::read_to_string("/sys/class/dmi/id/chassis_type").unwrap_or_default();
    match chassis.trim() {
        "8" | "9" | "10" | "14" | "31" | "32" => DeviceType::Laptop,
        _ => DeviceType::Desktop,
    }
}

/// IPv4 addresses the phone can likely reach: real network interfaces first, virtual ones
/// (Docker, VMs, VPN tunnels) skipped. At most 4.
pub fn lan_addresses() -> Vec<String> {
    let Ok(ifaces) = if_addrs::get_if_addrs() else {
        return Vec::new();
    };
    let mut out: Vec<(u8, String)> = ifaces
        .into_iter()
        .filter(|i| !i.is_loopback())
        .filter(|i| {
            ![
                "docker", "veth", "br-", "virbr", "lxc", "lxd", "tun", "tap", "vmnet", "vboxnet",
            ]
            .iter()
            .any(|p| i.name.starts_with(p))
        })
        .filter_map(|i| match i.ip() {
            IpAddr::V4(ip) if !ip.is_link_local() => {
                let rank = if i.name.starts_with("wl") {
                    0
                } else if i.name.starts_with("en") || i.name.starts_with("eth") {
                    1
                } else {
                    2
                };
                Some((rank, ip.to_string()))
            }
            _ => None,
        })
        .collect();
    out.sort();
    out.dedup();
    out.into_iter().take(4).map(|(_, ip)| ip).collect()
}

/// The user's Downloads folder (from `~/.config/user-dirs.dirs`, so it follows the language).
pub fn download_dir() -> std::path::PathBuf {
    let home = std::path::PathBuf::from(std::env::var_os("HOME").unwrap_or_default());
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| home.join(".config"));
    fs::read_to_string(config.join("user-dirs.dirs"))
        .ok()
        .and_then(|s| {
            s.lines()
                .find_map(|l| l.strip_prefix("XDG_DOWNLOAD_DIR="))
                .map(|v| {
                    let v = v.trim().trim_matches('"');
                    match v.strip_prefix("$HOME") {
                        Some(rest) => home.join(rest.trim_start_matches('/')),
                        None => std::path::PathBuf::from(v),
                    }
                })
        })
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| home.join("Downloads"))
}
