//! Continuum daemon library: everything except argument parsing, so tests can drive it.

pub mod clipboard;
pub mod dbus;
pub mod mdns;
pub mod notify;
pub mod outbox;
pub mod server;
pub mod state;
pub mod store;
pub mod sysinfo;
pub mod transfer;
pub mod wake;
