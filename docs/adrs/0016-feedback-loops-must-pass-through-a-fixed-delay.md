# ADR-0016 — Feedback loops must pass through a fixed-delay node

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-06
- **Issue:** —
- **Architecture:** [Feedback loops](../architecture/signal-graph.md#feedback-loops)

## Context

Feedback routings (an output fed back to an earlier input) enable creative effects. In a
block-based engine, a naive cycle gets an implicit delay of one buffer, so the sound would change
with the buffer size ([ADR-0007](0007-the-engine-targets-a-128-frame-default-buffer.md)).

## Decision

Cycles are allowed only through an explicit feedback-delay node with a user-set length. The engine
processes a cycle's nodes in sub-blocks no longer than its shortest feedback delay, so the output
does not depend on the buffer size.

## Alternatives considered

- **Acyclic only** — the simplest scheduler, but it rules out feedback routings between effects.
- **Allow cycles with a one-block delay** — easy, but the delay, and with it the sound, changes when
  the user changes the buffer size, and exports would not match playback.

## Consequences

- The scheduler must find cycles (strongly connected components), check that each has a delay node,
  and process them in sub-blocks.
- Very short feedback delays force small sub-blocks and raise CPU cost. A minimum delay length is
  still to be set.
- Feedback-delay length is a node parameter, so it is saved with the project and can be automated
  like any other.
