# ADR-0017: Phone mirroring without ADB (MediaProjection + Accessibility)

- **Status:** Accepted
- **Date:** 4 November 2026

## Context
The roadmap (M9) allowed a scrcpy-style capture through Shizuku/ADB. That needs USB or wireless
debugging, and many banking and wallet apps refuse to open while developer options or USB
debugging are on. Users would have to switch them on for mirroring and off for banking.

## Decision
- Video: Android's MediaProjection (the user taps "Share screen" once; the permission is kept
  for 5 minutes after the window closes), hardware H.264 at low latency, streamed on a normal
  pinned-TLS data connection. The desktop decodes with GStreamer into a borderless, rounded,
  phone-shaped window.
- Input: a separate, opt-in accessibility service ("Continuum remote control") performs
  gestures, global actions and text edits. It acts only while a stream is live. The clipboard
  watcher (ADR-0016) stays content-free.
- Banking mode: one switch (app, Quick Settings tile) that turns developer options, USB and
  wireless debugging off and restores them. It needs WRITE_SECURE_SETTINGS, granted once by ADB.

## Consequences
- Works with debugging off. Android shows its screen-sharing indicator while mirroring.
- Screens marked secure (banking apps, DRM video) show black, as Android intends.
- Drags are replayed on release, not live; live dragging needs ADB-level injection (later).
