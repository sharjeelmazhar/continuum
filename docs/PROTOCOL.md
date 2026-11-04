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
  if both `caps` lists contain them. Phones from 0.5.2 also send `"model": "<manufacturer>
  <model>"` (at most 64 printable characters kept), which the desktop uses to pick a picture.
- Keepalive: each side answers `ping {}` with `pong {}`. The desktop (0.5.1+) pings every 3 s
  and drops a session after 9 s without any frame (so a phone that leaves Wi-Fi shows as gone
  within seconds); the phone drops it after 60 s, then reconnects with backoff (1, 2, 4 … s, at
  most 60 s with the screen on and 10 minutes with it off; screen on, opening the app, a new
  network and the wake nudge retry at once). The phone pings only after 20 s without any frame
  (40 s to a desktop with the `presence` cap).
- **Presence (cap `presence`, 0.5.4+):** `presence { "active": bool }`, phone → desktop, sent
  right after `hello` and on every change, only to a desktop that lists `presence`. `active` is
  true while the screen is on, the app is open, a transfer runs or the phone rings (find). While
  the phone is not active and the desktop has no request, download or file transfer to it
  running, the desktop pings every 30 s and drops the session after 75 s without a frame, so the
  phone's Wi-Fi can stay in power save. When the desktop user acts on the phone at that pace
  (request, download, sending files, a command), the desktop pings at once and drops the
  session if nothing arrives within 4 s. A phone that never sends `presence` keeps the 3 s pace.
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
| `battery.status` | phone → desktop | `level` 0–100, `charging` bool; while charging (0.5.1+) also `watts` (0–240, 0 = unknown), `fast` (fast charger) and `full_in` (minutes to full, -1 = unknown). Sent after `hello` and when `level`, `charging` or `fast` change; changes of `watts` (≥ 2 W) or `full_in` (≥ 5 min) at most every 45 s (0.5.4+). |
| `presence` | phone → desktop | `active` bool; see "Session". Needs the `presence` cap on the desktop. |
| `clipboard` | both ways | A copied text or image; see "Clipboard". Needs the `clipboard` cap on both sides. |

Later plugins (`call`, `mirror`, `find`, `media`) add their rows here when they're built.

## Requests (phone as a data source)
The desktop asks the phone for data (photos, messages) with **requests**: the body carries
`req` (u32, chosen by the desktop, unique among its open requests) and the phone answers with
`<type>.reply { "req", … }` exactly once. A reply with `error` (string) failed; the special value
`"permission"` means the user has to allow access in the Continuum app on the phone. The desktop
gives up after 20 s (downloads: 30 s without a new part) and ignores late replies. Other errors the phone
may send: `"busy"` (more than 8 requests at once), `"bad request"`, `"not found"`,
`"too large"`, `"empty file"`, `"not a folder"`, `"not a file"`, or a plain message. At most 3
downloads (`*.fetch`) run at once per connection; a 4th gets `"busy"`.

### Photos (cap `photos`)
| Type | Body | Reply |
|---|---|---|
| `photos.list` | `req`, `offset` ≥ 0, `limit` 1–200 | `items`: `[{ "id" (i64), "name", "mime", "date" (ms since epoch), "size", "w", "h" (as displayed: rotation applied), "dur" (ms, 0 for images) }]`, newest first; `total` |
| `photos.thumbs` | `req`, `ids` (≤ 24), `size` (longest side, 64–640 px; older phones cap at 320) | `items`: `[{ "id", "data" (base64 JPEG) }]`. Ids that no longer exist, or don't fit in the 900 KB budget, are left out (ask again). |
| `photos.fetch` | `req`, `id` | `name`, `mime`, `size`, `parts`; then `parts` × `photos.part { "req", "part", "data" }` (512 KiB raw per part, base64, in order). Photos and videos, at most 4 GiB. Image formats other than JPEG/PNG/WebP/GIF (e.g. HEIC) are converted to JPEG first; videos are sent as they are. |
| `photos.send` | `req`, `ids` (≤ 100) | `{}`. The phone sends those photos/videos as a normal file transfer (`share.offer`), so they land in the download folder. |

Photos and videos come from the phone's MediaStore (all albums), ids are MediaStore ids.

### Files (cap `files`)
Browsing the phone's shared storage (what the Files app shows: `/storage/emulated/0`). Paths
are relative to that root, `/`-separated, without a leading `/`; `""` is the root. The phone
rejects `..`, `.` and empty segments, NUL, and anything that resolves (after symlinks) outside the
root. Needs "All files access" on Android 11+ (storage permission before), else `"permission"`.

