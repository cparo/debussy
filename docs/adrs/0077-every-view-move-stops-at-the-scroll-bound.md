# ADR-0077 — Every view move stops at the scroll bound

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-09
- **Issue:** —
- **Architecture:** [Transport and seeking](../architecture/user-interface.md#transport-and-seeking)


## Context

[ADR-0069](0069-panning-stops-at-the-later-of-the-project-s-end.md) bounded panning and zooming
out by the later of the project's end and the playhead, but left seek-recentring and playback
paging unbounded so they could always show the playhead. A seek near the end recentred the view
half a window past the project, and paging put the playhead at 20 % of the lane even at the end.
Both added empty time to the timeline that the position bar then took in. Since the bound
already takes in the playhead, bounding these moves still keeps it in view.

## Decision

- The scroll bound is the later of the project's end and the playhead, as before. It is the
  furthest frame any view move shows, and it is shown as the lane's last frame, so a playhead
  stopped at the end ([ADR-0076](0076-playback-stops-by-itself-at-the-project-s-end.md)) is
  drawn.
- Panning forward, zooming out, playback paging and a seek's recentring all stop there.
- None of them moves a view that is already further on backwards; paging that would go
  backwards does nothing.
- This supersedes ADR-0069.

## Alternatives considered

- **Keep paging and seeking unbounded** — the reported bug: the view scrolls into empty time
  near the end.
- **The bound exactly at the lane's right edge, as before** — the playhead at the end, where
  playback now stops, would fall just outside the view and not be drawn.
- **Pull a view back to the bound when paging** — makes the view jump backwards during playback.

## Consequences

- Near the end, paging no longer puts the playhead at 20 %; it pages less, then not at all.
- Views end one frame past the project's end rather than at it.
