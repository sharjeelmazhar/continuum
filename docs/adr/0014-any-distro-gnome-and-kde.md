# ADR-0014: Any distro; GNOME first, KDE Plasma as a supported second desktop

- **Status:** Accepted
- **Date:** 29 October 2026

## Context
Users run many distros (Ubuntu, Fedora, Arch, openSUSE, Mint…) and two main desktops: GNOME and
KDE Plasma. Building and testing for every combination isn't realistic for a small project,
but excluding KDE users or non-Ubuntu users would shrink the audience a lot.

## Decision
1. **Distro-neutral core.** `continuumd` uses only things every modern distro has: the network,
   D-Bus, systemd user services. Nothing in it is Ubuntu- or GNOME-specific (NFR-MNT-5).
2. **Flatpak is the main package** (on Flathub), because one Flatpak runs on every distro.
   Native packages come later where cheap: `.deb`, `.rpm`, and an AUR recipe for Arch.
3. **Desktop add-ons are thin and separate.** Each desktop gets small add-ons that only talk
   D-Bus to the daemon:

| Need | GNOME | KDE Plasma |
|---|---|---|
| Main app | GTK4/libadwaita app | Same app (runs fine on Plasma, with a GNOME look) |
| Background clipboard on Wayland | Shell extension (ADR-0010) | Built into the daemon: KWin supports the data-control protocol, so no add-on is needed |
| "Send to phone" in the file manager | Nautilus extension | Dolphin service menu (one small `.desktop` file) |
| Status / quick actions | Quick-settings tile (Shell extension) | Plasma widget (system tray) |
| Notifications, call banner | Standard freedesktop notifications + our banner window | Same |

4. **Support levels:** GNOME is tested every milestone. KDE Plasma gets its add-ons in a
   dedicated milestone (M10b) and is then tested before each release. Other desktops (XFCE,
   Cinnamon, wlroots compositors) get the core features without the add-ons, with community help welcome.

## Alternatives considered
- **Ubuntu/GNOME only**: simplest, but loses most potential users.
- **Separate Qt app for KDE**: twice the UI work. The GTK app works on Plasma, so a native Qt UI is not worth it for now.
- **Snap or AppImage as the main package**: Snap is mostly Ubuntu-centric. AppImage has no
  sandbox or auto-update. Flatpak is the cross-distro standard for desktop apps.

## Consequences
- Good: one codebase reaches every distro, and KDE support costs only a few small add-ons.
- Cost: a Flatpak can't install GNOME Shell or Nautilus extensions by itself. Users get those from
  extensions.gnome.org or a native package, and the app shows a guided one-click step for this
  (NFR-USE-4). The Flatpak needs permissions for the network, owning its D-Bus name, and
  autostart at login (via the Background portal).
