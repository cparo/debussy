# ADR-0101 — The dynamics display plots the steady-state transfer curve on fixed dBFS axes with red full-scale lines

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-12
- **Issue:** #59
- **Architecture:** [Dynamics display](../architecture/effect-displays.md#dynamics-display)

## Context

The `dynamics` dialog had no display (ADR-0091 leaves such a kind blank). Chris Paro asked for one
showing the transfer function, output over the detector's input level, including every curve,
ceiling and output parameter, on a −96 to +12 dBFS scale, with thin red lines at 0 dBFS on both
axes. Issue #134 asked for this curve together with a live gain-reduction meter.

## Decision

- `TransferDisplay` plots `audio_effects::dynamics_curve(...).output_db(x)` across the input
  axis. `dynamics_curve` is the mapping processing uses (the infinite ratio at the top value, the
  ceiling only when enabled), and `output_db` is `dsp::transfer_db`, so the picture is the level
  processing settles at.
- Both axes run from −96 to +12 dBFS, the same range as the equaliser's gain axis. The display
  reuses that display's gutters, grid, labels and curve style through shared constants and
  functions in `response_display`.
- The 0 dBFS lines are 1 logical pixel wide and red (`theme::FULL_SCALE`) in both tones.
- `audio_effects` exposes the curve as `TransferCurve`, so the UI crate needs no new dependency on
  `dsp`.

## Alternatives considered

- **Re-deriving the curve in the UI from the parameter values.** It could drift from processing,
  for example in how ∞ or the ceiling toggle is read.
- **A square plot.** It keeps 45° meaning 1:1, but it would leave most of a wide dialog empty.
  The axes are labelled, and the unity line is easy to read off the grid.
- **Adding `dsp` to the UI's dependencies.** It changes `Cargo.lock`, and the UI should reach the
  audio core only through `audio_effects`.

## Consequences

- The display shows the static curve only: attack, release, hold and lookahead do not appear. The
  live gain-reduction meter remains in #134 and needs a readout path from node to UI.
- A later change to the equaliser display's grid or labels changes this display too, through the
  shared items.
