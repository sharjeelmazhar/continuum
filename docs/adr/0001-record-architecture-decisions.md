# ADR-0001: Record decisions as ADRs

- **Status:** Accepted
- **Date:** 29 October 2026

## Context
Continuum is built in short sessions, often by different contributors (or AI sessions) with no
memory of earlier discussions. Without a written record, decisions get re-argued or silently
reversed.

## Decision
Every decision that is hard to reverse or affects more than one module gets an ADR in
`docs/adr/`. Requirements live in `docs/SRS.md`, and module designs in `docs/design/`.

## Alternatives considered
- **Only code comments**: comments explain *how*, not *why we didn't do X*.
- **Wiki / issues**: separate from the code, so they drift out of date and aren't versioned.

## Consequences
- Good: new contributors can learn the reasons quickly. Changes are deliberate.
- Cost: a few minutes per decision.
