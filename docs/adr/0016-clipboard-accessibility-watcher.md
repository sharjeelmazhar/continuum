# ADR-0016: Automatic phone clipboard through an accessibility copy watcher

- **Status:** Accepted (replaces the Shizuku / one-time ADB plan for phone → computer clipboard)
- **Date:** 29 October 2026

## Context
Android 10+ lets only the focused app (or the default keyboard) read the clipboard, and clipboard
change listeners don't fire for background apps. The owner wants copies on the phone to reach the
computer with no tap, like copies on the computer already reach the phone. KDE Connect needs a
"Send clipboard" tap for this direction.

## Decision
An optional **AccessibilityService** (`clip/CopyWatcher.kt`), turned on once by the user, watches
only event types and their own labels (never window content): a tap on "Copy", a "Copied" toast,
the system text-selection toolbar (with "Copy") closing, and Android's clipboard pop-up. Then it
adds a 1×1 transparent, untouchable, focusable `TYPE_ACCESSIBILITY_OVERLAY` window for a moment.
While that window has focus the app may read the clipboard; it reads, removes the window
(at most 600 ms), and sends the content if it's new. The app explains this in plain language and
offers it as a card until it's on. "Send clipboard" (notification, tile, share) stays as fallback.

## Alternatives considered
- **Shizuku / ADB helper running as the shell user**: reads silently, but needs a computer or
  wireless debugging after every reboot. Too much setup for non-technical users.
- **READ_LOGS + floating activity (KDE Connect style)**: needs a one-time ADB grant and Android 13+
  asks for log access again and again.
- **Being the keyboard**: would replace the user's keyboard. No.

## Consequences
- Good: copy → computer clipboard in about a second, no buttons, works on stock and Xiaomi phones.
- Cost: one Accessibility switch (Android 13+ sideloaded apps first need "Allow restricted
  settings"). Android 12+ shows "Continuum pasted from your clipboard" after each copy. Copy
  detection is a heuristic: a copy that triggers none of the signals is missed (fallback: the
  "Send clipboard" button). Reading briefly moves focus away from the current app; the overlay
  sets FLAG_ALT_FOCUSABLE_IM so the keyboard isn't touched.
