# Design documents

One document per module, plus one for cross-cutting security. Each says **how** a module meets
the requirements in [`../SRS.md`](../SRS.md), lists the requirement IDs it covers, and links to
the ADRs behind its choices. Diagrams are Mermaid (GitHub renders them).

| Doc | Module | Code |
|---|---|---|
| [00-system-overview](00-system-overview.md) | Whole system: components, connection lifecycle, threading | — |
| [01-continuum-proto](01-continuum-proto.md) | Wire protocol library | `desktop/continuum-proto/` |
| [02-continuumd](02-continuumd.md) | Desktop daemon | `desktop/continuumd/` |
| [03-continuum-ui](03-continuum-ui.md) | Desktop GTK app | `desktop/continuum-ui/` |
| [04-desktop-integrations](04-desktop-integrations.md) | Nautilus extension, GNOME Shell extension | `desktop/nautilus/`, `desktop/shell-extension/` |
| [05-android-app](05-android-app.md) | Android app and service | `android/` |
| [06-security](06-security.md) | Threat model, pairing, key storage | all |
| [07-build-test-ci](07-build-test-ci.md) | Tooling, test strategy, emulator, CI | `scripts/`, `.github/` |

## How to change a design
1. Update the design doc (and SRS, if requirements change) in the same PR as the code.
2. If the change reverses or replaces an ADR, add a new ADR.
3. Keep each doc short: purpose, responsibilities, interfaces, diagrams, failure handling, tests.
