# ADR-0088 — Equalisers are built on a trapezoidal state-variable filter, and a filter's order changes only while stopped

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-09
- **Issue:** #58
- **Architecture:** [Filters](../architecture/effects.md#filters)

## Context

Issue #58 asks for the first real effects: high-pass and low-pass filters and a parametric
equaliser, whose controls move while audio plays without zipper noise or clicks. The audio core is
built in-house by default (CLAUDE.md "Dependency Policy"), so the filter topology, its designers and
its smoothing are ours to choose, and each needs patent evidence before it is built
(`docs/architecture/patents.md` "Patent policy").

The patent evidence is in `docs/research/eq-filter-patents.md`. Eleven candidates were found;
Chris Paro checked their status on 2026-11-09: US10516376B2 has lapsed for fees, and the other ten
are in force. Reading their independent claims (same day) found that eight do not read on the
design: none claims the TPT state-variable filter, its output mixing, the bilinear transform with
prewarping, Butterworth cascades of these sections, or ramping in log frequency. Two were Unclear,
both on the planned order change (fading to a second, preallocated cascade built for the new
order): Sony US9906197B2 (to 2034) and Casio US10311845B2 (to 2038), the latter leaning towards
reading for orders 3 to 8. That was raised with `flag-risk` (tag `patent`).

## Decision

- **Topology.** Every filter is a trapezoidal-integrator state-variable filter in Simper's form,
  its output a mix `m0·input + m1·band + m2·low`, plus a trapezoidal one-pole. Designers give
  low-pass, high-pass, bell, low and high shelf, and notch; frequency is prewarped (`tan(π·f/fs)`)
  and held at or below 0.49·fs.
- **Butterworth.** `high-pass` and `low-pass` are cascades of up to four two-pole sections with pole
  Qs `1/(2·sin((2i+1)π/2n))`, and a one-pole for an odd order, all sharing one prewarped `g`.
  Parameters `cutoff` (20 Hz to 20 kHz) and `order` (1 to 8).
- **Order changes only while stopped** (decided by Chris Paro in answer to the flag). `order` is
  marked stopped-only (ADR-0087): the UI offers it only while
  playback is stopped or paused, and no automation curve may drive it. The node applies a new order
  at once and clears its state. There is no crossfade between cascades.
- **Parametric equaliser.** `parametric-eq` has 8 fixed bands in series, each with `enabled`, `type`
  (bell, low shelf, high shelf, notch), `frequency`, `gain` and `q`.
- **Smoothing.** Cutoff and band frequency ramp in log2(Hz), Q in log2, and gain in dB, over
  `RAMP_SECONDS`, with coefficients recomputed every sample while moving. A band's enable or type
  change interpolates `g`, `k` and the mix linearly from the coefficients in use to the new
  response's; "off" is the identity mix at the same `g` and `k`. An idle band is skipped, and its
  state cleared when its fade-out ends.
- **Saved identifiers.** The kind ids `high-pass`, `low-pass` and `parametric-eq`, the parameter
  names, and the order of the band types are part of the saved format, stored through the existing
  `SavedNodeKind::Effect`, so `format_version` is unchanged (as ADR-0044 did for the mixer);
  confirmed by Chris Paro through `flag-risk` (tag `product`).

## Alternatives considered

- **Direct-form biquads (RBJ cookbook).** Equally standard, but their coefficients cannot change
  every sample without risking instability and zipper noise; smoothing them needs coefficient
  interpolation tricks or a crossfade.
- **Crossfading to a second cascade on an order change.** Click-free while playing, but two
  in-force patents were Unclear on it; rejected in the flag.
- **Morphing the order in place** (unused sections at identity, each section's Q and mix ramped to
  the new order's). Click-free and outside the two claims as read, but the order is rarely changed
  while playing, and it complicates the node for that rare case; not chosen.
- **Dipping to silence and switching.** Clearly outside the claims but leaves an audible dropout on
  every change.
- **Switching instantly while playing.** Clicks, which the issue rules out.

## Consequences

- The order cannot be automated. Any later automation of effect parameters must refuse
  stopped-only ones.
- If playback is stopped while input monitoring passes through a filter, an order change may still
  click there.
- A morph between very different responses (a notch at the sine's own frequency to a +12 dB shelf)
  briefly overshoots the final level for up to 10 ms; tests allow a linear-ramp corner, no more.
- Patents US9906197B2 and US10311845B2 must be reread if a crossfade between filter instances is
  ever proposed. Rows 1 to 3 of the research note are to be reread if the design allows a zero
  cutoff, a combined Q/gain control, or band frequencies tied together.
