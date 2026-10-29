# Roadmap

This is a long-term project with no deadline. Work happens in short sessions whenever usage allows,
and new features get added over time. Each milestone is small enough for one or two chat sessions,
and each one ends with working, tested, committed code. The order puts something useful first (M4: files from the phone to the PC), and everything else builds on top. Nothing here is a schedule.

Legend: `D` = desktop (Rust/GTK), `A` = Android (Kotlin/Compose).

## Foundation and the first useful feature

### M0. Toolchains and skeletons
- Install rustup (stable), the GTK4/libadwaita dev packages, the Android SDK command-line tools
  and a Gradle wrapper. Write `scripts/setup-dev.sh` so anyone can repeat it.
- D: Cargo workspace with `continuum-proto` (lib), `continuumd` (daemon) and `continuum-ui` (GTK app).
- A: Gradle project `app/` (Compose, Material 3, minSdk 26), with a debug build that installs over adb.
- GitHub Actions CI: cargo fmt/clippy/test, and Android assembleDebug plus unit tests.

### M1. Protocol core (D)
- Device identity: an Ed25519 key and a self-signed X.509 cert, generated on first run and stored
  `0600` in `~/.local/share/continuum/`.
- mDNS advertise and browse for `_continuum._tcp`.
- TLS 1.3 server and client (rustls) with a custom verifier that pins the peer certificate.
- Framed messages (see PROTOCOL.md), a 1 MiB control-frame limit, and a version handshake.
- Tests: framing fuzz/proptest, a handshake between two in-process peers, and rejection of an unpinned cert.

### M2. Protocol core (A)
- The same identity and pinning on Android (Keystore-backed key, OkHttp/Conscrypt TLS 1.3 sockets).
- NsdManager discovery and a foreground service ("connectedDevice" type) that stays alive.
- Interop test: the Android client against `continuumd` on the LAN.

### M3. Pairing (D + A)
- D: a "Pair a phone" window (libadwaita) that shows a QR code with host, port, cert fingerprint
  and a one-time secret.
- A: a CameraX QR scanner that connects, checks the fingerprint and proves the secret. Both sides
  store the pinned peer.
- Fallback: pick the device from a discovered list and confirm a 6-digit code shown on both
  screens (SAS derived from both fingerprints).
- Unpair and rename, plus a device type and icon (phone, tablet, laptop, desktop) exchanged at
  pairing time.

### M4. Send files: phone → PC (the AirDrop moment)
- A: **Direct Share target**. Each paired PC shows up in the Android share sheet as its own
  entry, with the PC's name and a laptop or desktop icon (ShortcutManager sharing shortcuts).
  Supports multiple files.
- Transfer channel: chunked, SHA-256 verified, resumable after drops or power loss, and
  parallel files.
- D: an iOS-style progress toast, then the file lands in `~/Downloads/Continuum/` with
  "Open" and "Show in folder" actions.
- Tests: resume after a killed connection, hostile file names (`../`, NUL, very long names), and 4 GB files.

## Daily-driver features

### M5. Send files: PC → phone
- D: a Nautilus extension with a right-click entry "Send to ⟨Phone name⟩" (one per paired device,
  with its icon), plus drag-and-drop onto the Continuum window.
- A: received files go to `Download/Continuum` (MediaStore) with a notification that opens them.

### M6. Universal clipboard
- Text and images, both directions, sent automatically and end-to-end over the paired channel.
- A: Shizuku or a one-time `adb shell` grant lets the app see clipboard changes in the
  background. Without it, a fallback Quick Settings tile appears. An in-app setup guide covers this.
- D: on GNOME Wayland, background clipboard access needs a small **GNOME Shell extension**
  (Mutter has no wlr-data-control). It bridges to `continuumd` over D-Bus.
- Loop prevention (content hash and origin id), a size cap, and an "ignore password managers" option.

### M7. Notifications
- A: NotificationListenerService that forwards **only allowlisted apps** (picker UI with app
  icons). WhatsApp-style quick reply via RemoteInput. Dismissing on one side dismisses on both.
- D: native GNOME notifications with the app icon, plus the same allowlist editable from the PC.
- Privacy: the allowlist is empty by default, and the phone can hide notification content while it's locked.

### M8. Calls
- A: detect ringing via TelephonyManager callbacks, and look up the caller name from contacts.
- D: an **iOS-style incoming-call banner** (rounded, translucent, avatar, with Accept and Decline
  buttons) as a custom GTK layer, not a plain notification.
- Accept or decline with `TelecomManager` (ANSWER_PHONE_CALLS). Mute the ringer from the PC.
  Missed-call notifications.
- Note: call **audio** stays on the phone (or a Bluetooth headset). Routing call audio to the PC
  is a stretch goal (see M12).

## Wow features and polish

### M9. Phone mirroring (like iPhone Mirroring)
- A: screen capture with MediaProjection, or a scrcpy-style privileged capture via Shizuku
  (no "start casting" prompt each time), encoded H.264/H.265 with MediaCodec at low latency.
- D: a **borderless window with rounded corners shaped like the phone**, no title bar. Controls
  (close, rotate, screenshot, home/back) fade in on hover. GStreamer decoding with VA-API
  hardware acceleration.
- Input: mouse and keyboard on the PC control the phone (via Shizuku, injected like scrcpy).
- Target latency under 60 ms on the LAN. Opens from the menu or the shell extension.

### M10. Desktop integration and settings
- Continuum settings app (libadwaita): devices, per-feature toggles, notification allowlist,
  download folder.
- GNOME Shell quick-settings tile: phone battery, connection state, and one-tap "send clipboard"
  and "mirror".
- Find my phone (ring at full volume), media controls (play/pause/next from the PC), battery
  low alerts.
- Autostart via a systemd user service.

### M11. Hardening and release
- Security pass: cargo-audit, fuzzing the frame parser (cargo-fuzz), and a threat-model write-up.
- Battery: a guide for MIUI/HyperOS/Samsung autostart and battery settings, with in-app deep links.
- Packaging: `.deb`, Flatpak, a signed APK on GitHub Releases, and later F-Droid.
- README screenshots and a demo GIF.

## Later ideas (M12+, open-ended: add new ideas here)
- **Continuity Camera and mic**: use the phone as the PC's webcam and microphone (v4l2loopback
  and a PipeWire virtual source).
- SMS from the PC (read and reply).
- Handoff: open the phone's current browser URL on the PC, and the other way round.
- Shared keyboard and mouse: move the cursor off the screen edge onto the phone.
- Call audio over the PC's speakers and mic.
- Hotspot: one-click tethering.
- Other desktops (KDE Plasma, wlroots) and more than one phone per PC.
