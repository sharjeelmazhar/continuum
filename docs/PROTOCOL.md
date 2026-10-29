# Continuum protocol (draft v1)

This is a draft. M1 finalizes it, and anything that changes must be updated here first.

## Discovery
- mDNS service `_continuum._tcp.local.` on TCP port **47820** (it falls back to any free port,
  which the TXT record advertises).
- TXT: `v=1`, `id=<device id>`, `name=<display name>`, `type=phone|tablet|laptop|desktop`, `fp=<first 16 hex of cert SHA-256>`.
- `id` = base32 of the first 16 bytes of SHA-256(public key). It's stable and not personal.

## Transport and security
- TCP + **TLS 1.3 only**, mutual authentication. Each device has a long-lived Ed25519 key and a
  self-signed cert.
- Before pairing, the server only accepts the `pair.*` messages. After pairing, both sides
  **pin the peer cert's SHA-256**. Any mismatch closes the connection and raises a UI warning.
  No trust-on-first-use without user confirmation.
- Pairing:
  - **QR**: the desktop shows `continuum://pair?h=<host>&p=<port>&fp=<cert sha256>&s=<128-bit secret>`.
    The phone verifies `fp` during the TLS handshake, then sends `pair.request` with
    `HMAC-SHA256(secret, fp_desktop ‖ fp_phone)`. The secret is single-use and expires after 2 min.
  - **SAS**: both screens show a 6-digit code, `SHA-256(fp_a ‖ fp_b ‖ nonce) mod 10^6`, and both
    users confirm it.
- Rate limits: 5 failed pairings per minute per IP. Unknown peers can't send anything except pairing.

## Framing (control connection)
`u32 big-endian length` + UTF-8 JSON body. The body must be ≤ 1 MiB. Anything larger is a
protocol error, and the connection closes.
```json
{ "id": 42, "type": "clipboard.text", "body": { "text": "hello" } }
```
- `type` is `<plugin>.<action>`. Unknown types are ignored, with an `error.unsupported` reply
  so that clients can be newer than the server.
- First message on both sides: `hello { v, id, name, type, caps: ["share","clip",...] }`.
  Each side uses only the features that appear in both lists.
- Keepalive: `ping`/`pong` every 20 s. If 60 s pass without a reply, the peer is treated as
  disconnected and reconnects.

## Data connections (files, video)
The sender sends `share.offer { transfer_id, files: [{name, size, sha256?, mime}] }`.
The receiver replies `share.accept { transfer_id, port, token }`. Then the sender opens a new
TLS connection (same pinned certs) and sends the token and `file_index` + `offset`, so a
transfer resumes from the last verified byte. The receiver writes to `.part` and renames the
file once the hash matches. File names are sanitized: base name only, with no control
characters, no `..`, and at most 255 bytes.

## Plugins (namespaces)
`share` · `clip` · `notif` · `call` · `mirror` · `battery` · `find` · `media`. Each plugin's
messages are documented in that plugin's milestone section when it's built.
