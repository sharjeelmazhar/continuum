//! TLS 1.3 configurations with certificate pinning (ADR-0005).
//!
//! Server: accepts any client certificate at the TLS layer (the handshake still proves the
//! client holds the key); the daemon then checks the fingerprint against the paired list.
//! Client: accepts only the server certificate whose fingerprint was pinned.

use std::sync::Arc;

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{CryptoProvider, WebPkiSupportedAlgorithms, verify_tls13_signature};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName, UnixTime};
use rustls::server::danger::{ClientCertVerified, ClientCertVerifier};
use rustls::{CertificateError, DigitallySignedStruct, DistinguishedName, Error, SignatureScheme};

use crate::ProtoError;
use crate::identity::{Fingerprint, Identity, fingerprint};

fn provider() -> Arc<CryptoProvider> {
    Arc::new(rustls::crypto::ring::default_provider())
}

fn algorithms() -> WebPkiSupportedAlgorithms {
    rustls::crypto::ring::default_provider().signature_verification_algorithms
}

fn key(id: &Identity) -> PrivateKeyDer<'static> {
    PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(id.key_der.clone()))
}

pub fn server_config(id: &Identity) -> Result<rustls::ServerConfig, ProtoError> {
    rustls::ServerConfig::builder_with_provider(provider())
        .with_protocol_versions(&[&rustls::version::TLS13])
        .and_then(|b| {
            b.with_client_cert_verifier(Arc::new(AnyClientCert(algorithms())))
                .with_single_cert(vec![CertificateDer::from(id.cert_der.clone())], key(id))
        })
        .map_err(|e| ProtoError::Cert(e.to_string()))
}

pub fn client_config(
    id: &Identity,
    server_fp: Fingerprint,
) -> Result<rustls::ClientConfig, ProtoError> {
    rustls::ClientConfig::builder_with_provider(provider())
        .with_protocol_versions(&[&rustls::version::TLS13])
        .and_then(|b| {
            b.dangerous()
                .with_custom_certificate_verifier(Arc::new(PinnedServer {
                    fp: server_fp,
                    algs: algorithms(),
                }))
                .with_client_auth_cert(vec![CertificateDer::from(id.cert_der.clone())], key(id))
        })
        .map_err(|e| ProtoError::Cert(e.to_string()))
}

#[derive(Debug)]
struct AnyClientCert(WebPkiSupportedAlgorithms);

impl ClientCertVerifier for AnyClientCert {
    fn root_hint_subjects(&self) -> &[DistinguishedName] {
        &[]
    }

    fn verify_client_cert(
        &self,
        _: &CertificateDer<'_>,
        _: &[CertificateDer<'_>],
        _: UnixTime,
    ) -> Result<ClientCertVerified, Error> {
        Ok(ClientCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _: &[u8],
        _: &CertificateDer<'_>,
        _: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        Err(Error::PeerIncompatible(
            rustls::PeerIncompatible::Tls12NotOffered,
        ))
    }

    fn verify_tls13_signature(
        &self,
        msg: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        verify_tls13_signature(msg, cert, dss, &self.0)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.0.supported_schemes()
    }
}

#[derive(Debug)]
struct PinnedServer {
    fp: Fingerprint,
    algs: WebPkiSupportedAlgorithms,
}

impl ServerCertVerifier for PinnedServer {
    fn verify_server_cert(
        &self,
        cert: &CertificateDer<'_>,
        _: &[CertificateDer<'_>],
        _: &ServerName<'_>,
        _: &[u8],
        _: UnixTime,
    ) -> Result<ServerCertVerified, Error> {
        if fingerprint(cert) == self.fp {
            Ok(ServerCertVerified::assertion())
        } else {
            Err(Error::InvalidCertificate(
                CertificateError::ApplicationVerificationFailure,
            ))
        }
    }

    fn verify_tls12_signature(
        &self,
        _: &[u8],
        _: &CertificateDer<'_>,
        _: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        Err(Error::PeerIncompatible(
            rustls::PeerIncompatible::Tls12NotOffered,
        ))
    }

    fn verify_tls13_signature(
        &self,
        msg: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        verify_tls13_signature(msg, cert, dss, &self.algs)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.algs.supported_schemes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustls::{ClientConnection, ServerConnection};

    /// Runs a full in-memory handshake. Returns the client cert the server saw, or the error.
    fn handshake(
        server: &Identity,
        client: &Identity,
        pin: Fingerprint,
    ) -> Result<Fingerprint, Error> {
        let mut s = ServerConnection::new(Arc::new(server_config(server).unwrap()))?;
        let name = ServerName::try_from("continuum.invalid").unwrap();
        let mut c = ClientConnection::new(Arc::new(client_config(client, pin).unwrap()), name)?;
        for _ in 0..10 {
            let mut buf = Vec::new();
            c.write_tls(&mut buf).unwrap();
            s.read_tls(&mut buf.as_slice()).unwrap();
            s.process_new_packets()?;
            let mut buf = Vec::new();
            s.write_tls(&mut buf).unwrap();
            c.read_tls(&mut buf.as_slice()).unwrap();
            c.process_new_packets()?;
            if !s.is_handshaking() && !c.is_handshaking() {
                let peer = s.peer_certificates().expect("client cert required");
                return Ok(fingerprint(&peer[0]));
            }
        }
        panic!("handshake did not finish");
    }

    #[test]
    fn pinned_handshake_succeeds_and_server_sees_client_cert() {
        let (d, p) = (Identity::generate().unwrap(), Identity::generate().unwrap());
        assert_eq!(handshake(&d, &p, d.fingerprint()).unwrap(), p.fingerprint());
    }

    #[test]
    fn wrong_pin_is_rejected() {
        let (d, p, other) = (
            Identity::generate().unwrap(),
            Identity::generate().unwrap(),
            Identity::generate().unwrap(),
        );
        let err = handshake(&d, &p, other.fingerprint()).unwrap_err();
        assert!(matches!(err, Error::InvalidCertificate(_)), "{err:?}");
    }
}
