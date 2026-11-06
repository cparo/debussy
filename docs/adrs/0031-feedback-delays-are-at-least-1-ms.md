# ADR-0031 — Feedback delays are at least 1 ms

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-06
- **Issue:** #16
- **Architecture:** [Feedback loops](../architecture/signal-graph.md#feedback-loops)

## Context

Cycles in the processing graph must pass through a feedback-delay node, and the engine processes
a cycle in sub-blocks no longer than its shortest delay
([ADR-0016](0016-feedback-loops-must-pass-through-a-fixed-delay.md)). Very short delays force very
small sub-blocks, and per-sub-block overhead then dominates CPU cost. No minimum was set.

## Decision

- A feedback delay is at least 1 ms. Its length is stored as a time and converted to samples at
  the project rate, rounded up: 45 samples at 44.1 kHz, 48 at 48 kHz, 89 at 88.2 kHz, 96 at
  96 kHz.
- The UI's delay control stops at 1 ms. A shorter value from any other source (a saved project,
  automation) is raised to 1 ms.

## Alternatives considered

- **32 samples** — matches the smallest block size
  ([ADR-0007](0007-the-engine-targets-a-128-frame-default-buffer.md)), but is a different time at
  each project rate, so the same patch would sound different at 44.1 and 96 kHz.
- **One block** — needs no sub-blocks, but the minimum would change with the user's buffer size,
  the dependence ADR-0016 exists to prevent.

## Consequences

- At every supported rate the minimum is at least 45 samples, more than the 32-frame smallest
  block, so a cycle's sub-blocks are never smaller than blocks the engine must already process in
  time.
- Comb-filter and flanging effects that need delays under 1 ms are not possible through feedback
  routing; a dedicated effect can still provide them internally.