| Type | Body | Reply |
|---|---|---|
| `files.list` | `req`, `path` | `path` (as asked), `entries`: `[{ "name", "dir" (bool), "size", "date" (ms, modified), "mime" ("" for folders) }]`, folders first then by name; at most 5000 entries; hidden (`.`-prefixed) entries left out; `"truncated": true` when cut (5000 entries or ~900 KB) |
| `files.fetch` | `req`, `path` | Like `photos.fetch`: `name`, `mime`, `size`, `parts`, then `parts` × `photos.part`. Regular files only, at most 4 GiB. |

**Fast downloads (cap `fetch.raw`, 0.5.2+).** To a phone that lists `fetch.raw`, the desktop
adds `"token"` (32 lowercase hex characters, 128 random bits, single use) to `photos.fetch` /
`files.fetch`. The phone replies as usual with `"raw": true`, then opens a data connection
(same pinned TLS as file transfers) whose only frame is `data.hello { "token" }`, followed by the
`size` raw bytes and their SHA-256 (32 bytes). No base64, no frames: the bytes go as fast as the
link allows. The desktop accepts the token only from the device it sent it to, only once, and
keeps the file only if the length and hash match. A raw reply to a request without a token is an
error. Phones without the cap ignore `token` and send parts.

### Find my phone (cap `find`)
- `find.ring {}` (desktop → phone): rings at full alarm volume, even on silent (and lifts Do Not
  Disturb where Android allows it: up to Android 14), until the user
  taps Stop on the phone or `find.stop {}` arrives; gives up after 60 s.
- `find.stop {}`.

### Messages (cap `sms`)
| Type | Body | Reply |
|---|---|---|
| `sms.threads` | `req`, `limit` 1–200 | `threads`: `[{ "thread" (i64), "address", "name" (contact name or ""), "snippet", "date" (ms), "unread" (count), "out" (last message was sent by the phone), "photo" (0.5.1+: the contact's photo, 96 px base64 JPEG, or "" when there is none or the 500 KB photo budget is used up) }]`, newest first |
| `sms.messages` | `req`, `thread`, `before` (ms, 0 = newest), `limit` 1–200 | `messages`: `[{ "id", "address", "body", "date", "out", "status" ("sent", "sending", "failed", "received") }]`, **oldest first** (the newest `limit` messages older than `before`) |
| `sms.send` | `req`, `address`, `body` (1–5000 characters) | `{}` once the phone has handed it to the modem |

A `sms.messages` page may hold fewer than `limit` messages: the phone keeps the reply under
about 800 KB (each body at most 20,000 bytes) by leaving out the oldest, so paging with `before`
still works. MMS appear as `[MMS]` messages with negative ids.

Phone → desktop: `sms.changed {}` whenever the phone's SMS database changes (a message
arrived, was sent or read). The desktop reloads what it shows. Text SMS only for now (MMS show
as `[MMS]` snippets).

### Notifications (cap `notif`)
Phone → desktop, needs notification access on the phone (an explicit system setting):
- `notif.posted { "key", "pkg", "app", "title", "text", "time" (ms), "icon" (base64 PNG ≤ 96 px, may be ""; to a desktop with the `notif.icons` cap, phones from 0.5.4 send each app's icon once per connection and `""` after that, and the desktop reuses the one it got), "reply" (bool: has a reply field), "actions" ([titles], ≤ 3), "silent" (bool) }`.
  Sent for every new or updated notification, and for all current ones right after `hello`
  (those with `silent: true`, so the desktop lists them without popping them up). Updates of
  an "alert once" notification are silent too, and the desktop also pops up only notifications
  that are new or whose title or text changed (apps repost unchanged ones). Ongoing,
  group-summary, media and Continuum's own notifications are not sent.
- `notif.removed { "key" }`: dismissed or gone on the phone.

