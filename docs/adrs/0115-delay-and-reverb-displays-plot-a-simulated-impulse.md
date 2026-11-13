# ADR-0115 — Delay and reverb displays plot a simulated impulse response

- **Status:** Accepted
- **Decided by:** agent, ratified by Chris Paro
- **Date:** 2026-11-13
- **Issue:** —
- **Architecture:** [Impulse response display](../architecture/effect-displays.md#impulse-response-display)


## Context

`delay` and `reverb` had no `Curve`, so their dialogs showed a blank charcoal display. Neither has
a static curve like the equalisers' or `dynamics`': what they do lies in time. A synced delay's
time also depends on the project tempo, which `CurveFn` did not receive.

## Decision

Both kinds give `Curve::Impulse(ImpulseCurve)`: the node's real output after a unit impulse into
both channels, dry part included. It is computed by running the effect's own per-frame processing
offline (`Running::new` is shared by `prepare` and the simulation, starting at the values with no
ramps). It is plotted as the peak level per column in dB, in stacked left and right lanes, over a
span fitted to the values. `CurveFn` and `audio_effects::curve` take the whole-note length in
frames as a third argument, from `Tempo::frames_per_whole_note`, which is now the one home of
that formula. Building the curve only copies the values; the simulation runs in `render`, which
the display cache (ADR-0095) calls only when the curve, tone or size changes.

The worst case, a 20 s decay plate at 192 kHz (a 20 s span after clamping), takes about 150 ms on
the interface thread in a release build (room 105 ms; 39 ms and 30 ms at 48 kHz). That is
accepted as it is, with no mitigation.

## Alternatives considered

- **An analytic envelope** (echo heights from `feedback^k`, an exponential for the reverb) —
  cheap, but it would not show the low-pass, saturation, ping-pong routing, the plate's and
  room's actual density, or the dry spike, and could drift from what processing does.
- **A linear waveform** — echoes past the first few, and a reverb's tail, vanish into the axis;
  a dB scale shows the whole decay.
- **A fixed time span** — short delays and long reverbs would not both read; the span fitted to
  96 dB of decay keeps each response filling the plot.
- **A mono plot** — hides ping-pong and the right channel's offset, which are what the stereo
  controls change.
- **Mitigating the worst case** by simulating above 48 kHz at a lower rate for display, or in a
  background task — rejected for now by the developer: it is slow only at high rates with long
  decays, and the cache already skips redraws when nothing changed. Either remains open if
  dragging proves sluggish.

## Consequences

- The display always matches what is heard, at the cost of a simulation per redraw: about 2 ms
  per second of span at 48 kHz, rising with the rate.
- Every curve function takes the whole-note length, though only `delay` uses it.
- A change to the delay's or reverb's processing changes its display with no display code
  touched.
