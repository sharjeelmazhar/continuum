# ADR-0008: One plugin per feature, negotiated by capabilities

- **Status:** Accepted
- **Date:** 29 October 2026

## Context
Features will keep being added for years (see the open-ended ROADMAP). The two sides will often
run different versions. Adding a feature mustn't mean touching the connection code.

## Decision
Each feature is a **plugin** on both sides (`share`, `clip`, `notif`, `call`, `mirror`, `battery`,
`find`, `media`). A plugin owns its message types (`<plugin>.<action>`), its settings and its
tests. When connecting, each side sends the list of plugins it has (`hello.caps`). Only plugins
present on both sides are switched on. Unknown messages get an `error.unsupported` reply.

## Alternatives considered
- **One big message handler**: simple at first, becomes impossible to change safely.
- **Global protocol version only**: forces both devices to upgrade together.

## Consequences
- Good: features can be developed, disabled and tested independently. Old and new versions interoperate.
- Cost: a small plugin interface to design in M1 (desktop) and M2 (Android).
- A crash or error in one plugin is contained and must not drop the connection (NFR-REL-3).
