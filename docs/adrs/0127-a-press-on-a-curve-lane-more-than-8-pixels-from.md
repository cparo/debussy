# ADR-0127 — A press on a curve lane more than 8 pixels from the curve seeks rather than adding a point

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-14
- **Issue:** #63
- **Architecture:** [Layout](../architecture/user-interface.md#layout)

## Context

ADR-0126 made every press on empty curve lane add a point. Audio lanes seek on a press
(ADR-0076), so with curve lanes filling more of the pane there was no way to seek from them, and
a press meant to seek left a stray point.

## Decision

A press on a curve lane that catches no point or handle adds a point only within 8 logical
pixels above or below the curve at that frame. Further away it seeks there, as a press on an
audio lane does, also while playing.

## Alternatives considered

- **Always adding** (ADR-0126). Rejected by the developer: no seeking from curve lanes, and stray
  points.
- **Adding only with a modifier held.** Rejected: adding points is the lane's main gesture, and
  the line itself is a clear target.

## Consequences

- To add a point far from the line, the user presses near the line and drags it there.
- The 8-pixel reach is vertical: on a steep segment, a press beside the line but far below it
  seeks.
