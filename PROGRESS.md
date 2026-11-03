# Progress

> Read this first in every session. Update it before ending a session.

## Current
- **Milestone:** v0.5.0 (3 November 2026): Phone Link-style app (photos/videos, messages,
  notifications with quick reply, phone storage, ring, files-to-clipboard, GNOME Quick Settings
  tile), website on GitHub Pages (`website/`, `.github/workflows/pages.yml`), prebuilt desktop
  bundle in releases.
- **Next task:** phone storage downloads over a raw data connection (like sends) instead of
  base64 parts: S24 Ultra measured 57 MB/s PC->phone but 36 MB/s phone->PC. Then original
  device drawings per phone family (Samsung 2020+, Xiaomi, Redmi, Oppo...), matched by model
  name; official photos stay local-only (copyright). Then, in order of value:
  1. Notification allowlist (per-app mute, from PC and phone); hide content while locked.
  2. Photo viewer next/previous; message search.
  3. Calls (M8), then phone mirroring (M9).
  4. Leftovers: MIUI watchdog; Android 8-9 TLS; 6-digit pairing; re-announce after suspend;
     transfer speed plan (`docs/benchmarks/transfer-speed.md`).
- **Known limits:** Ring can lift the phone's Do Not Disturb only up to Android 14 (Android 15+
  forbids it; alarms still ring through the default "Priority" DND, not "Total silence").
  Android won't let an app hide its own foreground notification: "In the notification shade" off
  minimises it and links to Android settings to switch it off.
- **Testing aids:** `CONTINUUM_SNAPSHOT=/x.png continuum-ui` saves window pictures every 2 s
  (`/x.reply.png` for the quick-reply window); app actions `show-page`, `select-device`,
  `quick-reply`, `send-files`, `present` over `org.gtk.Actions`. The Quick Settings tile was tested
  in a nested `gnome-shell --devkit` with isolated XDG dirs and a D-Bus bridge.
- **Emulator note:** the emulator crashed once in its GPU layer (gfxstream) and restored an old
  snapshot; a snapshot `pre_ring` exists now.

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
| M7 | Notifications (allowlist, quick reply) | 🟡 mirror, popup Reply, dismiss done; allowlist next |
| M8 | Calls (iOS-style banner, accept/decline) | ☐ |
| M9 | Phone mirroring (borderless rounded window) | ☐ |
| M10 | Settings, shell tile, find phone, media | ☐ |
| M10b | KDE Plasma support (Dolphin menu, tray widget) | ☐ |
| M11 | Hardening and release | ☐ |

## Log (newest first)
- 3 November 2026: v0.5.1. In-app video (GStreamer gtk4paintablesink, falls back to the video player), grid multi-select with right-click Copy/Save (CopyFiles D-Bus method, real file names), 480 px thumbnails, resizable Messages split, contact photos, battery watts/fast/time-to-full with one or two bolts, desktop pings every 3 s (9 s timeout), notification listener rebind, shade switch opens the Android channel switch. Found: the emulator's clipboard sharing overwrote the PC clipboard with empty text (now off in Emulator.conf). Real Redmi: 0.5.0 update, permissions, lock-screen switch, multi-screenshot share to clipboard confirmed by the owner.
- 3 November 2026: v0.5.0. Phone storage browser, videos (streamed to disk, up to 4 GiB), Ring
  (alarm stream, max volume), quick reply from desktop popups, received files on the clipboard
  (gnome-copied-files, uri-list, image, paths), GNOME Quick Settings extension, Apple-style
  battery/status, square/whole-photo grid and EXIF-upright viewer, connection-notification switches,
  website + desktop bundle in releases. Fixed: Android 15+ DND mode left on by Ring (now skipped,
  leftover cleared).
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
