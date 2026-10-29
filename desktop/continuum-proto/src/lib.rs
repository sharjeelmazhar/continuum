//! Continuum wire protocol: types, framing, crypto and pairing. No I/O beyond traits.

/// Protocol version spoken by this build.
pub const PROTOCOL_VERSION: u32 = 1;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol_version_is_set() {
        assert_eq!(PROTOCOL_VERSION, 1);
    }
}
