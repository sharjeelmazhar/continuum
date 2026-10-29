# ADR-0007: Separate connections for bulk data

- **Status:** Accepted
- **Date:** 29 October 2026

## Context
A 4 GB video transfer and an incoming-call banner must not wait for each other. On one TCP
connection, a big chunk blocks every message behind it (head-of-line blocking).

## Decision
Each peer pair has **one control connection** for small messages. Each file transfer or video
stream opens its **own TLS connection**, authenticated with the same pinned certificates, plus a
one-time token issued over the control connection.

## Alternatives considered
- **Multiplex everything on one connection** (like HTTP/2): solves blocking only partly (TCP
  still blocks), and is more complex to implement twice.
- **QUIC**: built-in streams without head-of-line blocking, but immature on Android without
  extra native libraries. A good candidate to revisit later.

## Consequences
- Good: calls and notifications stay instant during big transfers. Transfers can run in parallel
  and resume independently.
- Cost: more TLS handshakes (cheap on a LAN). The token must be single-use and short-lived.
