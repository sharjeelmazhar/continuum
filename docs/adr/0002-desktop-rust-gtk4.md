# ADR-0002: Desktop in Rust with GTK4 + libadwaita

- **Status:** Accepted
- **Date:** 29 October 2026

## Context
The desktop side parses untrusted network data all day, moves large files fast, and must look
native on GNOME. It also runs permanently in the background, so memory and CPU use matter.

## Decision
Write the desktop in **Rust**: tokio (async I/O), rustls (TLS), mdns-sd (discovery), zbus
(D-Bus). Write the UI with **GTK4 + libadwaita** through gtk4-rs, the toolkit GNOME's own apps use.

## Alternatives considered
- **Python + GTK** (like GSConnect's helpers): fast to write, but slower, with weaker typing,
  and memory-safety problems surface only at runtime. Not ideal for a hostile-input parser.
- **C / Vala**: native to GNOME, but no memory safety.
- **Electron / Tauri**: doesn't look native on GNOME, and uses a lot of memory.
- **Go**: good networking, but GTK4 bindings are immature.

## Consequences
- Good: memory safety for the parser and TLS code (`unsafe` is forbidden in the workspace).
  Small, fast binaries. Native GNOME look.
- Cost: Rust is slower to write. gtk4-rs needs some GObject knowledge for custom widgets.
- The daemon doesn't depend on GTK, so it can run headless and with other desktops later.
