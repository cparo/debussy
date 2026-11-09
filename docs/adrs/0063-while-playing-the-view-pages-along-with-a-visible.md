# ADR-0063 — While playing, the view pages along with a visible playhead, from 80 % of the lane's width to 20 %

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-08
- **Issue:** —
- **Architecture:** [Transport and seeking](../architecture/user-interface.md#transport-and-seeking)

## Context

With zooms down to 2 frames per column ([ADR-0062](0062-zooms-finer-than-the-waveform-summaries-are-drawn.md)),
a lane can show as little as 43 ms, and during playback the playhead leaves the view almost at
once. The view stayed where it was, so watching the audio play at any useful zoom meant panning
by hand.

## Decision

While playing, once the playhead has been seen in view, the view follows it: when the playhead
reaches 80 % of the lane's width, the view moves to put it at 20 %. A playhead that is out of view
and was not followed there is left alone, so panning or zooming away during playback is not
undone; following resumes once the playhead is in view again. A playhead that passes the right
edge between two display frames, as it can at the finest zooms, is still paged to 20 %.

## Alternatives considered

- **Continuous scrolling with the playhead fixed on screen** — redraws every lane every display
  frame, and the moving waveform is hard to read; paging keeps the view still most of the time.
- **Paging at the right edge to the left edge** — the playhead disappears at the edge before the
  page turns and the audio just played vanishes; the 80 %/20 % points keep some context on both
  sides.
- **Always follow, recentring a playhead out of view** — fights the user when they pan away during
  playback to look at another part of the timeline.
- **A follow toggle in the top bar** — another control to manage; following only what is visible
  gives the same control through panning.

## Consequences

Each page refreshes the lanes, which at the finest zooms reads the audio from disk
([ADR-0062](0062-zooms-finer-than-the-waveform-summaries-are-drawn.md)); at 2 frames per column a
page turns about every 35 ms. There is no setting to turn following off while the playhead is in
view.
