# ADR-0101 — The dynamics display shows the envelope window beside the transfer curve

- **Status:** Accepted
- **Decided by:** agent, ratified by Chris Paro
- **Date:** 2026-11-12
- **Issue:** —
- **Architecture:** [Dynamics display](../architecture/effect-displays.md#dynamics-display)

## Context

Chris Paro's spec asked for two side-by-side visualisations: the envelope's shape, with the
current sample marked, the lookahead to the right and the oldest sample's age to the left; and the
scaling curve on −96 to +12 dB, with an inverted curve below a centre line when asymmetric
shaping is on. The review found two gaps. The x-axis of the curve plot was unspecified. And with a
5 s release beside a 1 ms attack, a single linear time axis flattens the attack side to nothing.

## Decision

- **Scales.** The envelope plot gives the release side and the attack side half its width each,
  each scaled to its own length. A side of zero length gives the other the whole width.
- **Curve plot.** The curve plot's x-axis is input level in dB and its y-axis is output level in
  dB, both −96 to +12. The curve includes the preamp, makeup and ceiling, so it shows the static
  input-to-output mapping.
- **Shared code.** The plots reuse the equaliser display's gutters, label face, colours and grid
  style, and are drawn from `audio_effects::window_weight` and `audio_effects::static_output`, so
  the picture and the processing share their formulas.

The plan proposed these defaults and Chris Paro approved it. The gutters, grid spacing and the
gap between the plots were chosen when the display was built.

## Alternatives considered

- **One linear time axis for the envelope.** Rejected because a short attack disappears next to a
  long release.
- **A logarithmic time axis.** Rejected because it cannot show zero, which is the current sample.
- **Plotting the curve's gain instead of its output level.** Rejected because input-to-output
  level is the usual dynamics graph and shows the ceiling directly.

## Consequences

- **Distorted time.** The envelope's time axis is piecewise, so distances on the two sides are not
  comparable. The labels give only the ends and the current sample.
- **Steady state only.** The curve plot shows steady-state levels. It does not show how the RMS
  window responds to transients.
- **Not verified by eye yet.** The display has not been checked on screen yet; it needs a visual
  check in the app.