Desktop → phone:
- `notif.dismiss { "key" }`, `notif.reply { "key", "text" }` (quick reply through the
  notification's reply action), `notif.action { "key", "index" }`.

## Screen mirroring (cap `mirror`, 0.5.9+)
Like iPhone Mirroring: the phone's screen in a window on the computer, used with the mouse and
keyboard. Needs no developer options or USB debugging.

1. Desktop → `mirror.start { "token" (32 hex, single use, valid 120 s), "max" (longest video
   side, 480-2560) }`. If the phone has no screen-sharing permission yet, it asks its user
   (Android's own prompt, plus a notification where background prompts are blocked) and sends
   `mirror.state { "state": "consent" }`.
2. The phone opens a **data connection** with `data.hello { "token" }`, then sends frames: a
   4-byte big-endian length, then one H.264 access unit (Annex B; key frames start with
   SPS/PPS). Frames are at most 8 MiB. A length of 0 is a heartbeat (no frame): the phone sends
   one after 1 s without frames (some encoders send nothing while the screen is still), so 15 s
   of silence means the phone is gone. Rotating the phone starts a new stream configuration (new
   SPS) at the new size, always with the screen's exact shape.
3. Desktop → `mirror.input` (only acted on while a stream is live, and only when the user turned
   on the phone's "Continuum remote control" accessibility service). Positions are fractions
   of the screen (0..1):
   - `{ "t": "down" | "move" | "up", "x", "y" }`: a live finger (mouse drag, touchpad
     scroll): it goes down, follows each `move` as it arrives and lifts on `up` (or after 3 s
     without input). The phone plays it as one continued stroke.
   - `{ "t": "touch", "p": [[x, y, ms], …] }`: a whole gesture at once, 1-200 points, `ms` since the press, not
     decreasing, < 59 s. A still touch is a tap (or a long press when held).
   - `{ "t": "scroll", "x", "y", "dx", "dy" }`: `dx`/`dy` in -1..1 of the screen size;
     positive `dy` scrolls down.
   - `{ "t": "key", "k" }`: `back`, `home`, `recents`, `notifications`, `screenshot`, `lock`,
     `backspace`, `delete`, `enter`, `paste`, and `sync` (not an input: the desktop lost frames and
     asks for a key frame; works without remote control).
   - `{ "t": "text", "s" }`: 1-1000 characters typed into the focused field.
4. Desktop → `mirror.stop {}`, or it closes the data connection. The phone ends screen sharing
   at once (0.5.9+; before, it kept the permission for 5 minutes), so its notification goes too.

Phone → desktop `mirror.request {}` (0.5.9+, the phone's Mirror tile in Quick Settings): the
desktop opens its mirror window for that phone, which then sends `mirror.start` as usual. At
most one window per phone every 5 s.

Phone → desktop `mirror.state { "state", "control" (bool: remote control is on) }`, with
`state` one of `consent`, `started`, `denied`, `stopped`, `error`, `control` (only `control`
changed).

## File transfer (share)
Paired devices accept files automatically (they're your own devices, like AirDrop between
devices on the same Apple ID). Both directions use the same messages. The **phone always opens
the data connection** (it can't accept incoming connections): when it sends, it writes the bytes;
when the desktop sends (the phone has the `share.receive` cap), the phone opens it to read them.

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
5. `transfer_id` is derived from the files (SHA-256 of name, size and modification time, first
   16 bytes as hex), so sharing the same files again later resumes too. A receiver refuses an
   offer whose `transfer_id` is already receiving. Unfinished transfers are kept for **24 hours**
   (partial files on the receiver, queued jobs on the desktop), then deleted.

## Clipboard
`clipboard { "id", "mime", "size", "part", "parts", "data" }`: a copied text (`text/plain`,
UTF-8) or image (`image/png|jpeg|webp|gif`), at most 12 MiB, split into parts of 512 KiB raw
bytes; `data` is standard base64 of one part, so every frame stays under 1 MiB. Parts are sent
in order; `id` is the first 16 hex chars of SHA-256(mime ‖ 0x00 ‖ data). The receiver checks
`parts` = ⌈size / 512 KiB⌉, each part's exact length, and the total, and drops anything
inconsistent. A device never sends back content it just received (same SHA-256).
Desktop copies are read through the X11/XWayland clipboard (works on GNOME, KDE and X11);
password-manager copies (`x-kde-passwordManagerHint`) and copied files are not sent.
Android 10+ only lets the focused app read the clipboard. The phone detects copies with an
optional accessibility service and reads them through a momentary invisible focusable window
(ADR-0016); without it, the user taps "Send clipboard" (notification, Quick Settings tile) or
shares text to Continuum.

File names from the peer are sanitized: base name only, no control characters, no leading
dots, at most 255 bytes, never `.` or `..`.
