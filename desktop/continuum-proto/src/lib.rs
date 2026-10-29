//! Continuum wire protocol: messages, framing, identity, pairing and certificate pinning.
//! No network I/O here, so everything can be tested without sockets. Spec: `docs/PROTOCOL.md`.

pub mod frame;
pub mod identity;
pub mod message;
pub mod pairing;
pub mod tls;

/// Protocol version spoken by this build.
pub const PROTOCOL_VERSION: u32 = 1;

/// Default TCP port (ADR-0011).
pub const DEFAULT_PORT: u16 = 47820;

/// mDNS service type.
pub const MDNS_SERVICE: &str = "_continuum._tcp.local.";

#[derive(Debug, thiserror::Error)]
pub enum ProtoError {
    #[error("frame of {0} bytes exceeds the 1 MiB limit")]
    FrameTooLarge(usize),
    #[error("malformed message: {0}")]
    BadMessage(#[from] serde_json::Error),
    #[error("malformed pairing link: {0}")]
    BadUri(&'static str),
    #[error("certificate error: {0}")]
    Cert(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol_version_is_set() {
        assert_eq!(PROTOCOL_VERSION, 1);
    }
}
