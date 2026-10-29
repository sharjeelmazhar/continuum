# ADR-0011: Own port and mDNS service, coexist with KDE Connect

- **Status:** Accepted
- **Date:** 29 October 2026

## Context
Many users will try Continuum while KDE Connect / GSConnect is still installed. KDE Connect uses
UDP/TCP ports 1714–1764 and its own discovery. Reusing its protocol would inherit its weaker
security model (see [ADR-0005](0005-tls13-pinned-self-signed-certs.md)).

## Decision
Continuum uses its own mDNS service type **`_continuum._tcp`** and TCP port **47820** by default.
If that port is taken, it picks any free port and advertises it in mDNS. The QR code carries the
address too, so pairing works even when multicast is blocked.

## Alternatives considered
- **Speak the KDE Connect protocol**: instant compatibility with existing apps, but locks us into
  its design and security choices.
- **Fixed port only**: fails when something else uses it.

## Consequences
- Good: both apps run side by side during migration, with no conflicts.
- Cost: users with a strict firewall need port 47820 (or the advertised port) open. The desktop
  installer adds a firewalld/ufw rule where possible.
