# ADR-0004: Headless daemon, UIs talk to it over D-Bus

- **Status:** Accepted
- **Date:** 29 October 2026

## Context
Several desktop parts need the phone connection: the main app, the Nautilus "Send to" menu, the
GNOME Shell extension (clipboard, quick-settings tile), and later the call banner and mirroring
window. Connections must stay up when the app window is closed.

## Decision
One background program, **`continuumd`**, owns all connections, keys and transfers. It has no UI.
Everything else is a client that talks to it over the **D-Bus session bus**
(`org.continuum.Daemon`). systemd starts it at login.

## Alternatives considered
- **Everything in the GTK app**: closing the window would drop the phone. Nautilus and the Shell
  extension would have nothing to talk to.
- **Unix socket with our own protocol**: works, but D-Bus is already how GNOME components talk,
  has introspection and signals, and is callable from Python (Nautilus) and JS (Shell).

## Consequences
- Good: the UI can crash or close without affecting transfers. Each client is small and optional.
  KDE or other front-ends could be added later.
- Cost: a D-Bus API to design and keep stable. It's versioned like the wire protocol.
- Only the local user's session can reach the daemon, so no extra auth is needed for D-Bus.
