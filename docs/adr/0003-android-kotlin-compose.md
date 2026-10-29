# ADR-0003: Android in Kotlin + Jetpack Compose, minSdk 26

- **Status:** Accepted
- **Date:** 29 October 2026

## Context
The phone app needs deep system access (share sheet, notifications, telephony, screen capture,
foreground services), a modern UI, and should run on most phones still in use.

## Decision
Native **Kotlin** with **Jetpack Compose** and Material 3. Minimum Android 8.0 (API 26), target
the latest SDK. Coroutines for concurrency.

## Alternatives considered
- **Flutter / React Native**: cross-platform UI, but every system feature we need would go
  through platform channels written in Kotlin anyway. Adds a layer with no benefit.
- **Shared Rust core via JNI**: one protocol implementation for both sides is attractive, but it
  complicates the build (NDK, cross-compilation, JNI glue) for a small team. We may revisit this
  if the two protocol implementations drift (would be a new ADR).
- **minSdk 29+**: simpler code, but drops many phones still in daily use.

## Consequences
- Good: full access to Android APIs. Compose is Google's recommended UI toolkit.
- Cost: the protocol is implemented twice (Rust and Kotlin). Interop tests against the real
  daemon (see SRS §5) keep them in sync.
- API 26–28 lack some features. Those features check the version and degrade gracefully.
