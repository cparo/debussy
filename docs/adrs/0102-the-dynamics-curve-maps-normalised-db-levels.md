# ADR-0102 — The dynamics curve maps normalised dB levels through tanh

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-12
- **Issue:** —
- **Architecture:** [Dynamics](../architecture/effects.md#dynamics)

## Context

ADR-0099 applied the curve to linear level: the output was `tanh((s·x)^c)`, with `c` at least 1.
On the log-log axes of a dynamics graph, that curve is a straight unity line with a slight
shoulder near the top. It cannot shape the range between the noise floor and full scale, which is
what makes a curve useful for compression, expansion and character. Chris Paro asked for `tanh`
to be applied to the levels in dB instead, normalised so that −96 dBFS is 0 and 0 dBFS is 1, with
exponents allowed down to 0, so that convex or concave curves can bridge the floor and the
reference ceiling smoothly.

## Decision

- **Normalisation.** With `u = (level dB + 96) / 96`, the curve is
  `v = tanh((s·u)^c) / tanh(s^c)` and the output is `−96·(1 − v)` dB.
- **Fixed points.** Dividing by `tanh(s^c)` keeps 0 dBFS at 0 dBFS, so −96 dB and 0 dB always map
  to themselves. The makeup gain and ceiling then move the curve, independently of its shape.
- **Curve exponent** runs from 0 to 16, on a linear scale. Below 1 the curve bends up (quiet
  levels are raised); above 1 it bends down (quiet levels are lowered, towards a gate). At 0,
  every level above the floor goes to full scale.
- **Steepness** is a plain multiplier from 0.01 to 10, on a log scale, with a default of 1. It sets
  how sharp the shoulder is. As it approaches 0 the curve tends to the pure power curve `u^c`, so
  with `c` = 1 it tends to unity.
- **Below the floor.** At or below −96 dB, and at silence, the gain is unity.
- **Both modes.** The waveshaper applies the same curve to each sample's magnitude and keeps its
  sign.

The normalisation by `tanh(s^c)` and the units and ranges of steepness were chosen when this was
built. Chris Paro asked for the dB domain and the floor and full-scale anchors.

## Alternatives considered

- **Keep the linear-domain curve of ADR-0099.** Rejected because its shape on dB axes is nearly
  fixed.
- **No normalisation (`v = tanh((s·u)^c)`).** Rejected because 0 dBFS would then come out at
  `−96·(1 − tanh(s^c))` dB, about −23 dB at the defaults. That would tie the curve's shape to an
  output-level loss that the makeup gain would have to undo.
- **Keep steepness in dB.** Rejected because steepness no longer scales a level. It is a
  dimensionless multiplier on the normalised level.

## Consequences

- The default curve (`s` = 1, `c` = 1) gently raises levels between the floor and full scale
  (−6 dB in comes out at about −4.6 dB). With a small steepness and `c` = 1 the curve is close to
  unity.
- Each gain now costs a `log10`, a `powf`, a `tanh` and an exponential. In the waveshaper that is
  per oversampled sample, up to about 770 000 per second for a stereo signal at 48 kHz with 8×
  oversampling.
- The patent research (ADR-0100) found no candidate whose verdict turned on the curve's form. The
  one *Unclear* verdict depended on peak detection, which stays out.
- This changes the ranges and units of `steepness`, `negative steepness`, `curve exponent` and
  `negative curve exponent` before the kind has been merged, so no saved project uses the old
  ones.
