# Progress

> Read this first in every session. Update it before ending a session.

## Current
- **Milestone:** M0. Toolchains and skeletons (see `docs/ROADMAP.md`)
- **Next task:** write `scripts/setup-dev.sh` (rustup, `libgtk-4-dev`, `libadwaita-1-dev`,
  GStreamer dev, Android cmdline-tools/SDK 36 and build-tools), run it, then create the Cargo
  workspace skeleton in `desktop/`.

## Milestones
| # | Milestone | Status |
|---|-----------|--------|
| M0 | Toolchains and skeletons | ⏳ next |
| M1 | Protocol core (desktop) | ☐ |
| M2 | Protocol core (Android) | ☐ |
| M3 | Pairing (QR + SAS) | ☐ |
| M4 | Send files phone → PC (share sheet, device icon) | ☐ |
| M5 | Send files PC → phone (Nautilus right-click) | ☐ |
| M6 | Universal clipboard | ☐ |
| M7 | Notifications (allowlist, quick reply) | ☐ |
| M8 | Calls (iOS-style banner, accept/decline) | ☐ |
| M9 | Phone mirroring (borderless rounded window) | ☐ |
| M10 | Settings, shell tile, find phone, media | ☐ |
| M11 | Hardening and release | ☐ |

## Log (newest first)
- 29 October 2026: project started. Stack decided (Rust + GTK4 desktop, Kotlin/Compose Android,
  Shizuku for clipboard, GPL-3.0). Roadmap, architecture and protocol draft written.
