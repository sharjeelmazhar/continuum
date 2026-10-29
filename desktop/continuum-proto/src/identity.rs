//! Device identity: an ECDSA P-256 key and a self-signed certificate (ADR-0015).

use ring::digest::{SHA256, digest};

use crate::ProtoError;

/// SHA-256 of a certificate (DER).
pub type Fingerprint = [u8; 32];

/// This device's key pair and certificate, both DER-encoded.
#[derive(Clone)]
pub struct Identity {
    pub cert_der: Vec<u8>,
    /// PKCS#8 private key.
    pub key_der: Vec<u8>,
}

impl Identity {
    /// Creates a new key pair and a self-signed certificate.
    pub fn generate() -> Result<Self, ProtoError> {
        let err = |e: rcgen::Error| ProtoError::Cert(e.to_string());
        let key = rcgen::KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256).map_err(err)?;
        let mut params = rcgen::CertificateParams::new(Vec::<String>::new()).map_err(err)?;
        params
            .distinguished_name
            .push(rcgen::DnType::CommonName, "Continuum");
        let cert = params.self_signed(&key).map_err(err)?;
        Ok(Self {
            cert_der: cert.der().to_vec(),
            key_der: key.serialize_der(),
        })
    }

    pub fn fingerprint(&self) -> Fingerprint {
        fingerprint(&self.cert_der)
    }

    pub fn device_id(&self) -> String {
        device_id(&self.fingerprint())
    }
}

pub fn fingerprint(cert_der: &[u8]) -> Fingerprint {
    digest(&SHA256, cert_der)
        .as_ref()
        .try_into()
        .expect("SHA-256 is 32 bytes")
}

/// Device id: first 16 bytes of the fingerprint, as lowercase hex.
pub fn device_id(fp: &Fingerprint) -> String {
    hex::encode(&fp[..16])
}

pub fn fingerprint_from_hex(s: &str) -> Option<Fingerprint> {
    let bytes = hex::decode(s).ok()?;
    bytes.try_into().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_identities_differ_and_ids_are_short() {
        let a = Identity::generate().unwrap();
        let b = Identity::generate().unwrap();
        assert_ne!(a.fingerprint(), b.fingerprint());
        assert_eq!(a.device_id().len(), 32);
        assert!(hex::encode(a.fingerprint()).starts_with(&a.device_id()));
    }

    #[test]
    fn hex_round_trip() {
        let fp = Identity::generate().unwrap().fingerprint();
        assert_eq!(fingerprint_from_hex(&hex::encode(fp)), Some(fp));
        assert_eq!(fingerprint_from_hex("abcd"), None);
    }
}
