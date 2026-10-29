# Architecture Decision Records (ADRs)

An ADR is a short note that records **one important decision**: what we chose, why, and what
we gave up. Big companies keep them so that, months later, nobody has to guess why something
is the way it is, and nobody reverses a decision without knowing the reasons.

## Rules
- One decision per file: `NNNN-short-title.md`, numbered in order and never renumbered.
- Status is `Proposed`, `Accepted`, `Superseded by ADR-XXXX` or `Deprecated`.
- Don't edit an accepted ADR's decision. To change it, write a new ADR that supersedes it.
- Use [`template.md`](template.md).

## Index
| # | Decision | Status |
|---|---|---|
| [0001](0001-record-architecture-decisions.md) | Record decisions as ADRs | Accepted |
| [0002](0002-desktop-rust-gtk4.md) | Desktop in Rust with GTK4 + libadwaita | Accepted |
| [0003](0003-android-kotlin-compose.md) | Android in Kotlin + Jetpack Compose, minSdk 26 | Accepted |
| [0004](0004-headless-daemon-dbus.md) | Headless daemon, UIs talk to it over D-Bus | Accepted |
| [0005](0005-tls13-pinned-self-signed-certs.md) | TLS 1.3 with pinned self-signed certificates | Accepted |
| [0006](0006-length-prefixed-json-control-frames.md) | Length-prefixed JSON for control messages | Accepted |
| [0007](0007-separate-data-connections.md) | Separate connections for bulk data | Accepted |
| [0008](0008-plugin-per-feature.md) | One plugin per feature, negotiated by capabilities | Accepted |
| [0009](0009-shizuku-for-phone-clipboard.md) | Shizuku / ADB grant for background clipboard on Android | Accepted |
| [0010](0010-gnome-shell-extension-for-clipboard.md) | GNOME Shell extension for desktop clipboard on Wayland | Accepted |
| [0011](0011-own-port-and-mdns-service.md) | Own port and mDNS service, coexist with KDE Connect | Accepted |
| [0012](0012-gpl3-license.md) | GPL-3.0-or-later | Accepted |
