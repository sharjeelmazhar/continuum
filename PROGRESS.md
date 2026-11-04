# Progress

> Read this first in every session. Update it before ending a session.

## Current
- **Milestone:** v0.5.10 (4 November 2026): Phone Mirroring (M9: own app and dock icon,
  borderless rounded window, live mouse/keyboard control, Mirror tile on the phone, Mirror Phone
  in GNOME Quick Settings) and Banking mode, on top of the v0.5 Phone Link-style app. Website on
  GitHub Pages (`website/`), desktop bundle + deb/rpm/Arch + apt/dnf/pacman repos per release.
- **How to release:** bump `desktop/Cargo.toml` version, write `.github/release-notes/vX.Y.Z.md`,
  push main, wait for CI, then push tag `vX.Y.Z`. The Release workflow builds the signed APK
  (versionCode = major*10000+minor*100+patch), bundle and packages, and triggers Pages for the
  repos and site. Local test builds: `assembleRelease -PversionCode=… -PversionName=…` with the
  key env vars (see CLAUDE.local.md); deb: first 41 lines of packaging/build-packages.sh.
- **Distro test (resumable):** serve a local site first (`/data2/vms/continuum-test/site`, built with packaging/build-packages.sh + build-repos.sh + website/install.sh; `python3 -m http.server 8000 --bind 192.168.122.1 -d /data2/vms/continuum-test/site`), then `VM_DIR=/data2/vms KEEP=1 SITE=http://192.168.122.1:8000 scripts/test-distros.sh`; after release, without SITE (fedora, arch, ubuntu GNOME VMs; reruns skip finished steps; results in /data2/vms/continuum-test/<distro>/result.txt).
- **Next task:** `phone-ping` (owner's helper in ~/.local/bin) sends through GSConnect, which is
  no longer installed, so pings fail silently: add a desktop → phone "show this notification"
  message to Continuum (e.g. `notify.show`, rate-limited, plain text) and point phone-ping at
  it. Then M9 follow-ups: confirm Banking mode with a second banking app, a live drag test of
  the mirror resize grips, test on a Samsung phone, measure end-to-end mirror latency. Then:
  VA-API decoding; a desktop "Set up banking mode" button that runs the one-time adb grant.
  After that: measure raw downloads on a fast phone (S24 Ultra); check continuum-ui/src/models.rs
  against real devices. Then, in order of value:
  1. Notification allowlist (per-app mute, from PC and phone); hide content while locked.
  2. Photo viewer next/previous; message search.
  3. Calls (M8).
  4. Leftovers: MIUI watchdog; Android 8-9 TLS; 6-digit pairing; re-announce after suspend;
     transfer speed plan (`docs/benchmarks/transfer-speed.md`).
- **Mirroring facts (for debugging):** phone → desktop stream is length-prefixed H.264 on a data
  connection (docs/PROTOCOL.md "Screen mirroring"). Desktop debug: `CONTINUUM_MIRROR_DEBUG=1
  continuum-ui --mirror <id>` (fps, Mbit/s, skipped, and why a stream ended). Phone debug:
  `adb shell setprop log.tag.Continuum DEBUG`, then `adb logcat --pid=<pid> -s Continuum`
  (YouTube floods the shared log, so filter by pid live). Video apps take the hardware encoder
  (CodecException 0xffffffe0): the phone restarts smaller (REDUCED_MAX 1600), then in software
  (SOFTWARE_MAX 1280), full size again every 30 s. On the test Redmi the largest 60 fps size is
  832x1850. Xiaomi phones ask on screen to confirm every `adb install` (owner must tap).
- **Owner preferences learned:** auto-rotate stays off and the phone's Quick Settings order must
  not change (never `cmd statusbar add-tile` on the real phone; check `mCurrentRotation` before
  any `input tap`); no coloured icon tiles (plain symbolic icons, Continuum's own set in
  desktop/data/icons/hicolor/scalable/actions); settings show only what still needs doing
  (except remote control, always shown); explanations as formatted dialogs.
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
| M9 | Phone mirroring (borderless rounded window) | ✅ v0.5.9-0.5.10 (latency measurement, Samsung test left) |
| M10 | Settings, shell tile, find phone, media | ☐ |
| M10b | KDE Plasma support (Dolphin menu, tray widget) | ☐ |
| M11 | Hardening and release | ☐ |

## Log (newest first)
- 4 November 2026: v0.5.10. Mirroring survived YouTube: video apps take the phone's hardware encoder; now rebuilt on the same connection (smaller hardware, then software, full size every 30 s). Full 60 fps resolution and ~0.2 bit/pixel bitrate (up to 50 Mbit/s, encoder-capped, RateControl backs off). Wi-Fi lock (power save off), URGENT_DISPLAY encoder thread, Qualcomm low-latency vendor key. Desktop logs why a stream ended. Tested on the Redmi: 20 Shorts swipes without a drop, 720x1600 hardware at ~55-60 fps during video. Found phone-ping broken (GSConnect gone).
- 4 November 2026: v0.5.9 released (Phone Mirroring, Banking mode, Mirror tile, fixes). CI and Release green; APK, desktop bundle, deb/rpm/Arch on the release; apt/dnf/pacman repos and website updated.
- 4 November 2026: review + fixes before 0.5.9. Desktop announces `mirror.view` (it never advertised mirroring, so the phone's Mirror tile gave up); the phone sends heartbeats / `mirror.request` only to such desktops (older desktops ended the stream on a zero-length frame). Mirror tile asks "Start now" right away and keeps the permission 30 s for the computer; tested end to end on the Android 17 emulator (tile -> PC window -> stream -> close -> notification and service gone). Hint detection never wipes text in fields without selection support. Banking mode keeps saved helpers if switched on twice and matches accessibility entries in short or long form. Remote control row always visible (security). Resize grips measured in surface coordinates (no swinging). Pairing refused while the port isn't bound yet. Stop-then-start race fixed (`stopping`).
- 4 November 2026: M9 polish on the real Redmi. Banking mode confirmed with a real banking app (opens with it on, warns with it off); it also pauses Continuum's accessibility helpers because banking apps check enabled accessibility services. Mirror window: resize from edges/corners keeping the phone's shape, round red close, closes by itself when sharing ends after it was live (no frozen picture). Lag skip now needs a backlog that lasts 250 ms or 10+ frames: Wi-Fi bursts had made it drop ~half the frames (26 → ~50 fps in motion, 0 skipped). Phone reconnects after an app update even where MY_PACKAGE_REPLACED isn't delivered (MIUI): the notification listener and remote-control service restart the connection. Icons: macOS-style coloured tiles with Continuum's own solid SVG glyphs (desktop sidebar, Mirror/Ring), iOS-style coloured tiles in Android settings and the home screen (same colour per feature on both), Quick Settings tiles get their own bank / clipboard icons, new Phone Mirroring app icon.
- 4 November 2026: M9 phone mirroring + Banking mode (ADR-0017), not released yet. Phone: MediaProjection → hardware H.264 (low latency, exact screen shape, new SPS on rotation) on a data connection; permission kept 5 min; "Continuum remote control" accessibility service for taps, swipes, scroll, keys, typing (only while streaming). Desktop: `StartMirror`/`StopMirror` D-Bus (socket pair to the UI), borderless phone-shaped window with rounded corners and soft shadow, bar above the phone on hover (close, back, home, recents, screenshot), Mirror button + `mirror` app action. Banking mode: switch + Quick Settings tile turn developer options/USB/wireless debugging off and restore them (one-time adb grant of WRITE_SECURE_SETTINGS). Real Redmi Note 11 (MIUI): hardware encoder OMX.qcom.video.encoder.avc 864x1920 at 60 fps, ~1.5 Mbit/s for a still UI, desktop shows ~55-58 fps, 0 skipped. Phone Mirroring is its own app (`continuum-ui --mirror [ID]`, app id ...Continuum.Mirror, own .desktop + icon from scripts/render-mirror-icon.py) so it has its own dock icon. Fixed: blank dock icon after the bundle uninstall left a stale ~/.local icon cache. Speed: 128 KiB send buffer for video (no queued lag), adaptive bitrate (backs off when sending blocks >35% of a second), no B-frames, encoder buffer released before sending, desktop skips to the next key frame if decoding falls behind (asks the phone with `sync`), screen kept on (dimmed) while mirrored, typed passwords forgotten on stop. Emulator is encoder-bound (software c2.android.avc: ~10 fps at 862x1920, ~28 at 640); desktop showed every frame it got. Debug: CONTINUUM_MIRROR_DEBUG=1 (UI fps/Mbit/s), `setprop log.tag.Continuum DEBUG` (phone fps, send time), CONTINUUM_MIRROR_MAX. Live touch: mouse drags and touchpad scrolls are one continued stroke (down/move/up). Window is only the phone (no border or shadow); the bar shows only above the phone. Emulator (Android 17): live shade pull-down and back, consent, stream, reopen without asking, tap, scroll, back, typing incl. emoji/backspace, Banking mode on (adb dropped, mirroring kept going) and off. Packaging recommends the H.264 decoder (gstreamer1.0-libav / openh264 / gst-libav).
- 4 November 2026: v0.5.8. Updater opens Android's installer (ACTION_VIEW + FileProvider) from the visible card instead of a PackageInstaller session (MIUI blocked the background prompt; state stuck Installing). Tested 0.5.6-labelled signed build -> 0.5.7 on the emulator incl. Cancel then Install and Play Protect scan. Releases are no longer pre-releases; Continuum.apk + releases/latest link on the site.
- 3 November 2026: v0.5.6–0.5.7. Screenshot watcher reacts to any non-delete MediaStore change (MIUI doesn't flag inserts), detects by folder or Screenshot_ file name, oversized screenshots re-encoded as JPEG; daemon converts JPEG/WebP clipboard images to PNG (image crate, limits). Simulated Pixel, Xiaomi (JPEG), Samsung, vivo, OPPO and a 14 MB Ultra screenshot on the emulator: 6/6. Not yet confirmed on the owner's Redmi.
- 3 November 2026: v0.5.5. Text Size menu (desktop), dark-mode prompt cards (Android 16+ bright containers), installer drops old open-to-all firewall rules, website + README rewritten with demo-content screenshots (emulator only). Verified on this PC: one-line install, phone screenshot to PC clipboard (2 s), PC text to phone, 50 MB both ways with matching SHA-256.
- 3 November 2026: v0.5.4. System packages (deb, rpm, Arch) and signed apt/dnf/pacman repositories on GitHub Pages, published by the Release workflow; one-line installer (website/install.sh); continuumd-update.path restarts the service after updates. Tested install and update on Ubuntu 26.04, Fedora 45 and Arch VMs (GNOME 50/51). Extension declares GNOME 51. Android redesign (home, Settings, notification categories), security fixes (local-only external pairing links, overflow-safe sizes, escaped peer text, connection cap, caller-granted shares, GitHub-only updater, local-only firewall rule), one charging bolt.
- 3 November 2026: Phone efficiency. Adaptive keepalive (`presence` cap/message: desktop pings every 3 s while the phone is in use or a request/transfer runs, else every 30 s with a 75 s timeout; a desktop action at the slow pace probes with a 4 s deadline), phone pings only after 40 s of silence, reconnect backoff up to 10 min with the screen off (screen on retries at once), mDNS on every 4th failure or after a network change, throttled charging watts, app icons once per connection (`notif.icons`), identical reposts skipped, screenshot observer only while connected (inserts only on Android 11+), lazy clipboard parts, HEIC via a temp file, cached photo total and contact photos, update check sleeps until due. Emulator: 3 s pings with the screen on, 30 s with it off, frozen app dropped in under 9 s.
- 3 November 2026: v0.5.2. Raw fetch over data connections (cap fetch.raw, token, SHA-256), Wi-Fi/CPU locks during transfers, 1 MiB chunks, 4 MiB phone socket buffers, transfers button + quiet GNOME progress notifications, phone progress with time left, 49 original phone drawings + model table (hello `model`), autotext in reply boxes, arrow path bar, no duplicate downloads, busy toasts dismissed on completion. Fixed a KDE Connect/GSConnect notification echo loop (DESKTOP_MIRROR_APPS).
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
