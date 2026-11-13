# ADR-0103 — The amplifier display plots the recentred static curve on linear ±3 dBFS axes

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-12
- **Issue:** #60
- **Architecture:** [Amplifier display](../architecture/effect-displays.md#amplifier-display)

## Context

Issue #60's design includes a display of the amplifier's static transfer function. Chris Paro
asked for it on linear axes, first at ±12 dBFS, then narrowed to ±3 dBFS so the span out to
0 dBFS stays legible, with thin red lines at full scale like the dynamics display (ADR-0101).
Bias shifts the curve: `f(drive·x + bias)` at `x = 0` is `f(bias)`, a steady offset that the DC
filter removes from the output.

## Decision

- `AmplifierDisplay` plots `audio_effects::amplifier_curve(...).output(x)`, which is
  `makeup·(f(drive·x + bias) − f(bias))`. Subtracting `f(bias)` recentres the curve where the DC
  filter leaves silence, so the curve always passes through the origin, and bias shows as
  asymmetry rather than an offset. `amplifier_curve` uses `Shape::apply`, the function
  processing uses.
- Both axes are linear in sample value from −10^(3/20) to +10^(3/20). Grey lines run through 0,
  and red 1-pixel lines mark ±1 on both axes.
- Labels are shown only at ±1 (`0dB`) and at the edges (`+3dB`).
- It reuses the equaliser display's gutters, label size, curve width and the `FULL_SCALE` colour.

## Alternatives considered

- **Plotting without recentring.** Accurate for the shaper alone, but it shows an offset the
  output never has, and a nonzero bias would move the whole curve off the origin.
- **dB axes, as the dynamics display has.** A waveshaper's curve is a function of the signed
  sample, and its asymmetry, the point of bias, is lost on a magnitude scale.
- **±12 dBFS axes.** Most of the plot would be beyond full scale, squeezing the region the shapes
  act in.
- **A label at every grid line.** Linear dB labels fall at irregular positions; two marks per
  side say what the axes need.

## Consequences

- The recentred curve is the steady state of a DC-free output. It does not show the DC filter's
  low-frequency response or the transient while the filter settles after a bias change.
- Oversampling does not appear; it changes aliasing, not the static curve.
