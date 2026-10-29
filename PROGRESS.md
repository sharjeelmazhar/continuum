# Progress

> Read this first in every session. Update it before ending a session.

## Current
- **Milestone:** M4 ✅ (send files phone → PC). Next: M5 (PC → phone, Nautilus "Send to") and
  M6 (universal clipboard).
- **Next task:** M5. Leftovers from M2/M3, in order of value:
  1. MIUI: after a crash or an update, the service isn't restarted until the app is opened
     (MIUI's background broadcast queue was 400+ deep). Consider WorkManager as a watchdog.
  2. Android 8-9: bundle Conscrypt for TLS 1.3 (`net/Tls.kt` TODO); test on an API 26 image.
  3. Pairing without a camera (6-digit code), and re-announce after the PC wakes from suspend.
  4. Battery measurement on the real phone (ping every 20 s).
- **Verified:** real share sheet on the emulator ("Send to MDPT" is the first Direct Share
  entry): 103.5 MB in 18 s, byte-identical; PC service restart mid-transfer resumes; real phone
  (Redmi Note 11) stays connected and reconnects after PC service restarts.

## Milestones
| # | Milestone | Status |
|---|-----------|--------|
| M0 | Toolchains and skeletons | ✅ |
| M1 | Protocol core (desktop) | ✅ |
| M2 | Protocol core (Android) | 🟡 Android 10+ done; 8–9 next |
| M3 | Pairing (QR + SAS) | 🟡 QR done; SAS next |
| M4 | Send files phone → PC (share sheet, device icon) | ✅ |
| M5 | Send files PC → phone (Nautilus right-click) | ☐ |
| M6 | Universal clipboard | ☐ |
| M7 | Notifications (allowlist, quick reply) | ☐ |
| M8 | Calls (iOS-style banner, accept/decline) | ☐ |
| M9 | Phone mirroring (borderless rounded window) | ☐ |
| M10 | Settings, shell tile, find phone, media | ☐ |
| M10b | KDE Plasma support (Dolphin menu, tray widget) | ☐ |
| M11 | Hardening and release | ☐ |

## Log (newest first)
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
