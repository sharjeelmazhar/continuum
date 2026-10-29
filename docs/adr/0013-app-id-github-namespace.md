# ADR-0013: App ID under the GitHub namespace

- **Status:** Accepted
- **Date:** 29 October 2026

## Context
Linux apps (Flathub, D-Bus, desktop files) and Android apps need a unique reverse-domain ID.
Flathub only accepts IDs under a domain the publisher controls. The project has no website,
but it's hosted on GitHub, and GitHub accounts count as controlled domains (`io.github.<user>`).
Changing the ID after release breaks upgrades and existing pairings, so it must be fixed early.

## Decision
- Desktop app ID: `io.github.sharjeelmazhar.Continuum`
- D-Bus: `io.github.sharjeelmazhar.Continuum.Daemon` (interfaces end in a version, e.g. `Daemon1`)
- Android package / application ID: `io.github.sharjeelmazhar.continuum` (lower case by Android convention)

## Alternatives considered
- **`org.continuum.*`**: we don't own `continuum.org`. Flathub would reject it.
- **Buying a domain**: costs money and must be renewed forever. Not needed.

## Consequences
- Good: accepted by Flathub and F-Droid without a website. Stable from now on.
- Cost: if the repo ever moves to a GitHub organization, the ID stays as is (moving it would
  break upgrades). That's fine: the ID is invisible to users.
