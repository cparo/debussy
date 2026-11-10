# ADR-0094 — The parametric EQ has four bands framed by optional high- and low-pass stages

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-10
- **Issue:** —
- **Architecture:** [Parametric equaliser](../architecture/effects.md#parametric-equaliser)

## Context

ADR-0088 gave `parametric-eq` eight bands. That is more than the people Debussy is for need
(`docs/product/`), and a wall of bands invites fixing problems in the mix that are better fixed at
the source: a better take, a better microphone position. The cuts people most often reach for at
either end of the spectrum, removing rumble and hiss, are what `high-pass` and `low-pass` already do,
but using them means adding two more nodes beside the equaliser.

## Decision

- `parametric-eq` has four fully parametric bands, starting at 100, 500, 2 000 and 8 000 Hz.
- An optional high-pass stage runs before the bands and an optional low-pass stage after them.
  Each has `enabled` (default off), `cutoff` (defaults 80 Hz and 12 kHz) and `order` (1 to 8,
  default 2, stopped-only), with the same Butterworth design, ranges and defaults as the standalone
  `high-pass` and `low-pass` (ADR-0088), shared in code.
- Turning a stage on or off crossfades between its input and its filtered output over the
  parameter ramp time. A stage that is off, once faded, is skipped and its state cleared.
- The equaliser's frequency response includes each enabled stage.
- Saved values for bands 5 to 8 are not migrated (decided by Chris Paro in answer to the `product`
  flag): they stay in the file, match no parameter, and do nothing. Nothing has been released, so
  no user's project depends on them.

## Alternatives considered

- **Keep eight bands** — rejected for the audience reasons above.
- **Three bands, or fixed-type bands** — four fully parametric bands still cover a low shelf, a
  high shelf and two bells, or a notch for a hum, without forcing a choice.
- **High-pass and low-pass as band types** — a band is one two-pole section, so it could not offer
  the steeper orders, and a band spent on a cut is a band lost for shaping.
- **Morph the cascade's coefficients on enable, as bands do** — a cascade has no single identity
  mix to morph from, and the order can change the number of sections. Because each section's
  output is linear in its mix coefficients, a band's morph to the identity is itself a crossfade
  with the input, so a crossfade gives the stage the same behaviour.
- **Migrate band 5–8 values into bands 1–4** — there is no faithful mapping from eight bands to
  four, and no released project to protect.

## Consequences

- A project saved with bands 5 to 8 in use sounds different when reopened; those values sit unused
  in the file until it is saved again.
- An equaliser cut that needs more than four bands takes a second `parametric-eq` in series.
- The standalone `high-pass` and `low-pass` effects remain, for use without an equaliser.
