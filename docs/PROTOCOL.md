# Continuum protocol (v1)

Both apps must follow this document exactly. Change it here first, in the same commit as the code.
Shared test vectors: `desktop/continuum-proto/tests/vectors.json` (used by the Rust and Kotlin tests).

## Identity
- Each device has one long-lived **ECDSA P-256** key pair and a **self-signed X.509 v3
  certificate** (ADR-0015). Desktop: generated on first run, stored `0600`. Android: generated
  inside the Android Keystore (the private key never leaves it).
- **Fingerprint** `fp` = SHA-256 of the certificate DER. Written as 64 lowercase hex characters.
- **Device id** = the first 32 hex characters of `fp` (16 bytes). Certificates are never
  reissued (a new certificate means a new device that must pair again), so the id is stable.

## Discovery
- mDNS service `_continuum._tcp.local.`, instance name = device id, TCP port **47820** (any free
  port if that one is taken). TXT: `v=1`, `id`, `name`, `type`.
- The pairing QR code carries the desktop's addresses too, so pairing works without mDNS.

## Transport
- TCP + **TLS 1.3 only**, and both sides present their certificate (mutual TLS).
- The desktop is the server, the phone is the client.
- The phone accepts the server only if `SHA-256(server cert) == fp` from the QR code (or the
  pinned fp, after pairing). The desktop accepts any client certificate at the TLS layer (the
  TLS handshake still proves the client owns the key), then decides by fingerprint:
  - pinned (paired) fingerprint → normal session;
  - unknown fingerprint → only `pair.request` is allowed. A `hello` gets
    `pair.reject { reason }` (so a phone that was removed while away can forget the computer);
    anything else closes the connection.

## Framing
`u32 big-endian length` + UTF-8 JSON object. Length ≤ **1 MiB** (1,048,576 bytes); a larger
length is a protocol error and the connection closes without reading the body.

```json
{ "type": "battery.status", "body": { "level": 82, "charging": false } }
```
`type` is `<namespace>.<action>`. `body` is an object (may be `{}`). Unknown fields are
ignored. Unknown types get `error {code:"unsupported"}` back and are otherwise ignored.

## Pairing (QR)
The desktop shows:
```
continuum://pair?v=1&h=<ip>[,<ip>…]&p=<port>&fp=<64 hex>&s=<secret>&n=<desktop name, URL-encoded>
```
`s` = 16 random bytes, base64url without padding. Single use, expires after 2 minutes.

1. Phone connects to one of the `h` addresses (tries them in parallel) and checks `fp`.
2. Phone → `pair.request { "name", "type", "proof" }`
   - `proof` = hex(HMAC-SHA256(key = secret bytes, message = fp_desktop_raw ‖ fp_phone_raw)),
     where `fp_*_raw` are the 32-byte digests.
3. Desktop checks the proof (constant time) and that the secret is active, then pins `fp_phone`
   and burns the secret → `pair.accept { "id", "name", "type" }`.
   On failure → `pair.reject { "reason" }` and close.
4. Phone pins `fp_desktop`. The connection continues as a normal session (next: `hello`).

`type` is one of `phone`, `tablet`, `laptop`, `desktop`.

## Session
- Both sides send `hello { "v": 1, "id", "name", "type", "caps": [ … ] }` first (the phone
  right after connecting or after `pair.accept`; the desktop in reply). Features are used only
  if both `caps` lists contain them.
- Keepalive: the phone sends `ping {}` every 20 s; the desktop answers `pong {}`. 60 s without
  any frame = connection dead; the phone reconnects with backoff (1, 2, 4 … 30 s).
- **Wake nudge (UDP):** when the desktop daemon starts, it sends the UTF-8 datagram
  `continuum-wake 1 <desktop id>` to UDP port 47820 at the last address of each paired phone
  that isn't connected (at 0, 2, 5, 10, 20 and 40 s). A phone that's paired with that id and
  not connected retries immediately. It carries no secret and only triggers a normal pinned
  connection attempt.
- `unpair {}`: the sender has removed this pairing. The receiver deletes its pin and closes.
- `error { "code", "msg" }`: informational.

## Messages by plugin
| Type | Direction | Body |
|---|---|---|
| `battery.status` | phone → desktop | `level` 0–100, `charging` bool. Sent after `hello` and on every change. |

Later plugins (`share`, `clip`, `notif`, `call`, `mirror`, `find`, `media`) add their rows here
when they're built.

## File transfer (share)
Paired devices accept files automatically (they're your own devices, like AirDrop between
devices on the same Apple ID). Phone → desktop first; the other direction uses the same messages.

1. Sender → `share.offer { "transfer_id", "files": [{ "name", "size", "mime" }] }`
   - `transfer_id`: 32 hex chars (random). Reused when retrying, so the receiver can resume.
   - At most 1000 files; every `size` is known in advance.
2. Receiver → `share.accept { "transfer_id", "token", "offsets": [u64…] }` or
   `share.reject { "transfer_id", "reason" }` (e.g. not enough disk space).
   - `token`: 32 hex chars, single use, valid 60 s.
   - `offsets[i]`: bytes of file *i* already received and verified earlier (0 for new files).
3. Sender opens a **data connection**: a new TLS connection to the same host and port, with the
   same pinned certificates. Its first (and only) frame is `data.hello { "token" }`.
   Then, raw bytes with no framing: for each file in order, bytes `offsets[i] .. size`,
   followed by the 32-byte SHA-256 of the **whole** file.
4. Receiver writes to a partial file, records the verified offset every 8 MiB (fsync first),
   checks each hash, and only then moves the file into place (unique name, never overwriting).
   It reports on the control connection:
   `share.result { "transfer_id", "ok", "saved": [names], "error"? }`.
   If the data connection breaks, the sender offers the same `transfer_id` again and resumes.

File names from the peer are sanitized: base name only, no control characters, no leading
dots, at most 255 bytes, never `.` or `..`.
