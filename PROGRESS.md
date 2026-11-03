# Progress

> Read this first in every session. Update it before ending a session.

## Current
- **Milestone:** Phone Link-style app (photos, messages, notifications = M7 + part of M12 "SMS"),
  built 3 November 2026. v0.5.0 not tagged yet.
- **Next task:** test the new features on the real Redmi (MIUI: restricted settings for SMS and
  notification access, battery killing), then tag v0.5.0. Then, in order of value:
  1. Desktop: photo viewer next/previous and multi-select "Save", message search, contact photos.
  2. Notification allowlist (per-app mute, from PC and phone), and hide content while locked.
  3. Calls (M8), then phone mirroring (M9).
  4. Leftovers: MIUI service restart watchdog; Android 8-9 TLS (Conscrypt); 6-digit pairing
     without a camera; re-announce after suspend; battery measurement; transfer speed plan in
     `docs/benchmarks/transfer-speed.md` (re-run: `scripts/bench-transfer.sh`).
- **Verified on the emulator (Android 17) with the daemon on the PC, 3 November 2026:** photo grid
  (18 photos, thumbnails, full-size download), SMS threads and conversation, sending an SMS from
  the PC, incoming SMS → live update + GNOME popup (once, not per repost), notification list with
  app icons, quick reply through Google Messages, dismiss. Dev aids: `CONTINUUM_SNAPSHOT=/x.png
  continuum-ui` saves the window every 2 s; app actions `show-page` / `select-device` over
  `org.gtk.Actions` on the session bus.
- **Verified on the real phone earlier:** PC → phone 150 MB byte-identical; Wi-Fi off mid-transfer →
  resumed; clipboard both ways.

## Milestones
| # | Milestone | Status |
|---|-----------|--------|
| M0 | Toolchains and skeletons | ✅ |
| M1 | Protocol core (desktop) | ✅ |
| M2 | Protocol core (Android) | 🟡 Android 10+ done; 8–9 next |
| M3 | Pairing (QR + SAS) | 🟡 QR done; SAS next |
| M4 | Send files phone → PC (share sheet, device icon) | ✅ |
| M5 | Send files PC → phone (Nautilus right-click) | ✅ |
| M6 | Universal clipboard | ✅ (real-phone check of auto copy pending) |
| M7 | Notifications (allowlist, quick reply) | 🟡 mirror, reply, dismiss done; allowlist next |
| M8 | Calls (iOS-style banner, accept/decline) | ☐ |
| M9 | Phone mirroring (borderless rounded window) | ☐ |
| M10 | Settings, shell tile, find phone, media | ☐ |
| M10b | KDE Plasma support (Dolphin menu, tray widget) | ☐ |
| M11 | Hardening and release | ☐ |

## Log (newest first)
- 3 November 2026: Phone Link-style desktop app: sidebar with the phone (picture, battery) and
  Photos (lazy grid, disk-cached thumbnails, viewer with copy/save/open), Messages (threads,
  bubbles, send, new conversation), Notifications (icons, quick reply, actions, dismiss, clear all),
  Send Files (drop anywhere). Daemon: generic request/reply routing (`Request`, `FetchPhoto`,
  `ListNotifications`, `NotificationCommand`, `PhoneEvent` on D-Bus), GNOME popups for phone
  notifications that close when dismissed on the phone. Android: MediaStore, SMS (read/send,
  contact names, change observer), NotificationListenerService, "On your computer" permission
  section. Fixed: a pairing link was asked again after an app update.
- 30 October 2026: in-app updater (GitHub release list → SHA-256-checked download →
  PackageInstaller session; Android 12+ later updates can skip the prompt). Verified on the
  emulator: 0.4.0 updated itself to 0.4.1. Distribution decided: GitHub only, no Play Store. v0.4.2.
- 29 October 2026: automatic phone → PC clipboard (accessibility copy watcher + 1-px focus
  window, ADR-0016) instead of Shizuku. Emulator (Android 17): 3/3 copies on the PC in < 1 s.
  v0.4.1.
- 29 October 2026: M5 + most of M6. PC → phone files (D-Bus SendFiles, `continuumd send`,
  Files/Dolphin right-click, app button; phone saves to Downloads/Continuum, resumable).
  Transfer ids derived from the files, so re-sharing resumes; unfinished transfers kept 24 h.
  Clipboard text + images both ways (X11/XWayland watcher on the desktop). Owner's logo and
  AirDrop-style device pictures (`scripts/render-device-icons.py`). Progress notifications with
  size and speed; one-tap "Hide" for the connection notification. v0.4.0.
- 29 October 2026: M4 done. Direct Share target per PC, resumable verified transfers
  (8 MiB checkpoints, SHA-256, safe names, disk-space check), GNOME notifications with Open /
  Show in Files. Fixed: TLS session tickets caused resets on close; crash on unreadable shares;
  MIUI Autostart card detection. v0.3.0.
- 29 October 2026: first working connection. Desktop: TLS 1.3 pinned server, QR pairing,
  D-Bus API, GTK device list, mDNS, UDP wake nudge, systemd install. Android: Keystore identity,
  pinned TLS, foreground service, QR scanner, auto-reconnect, LAN discovery fallback, Android 17
  local-network permission. 36 Rust + 9 Kotlin tests, all edge cases above verified. v0.2.0.
- 29 October 2026: v0.1.0 developer preview published on GitHub Releases (signed APK, built by
  `release.yml` on tag push; process in `docs/design/07-build-test-ci.md`).
- 29 October 2026: published to GitHub (public, CI green on both jobs). App ID fixed as
  `io.github.sharjeelmazhar.Continuum` (ADR-0013). Commits use the GitHub noreply email.
- 29 October 2026: M0 done. setup-dev.sh run with apt (JDK 21 is needed for Gradle). Desktop
  workspace builds (ui launches). Docs: SRS, design docs 00-07 with Mermaid diagrams, ADRs
  0001-0012. Android skeleton (AGP 9.5, Kotlin 2.4, Compose BOM 2026.09, compile/target 37,
  min 26) builds, passes tests and lint, and runs on the `continuum-test` emulator.
  `scripts/emulator.sh` and GitHub Actions CI added.
- 29 October 2026: project started. Stack decided (Rust + GTK4 desktop, Kotlin/Compose Android,
  Shizuku for clipboard, GPL-3.0). Roadmap, architecture and protocol draft written.
