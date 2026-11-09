# ADR-0085 — The bars and beats grid is a sharp overlay thinned by zoom

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-09
- **Issue:** #57
- **Architecture:** [Layout](../architecture/user-interface.md#layout)

## Context

#57 includes a bars/beats grid in the lanes, so takes can be checked against the click and, in a
follow-up, clips snapped to it. Lanes are rasterised and blurred (ADR-0068); lines drawn into
that raster would blur and would force a re-raster on every tempo change. Zoomed out, a line per
beat would merge into a solid tint.

## Decision

- The grid is a view toggle in the top bar beside waveform/spectrogram: session state, off at
  launch, like the display choice.
- Its lines are drawn in the canvas stacked over each lane with the playhead cursor, not in the
  raster: one physical pixel wide, snapped to the pixel grid, in the text colour, bars at 40 %
  and beats at 16 % opacity.
- Line positions come from `project_model::tempo`, so they fall on the frames the click plays.
- Beat lines are hidden when they would be closer than 6 logical pixels. Bar lines are then drawn
  on every 2^k bars, k the least that keeps them at least 6 pixels apart, aligned to multiples of
  2^k so they do not shift while panning.
- The playhead's time readout stays `HH:MM:SS.000`.

## Alternatives considered

- **Lines in the lane raster** — blurred with the waveform, and every tempo change or scroll
  would re-raster every lane.
- **A ruler of bar numbers above the lanes** — useful, but a layout change of its own; the
  overlay answers #57's need.
- **Thinning to every 4 or 5 bars** — powers of two keep each thinner level a subset of the
  last, so lines do not jump while zooming.
- **A bars/beats time readout** — would hide the frame-exact time the rest of the editor uses.

## Consequences

- With the grid on, each lane's overlay redraws as the view moves, as the cursor already does.
- Snapping to the grid is left to a follow-up issue.
