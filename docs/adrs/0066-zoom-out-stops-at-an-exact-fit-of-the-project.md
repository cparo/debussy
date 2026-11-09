# ADR-0066 — Zoom out stops at an exact fit of the project

- **Status:** Accepted
- **Decided by:** agent, ratified by Chris Paro
- **Date:** 2026-11-08
- **Issue:** —
- **Architecture:** [Transport and seeking](../architecture/user-interface.md#transport-and-seeking)

## Context

Zoom out went to 2^24 frames per column, a window of about 17 billion frames: hours past any
project, so the last dozen or more steps showed the clips shrinking to a sliver of empty timeline.
The farthest useful zoom is the one that shows the whole project.

## Decision

Zoom out stops once the view covers the project, frame 0 to the end of its last clip. The step
that would reach or pass that end lands instead on a *fit* level that shows exactly that stretch
(the end divided over the lane's columns, rounded up) starting at frame 0. The fit follows the
project as clips change. Zooming in from the fit returns to the coarsest √2 grid level finer than
it. While the view already covers the project, zoom out does nothing and its button is disabled.

## Alternatives considered

- **Stop at the first grid step that covers the project** — keeps every zoom on the √2 grid, but
  leaves up to √2 of the window empty past the end and keeps the old start, so the project's
  beginning may be off screen.
- **Keep the fixed 2^24 ceiling** — needs no project knowledge, but most of its range shows
  nothing.

## Consequences

The fit is the one zoom off the grid, so two zoom steps around it do not halve or double the time
shown. A project shorter than the default window opens with zoom out disabled. An empty project
fits at the default zoom. The 2^24 ceiling remains as a bound on the fit for very long projects.
