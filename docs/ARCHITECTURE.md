# Architecture

```
┌──────────────── Linux desktop ────────────────┐        LAN (TLS 1.3, pinned certs)        ┌──────── Android ────────┐
│ continuum-ui (GTK4/libadwaita)                 │                                           │ Compose UI               │
│   pairing, settings, call banner, mirroring    │                                           │   pairing, settings      │
│        │ D-Bus (session bus)                   │   control conn: framed JSON messages      │ ContinuumService         │
│ continuumd (Rust, tokio) ──────────────────────┼──────────────────────────────────────────►│  (foreground service)    │
│   discovery · sessions · transfers · clipboard │   data conns: file chunks, video stream   │  plugins: share, clip,   │
│        ▲ D-Bus                                 │◄──────────────────────────────────────────┼  notif, call, mirror     │
│ nautilus-continuum (Python ext)                │                                           │ Shizuku bridge (opt.)    │
│ gnome-shell extension (JS: clipboard, tile)    │                                           └──────────────────────────┘
└────────────────────────────────────────────────┘
```

## Repo layout
```
desktop/                 Cargo workspace
  continuum-proto/       wire types, framing, crypto, pairing (no I/O beyond traits); heavily tested
  continuumd/            daemon: mDNS, TLS sessions, plugins, D-Bus API (zbus)
  continuum-ui/          GTK4/libadwaita app (pairing, settings, call banner, mirroring window)
  nautilus/              Nautilus Python extension: "Send to <device>"
  shell-extension/       GNOME Shell extension (clipboard bridge, quick settings tile)
android/                 Gradle project (Kotlin, Compose, Material 3)
  app/                   UI + ContinuumService + plugins
docs/                    ROADMAP, ARCHITECTURE, PROTOCOL
scripts/                 setup-dev.sh, run helpers
```

## Principles
- **Plugin per feature** on both sides (share, clipboard, notifications, calls, mirror, battery...).
  A plugin owns its message types (`ns.action`) and can be disabled per device.
- **The daemon is headless.** Every UI talks to it over D-Bus (`org.continuum.Daemon`), so the
  daemon keeps running when the UI is closed, and any UI is optional.
- **Separate channels**: one control connection per peer. Bulk data (files, video) opens extra
  TLS connections that authenticate with the same pinned certs, so a large transfer never blocks
  a call notification.
- **Speed**: zero-copy chunking, parallel transfers, and hardware video encode/decode.
- **Resilience**: auto-reconnect with backoff, resumable transfers, and atomic writes.

## Key libraries
Desktop: tokio, rustls, rcgen, ed25519-dalek, mdns-sd, zbus, serde/serde_json, gtk4-rs,
libadwaita-rs, gstreamer-rs, qrcode, proptest.
Android: Kotlin coroutines, Jetpack Compose, CameraX + ML Kit barcode (or ZXing), Shizuku API,
MediaCodec/MediaProjection, AndroidX Room/DataStore.
