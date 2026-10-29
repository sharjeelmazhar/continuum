# ADR-0012: GPL-3.0-or-later

- **Status:** Accepted
- **Date:** 29 October 2026

## Context
Continuum is open source. We want improvements to stay open, and we may reuse ideas or code from
projects in the same space: KDE Connect and GSConnect (GPL), scrcpy (Apache-2.0).

## Decision
License all code under **GPL-3.0-or-later**.

## Alternatives considered
- **MIT / Apache-2.0**: allows closed-source forks, which we don't want here.
- **AGPL**: aimed at network services. Not relevant.

## Consequences
- Good: compatible with GPL and Apache-2.0 dependencies, and forks stay open.
- Cost: Google Play allows GPL apps, but every dependency must have a GPL-compatible license.
  `cargo-deny` checks this in CI (M11).
