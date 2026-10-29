# ADR-0005: TLS 1.3 with pinned self-signed certificates

- **Status:** Accepted
- **Date:** 29 October 2026

## Context
Devices talk over home, office and café Wi-Fi, where other people can see and tamper with
traffic. There is no server, no account, and no internet requirement. KDE Connect historically
used trust-on-first-use, which lets an attacker on the network slip in during the first contact.

## Decision
- Each device generates its own long-lived key pair and a **self-signed certificate** on first run.
- All traffic uses **TLS 1.3 only, with mutual authentication** (both sides present certificates).
- Trust is established only by a user action: scanning a QR code that contains the desktop's
  certificate fingerprint plus a one-time secret, or comparing a 6-digit code on both screens.
- After pairing, each side **pins** the other's certificate fingerprint and rejects anything else.

## Alternatives considered
- **A certificate authority / cloud account**: needs a server and accounts, breaks "no cloud".
- **Trust on first use**: invisible to an attacker who is present at first contact.
- **Noise protocol / custom crypto**: fine cryptography, but less battle-tested tooling on
  Android than TLS, and harder to audit.
- **TLS 1.2 allowed**: needed only for very old systems, which we don't support.

## Consequences
- Good: an attacker on the LAN can't read or change traffic, and can't pair without physical
  access to both screens.
- Cost: losing a device's key means re-pairing (acceptable). Custom certificate verifiers on
  both platforms must be written carefully and tested (M1, M2).
- Key algorithm (Ed25519 vs ECDSA P-256) is fixed in M1 after checking Android Keystore support.
