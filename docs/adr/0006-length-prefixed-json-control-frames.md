# ADR-0006: Length-prefixed JSON for control messages

- **Status:** Accepted
- **Date:** 29 October 2026

## Context
Control messages (clipboard, notifications, call events, transfer offers) are small and varied.
They must be easy to parse safely in Rust and Kotlin, easy to debug, and easy to extend.

## Decision
Each message is a **4-byte big-endian length followed by a UTF-8 JSON object**
`{id, type, body}`, with a hard limit of 1 MiB. Bulk data (file bytes, video) never goes through
JSON (see [ADR-0007](0007-separate-data-connections.md)).

## Alternatives considered
- **Newline-delimited JSON** (KDE Connect): a missing newline or a giant line is an easy
  denial-of-service. A length prefix lets us reject oversized frames before reading them.
- **Protocol Buffers / CBOR**: smaller and faster, but adds a schema compiler to both builds and
  makes debugging harder. Control traffic is tiny, so speed isn't the bottleneck.

## Consequences
- Good: human-readable in logs and tests. serde (Rust) and kotlinx.serialization (Kotlin) handle it.
- Cost: slightly larger messages. If profiling ever shows JSON is a bottleneck, switch the
  encoding behind the same framing (new ADR).
