# ADR-0076 — Playback stops by itself at the project's end

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-09
- **Issue:** —
- **Architecture:** [Transport and seeking](../architecture/user-interface.md#transport-and-seeking)


## Context

Playback never stopped on its own. Started near the end of a project, it ran on into silence, and
because the timeline and scroll bound include the playhead
([ADR-0069](0069-panning-stops-at-the-later-of-the-project-s-end.md)) and the view pages along
with it ([ADR-0063](0063-while-playing-the-view-pages-along-with-a-visible.md)), the position
bar's timeline kept growing and the view kept scrolling into empty time. The playhead time's
format ([ADR-0075](0075-the-playhead-time-reads-hh-mm-ss-000-dropping.md)) could even gain an
hours field on a 45-minute project. The developer chose among the options below.

## Decision

- Playback stops by itself once the playhead reaches the project's end (the end of the last
  clip), leaving the playhead at the end. The return point is kept, so Stop still returns there.
- Play with the playhead at or past the end starts from the return point if it is before the
  end, otherwise from frame 0.
- A seek past the end is kept: importing into a track and pasting both work from the playhead,
  and need to reach empty time after the project.
- ADR-0069's bound is unchanged. The playhead can still be past the end after such a seek or an
  edit that shortens the project, and the bound must still reach it there.

## Alternatives considered

- **Stop and return to the return point** — loses the place playback reached, which is where the
  user listened to last.
- **Run on, clamping only the display** — the playhead would leave the view and the position bar
  while still playing silence.
- **Clamping seeks to the end as well** — first chosen, then dropped while building: it stopped
  import and paste from placing audio after the project's end.

## Consequences

- The real-time side plays up to one display frame of silence past the end before the next tick
  stops it; the playhead is then put back at the end.
- Recording past the end, when it exists, will need playback to run on while recording.
