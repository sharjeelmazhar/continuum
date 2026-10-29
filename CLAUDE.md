# Continuum — instructions for every Claude session

Continuum is an open-source, Apple-Continuity-style ecosystem between **Android phones** and
**Linux desktops** (GNOME first). It's meant to replace KDE Connect / GSConnect with a modern,
fast, secure and polished experience.

## Start of every session (keep it cheap)
1. Read `PROGRESS.md`. It has the current milestone, what's done, and the **next task**.
2. Open only the docs that task needs: `docs/ROADMAP.md` (milestone scope),
   `docs/ARCHITECTURE.md` (layout and stack), `docs/PROTOCOL.md` (wire format and security).
3. If `CLAUDE.local.md` exists, read it (the owner's private machine notes, not tracked by git).
4. Do one milestone task per chat. Before stopping, update `PROGRESS.md` (status, log line,
   next task) and commit.

## Hard rules
- **The repo is public. Never commit private info**: names, emails, locations, personal
  circumstances, phone numbers, IPs, device serials, keys or certificates. Personal notes go in
  `CLAUDE.local.md` (gitignored). Use generic examples ("Pixel 8", "my-desktop") in docs and tests.
- **Side project on a limited usage budget** (the owner needs usage for their job). Keep each session small and efficient. Don't re-read whole files you don't need. No speculative
  refactors. Keep replies short.
- **Security first.** Every connection is TLS 1.3 with a pinned peer certificate after pairing.
  Nothing plaintext on the LAN. Treat every incoming frame as hostile: size limits, strict
  parsing, no path traversal in file names.
- **Tests come with features.** Rust: `cargo test` plus clippy (`-D warnings`). Android:
  JUnit plus a build. A task is done only when its tests pass.
- **Long jobs must be resumable**: atomic writes (temp file, then rename), and file transfers
  resume from verified chunks.
- Conventional commits (`feat(desktop): …`, `fix(android): …`, `docs: …`). Commit after each
  finished task.

## Stack (decided, don't revisit without the owner)
- Desktop: Rust (tokio, rustls, mdns-sd), with GTK4 + libadwaita UI (gtk4-rs). Cargo workspace in `desktop/`.
- Android: Kotlin + Jetpack Compose, minSdk 26 (Android 8.0), target the latest SDK. Gradle project in `android/`.
- Phone clipboard auto-sync: Shizuku / one-time ADB grant (Android 10+ blocks background clipboard reads).
- License: GPL-3.0-or-later.
