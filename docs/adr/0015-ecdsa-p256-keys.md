# ADR-0015: ECDSA P-256 device keys

- **Status:** Accepted
- **Date:** 29 October 2026

## Context
ADR-0005 left the key algorithm open (Ed25519 or ECDSA P-256). On Android the private key must
live in the Android Keystore (hardware-backed on most phones) so it can never be copied off the
device. The Keystore supports EC P-256 on every Android version we target, but not Ed25519 for
TLS client authentication.

## Decision
Both sides use **ECDSA P-256 with SHA-256** keys and self-signed X.509 certificates. The desktop
generates them with rcgen. The phone generates the key inside the Android Keystore, which also
creates the self-signed certificate.

The device id is the first 16 bytes of the certificate fingerprint (see PROTOCOL.md), so neither
side needs a certificate parser to compute it.

## Alternatives considered
- **Ed25519**: smaller and faster, but not usable from the Android Keystore for TLS.
- **Software key on Android**: works everywhere, but the key could be extracted from a rooted or
  backed-up phone.
- **RSA**: much bigger keys and slower handshakes for no benefit.

## Consequences
- Good: the phone's key is non-exportable. P-256 is supported by rustls, Conscrypt and every TLS stack.
- Cost: none significant. Certificates are never reissued; a new key means pairing again.
