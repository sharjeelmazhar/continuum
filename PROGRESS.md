# Progress

> Read this first in every session. Update it before ending a session.

## Current
- **Milestone:** M1. Protocol core, desktop (see `docs/ROADMAP.md`, design: `docs/design/01-continuum-proto.md`)
- **Next task:** M1 part 1 in `continuum-proto`: `message` (Envelope), `frame` (length-prefixed
  decoder with the 1 MiB limit), `filename::sanitize`, `caps::negotiate`, with unit tests and
  proptest. Part 2: `identity` + pinned `verify` (rustls). Part 3: TLS + mDNS in `continuumd`
  with an in-process two-peer test.
- **Known issues:** Android placeholder window theme gives white status-bar icons on a light
  background (fix with the real theme in M2).

## Milestones
| # | Milestone | Status |
|---|-----------|--------|
| M0 | Toolchains and skeletons | ✅ |
| M1 | Protocol core (desktop) | ⏳ next |
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
- 29 October 2026: published to GitHub (public, CI green on both jobs). App ID fixed as
  `io.github.sharjeelmazhar.Continuum` (ADR-0013). Commits use the GitHub noreply email.
- 29 October 2026: M0 done. setup-dev.sh run with apt (JDK 21 is needed for Gradle). Desktop
  workspace builds (ui launches). Docs: SRS, design docs 00-07 with Mermaid diagrams, ADRs
  0001-0012. Android skeleton (AGP 9.5, Kotlin 2.4, Compose BOM 2026.09, compile/target 37,
  min 26) builds, passes tests and lint, and runs on the `continuum-test` emulator.
  `scripts/emulator.sh` and GitHub Actions CI added.
- 29 October 2026: project started. Stack decided (Rust + GTK4 desktop, Kotlin/Compose Android,
  Shizuku for clipboard, GPL-3.0). Roadmap, architecture and protocol draft written.
