# Progress

> Read this first in every session. Update it before ending a session.

## Current
- **Milestones:** M1 ✅, M2 and M3 mostly done (see below). Next is M4 (send files phone → PC).
- **Next task:** finish M2/M3 leftovers, then M4.
  1. Android 8–9: bundle Conscrypt for TLS 1.3 (`net/Tls.kt` TODO), and test on an API 26 emulator image.
  2. Pairing fallback without a camera (6-digit code, SAS) from PROTOCOL/06-security.
  3. Re-announce after the PC wakes from suspend (logind PrepareForSleep) and on desktop network changes.
  4. Measure battery on the real phone (ping every 20 s): `adb shell dumpsys batterystats`.
- **Tested on the emulator** (`continuum-test`, Android 17) against the installed daemon: pairing,
  confirm dialog for outside links, local-network permission prompt, reconnect after Wi-Fi off/on
  (0.2 s), PC service restart (1.1 s), PC off then on with wake nudge (0.6 s), app process killed
  (2.5 s), phone reboot without opening the app, removal from either side.
  Not testable on the emulator: mDNS discovery and the UDP nudge over real Wi-Fi (emulator NAT);
  verify on the real phone.
- **Known issues:** the emulator delivers BOOT_COMPLETED ~25 s late (system queue, not us).

## Milestones
| # | Milestone | Status |
|---|-----------|--------|
| M0 | Toolchains and skeletons | ✅ |
| M1 | Protocol core (desktop) | ✅ |
| M2 | Protocol core (Android) | 🟡 Android 10+ done; 8–9 next |
| M3 | Pairing (QR + SAS) | 🟡 QR done; SAS next |
| M4 | Send files phone → PC (share sheet, device icon) | ☐ |
| M5 | Send files PC → phone (Nautilus right-click) | ☐ |
| M6 | Universal clipboard | ☐ |
| M7 | Notifications (allowlist, quick reply) | ☐ |
| M8 | Calls (iOS-style banner, accept/decline) | ☐ |
| M9 | Phone mirroring (borderless rounded window) | ☐ |
| M10 | Settings, shell tile, find phone, media | ☐ |
| M10b | KDE Plasma support (Dolphin menu, tray widget) | ☐ |
| M11 | Hardening and release | ☐ |

## Log (newest first)
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
