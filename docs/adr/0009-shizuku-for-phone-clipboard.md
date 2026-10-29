# ADR-0009: Shizuku / ADB grant for background clipboard on Android

- **Status:** Accepted
- **Date:** 29 October 2026

## Context
Since Android 10, only the keyboard app or the app in the foreground may read the clipboard.
A background service can't notice that the user copied something, so automatic phone → PC
clipboard sync is impossible for a normal app.

## Decision
- With **Shizuku** (or a one-time `adb shell` permission grant), the app reads the clipboard in the
  background, so the sync is automatic. The app has a guided, plain-language setup for this.
- Without it, the app offers manual sending: a Quick Settings tile, a share-sheet action, and a
  notification button. PC → phone always works (writing the clipboard is allowed).

## Alternatives considered
- **Accessibility service tricks**: fragile, Google Play rejects them, and they feel invasive.
- **Custom keyboard**: users don't want to change keyboards for this.
- **Root**: most users don't have it.

## Consequences
- Good: automatic sync is possible without root. Everyone else still has a one-tap fallback.
- Cost: Shizuku must be restarted after each reboot on non-rooted phones (wireless debugging
  helps on Android 11+). The guide must explain this simply.
