# ADR-0069 — Panning stops at the later of the project's end and the playhead

- **Status:** Accepted
- **Decided by:** agent, ratified by Chris Paro
- **Date:** 2026-11-08
- **Issue:** —
- **Architecture:** [Transport and seeking](../architecture/user-interface.md#transport-and-seeking)

## Context

A sideways two-finger scroll over a lane pans the view, and nothing stopped it at the end: the view
start only saturated at 0, so a scroll could carry the view indefinitely into empty timeline, with
no way back except scrolling as far again. Seeking and playback paging, though, legitimately put
the view past the last clip when the playhead is there.

## Decision

The scroll bound is the later of the project's end (the end of its last clip) and the playhead.
Panning forward stops with the bound at the lane's right edge (start 0 when it fits in the view),
and never moves a view that is already further on backwards. Zooming out on the grid, from a view
that ended within the bound, clamps the zoomed start the same way; zooming in cannot widen the
view and the fit starts at 0. Seek-recentring and playback paging are not bounded: they must show
the playhead wherever it is.

## Alternatives considered

- **Bound by the project's end alone** — a playhead past the last clip (a seek into silence, or
  playback running on) could be shown by a seek but not panned back to after panning away.
- **Clamp every view change, seeks and paging included** — the playhead could then be out of
  reach of the view, defeating seek-into-view and playback paging.
- **Pull a view that is past the bound back on any forward pan** — a small scroll would make the
  view jump backwards, which reads as a glitch; leaving it in place and letting a backward pan
  return is calmer.
- **Allow some slack past the end** — no use for it yet; empty timeline past the end shows
  nothing to edit.

## Consequences

The project's end sits at the lane's right edge when scrolled fully right, so a view cannot be
panned to put the end mid-lane; seek there if that is needed. The bound moves with the playhead,
so during playback past the last clip the pannable range grows as it plays.
