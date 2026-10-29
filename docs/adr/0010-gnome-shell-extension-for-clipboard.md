# ADR-0010: GNOME Shell extension for desktop clipboard on Wayland

- **Status:** Accepted
- **Date:** 29 October 2026

## Context
On Wayland, only the focused window can read the clipboard. GNOME's compositor (Mutter) doesn't
support the `wlr-data-control` protocol that other compositors offer for clipboard managers, so
a background daemon can't see clipboard changes.

## Decision
Ship a small **GNOME Shell extension** that watches the clipboard (Shell code is allowed to) and
forwards changes to `continuumd` over D-Bus, and sets the clipboard when the phone sends
something. The same extension provides the quick-settings tile (FR-SET-2).

## Alternatives considered
- **Polling with a hidden window**: doesn't work on Wayland without focus.
- **X11-only support**: Wayland is the default on GNOME.
- **wlr-data-control**: not available on Mutter.

## Consequences
- Good: automatic clipboard sync on GNOME Wayland, the main target.
- Cost: Shell extensions must be updated for each GNOME release. The extension stays tiny (no
  logic beyond bridging) to keep that cheap. Other desktops (KDE, wlroots) can use their own
  clipboard APIs in the daemon later.
