# ADR-0120 — Chorus and flanger are LFO-modulated delays read by 4-point Hermite interpolation

- **Status:** Accepted
- **Decided by:** agent, ratified by Chris Paro
- **Date:** 2026-11-13
- **Issue:** #62
- **Architecture:** [Modulated delay](../architecture/modulation.md#modulated-delay)

## Context

Issue #62 asks for chorus and flanger with a design pass covering the modulated-delay
interpolation, LFO shapes and stereo spread. The delay reads only whole-frame taps; a moving
tap needs a fractional read, and the interpolation sets how much a sweeping delay dulls and
aliases. Flanging needs delays under 1 ms with feedback, below the graph's 1 ms minimum
feedback delay (ADR-0031). Patent evidence for every element is in ADR-0119 and
[modulation-patents.md](../research/modulation-patents.md): no in-force claim found reads on the
modulated delay, the Hermite read, the chorus or the flanger.

## Decision

- Two effect kinds, `chorus` and `flanger`, over one modulated-delay core in `audio_effects`.
- The fractional read is `dsp::TapDelay::read_cubic`, 4-point, 3rd-order Hermite (Catmull-Rom).
- One LFO per node drives both channels; the right channel reads it `spread` degrees ahead.
- The chorus has 1 to 4 voices at evenly spaced LFO phases, sweeping from `delay` up to
  `delay + depth`, summed × 1/√n, with no feedback. `voices` changes live by ramping each voice's
  gain; all four always run.
- The flanger sweeps `delay·(1 + depth·lfo)` and keeps its feedback, −95 to +95 %, inside the
  node.
- The kind ids, parameter names, groups, ranges and choice orders in
  `docs/architecture/modulation.md` are saved format. The LFO note list is 1/16 to 4/1.
- Neither kind has a display curve (`curve: None`), as ADR-0105 allows.

## Alternatives considered

- **Linear interpolation** (`read_fractional`, as the plate's modulated allpasses use). Cheaper,
  but it low-passes the wet signal by an amount that moves with the fraction, an audible
  shimmer at chorus depths.
- **Allpass interpolation.** Flat magnitude, but its state makes a swept delay produce
  transients, and it is poor near a fraction of 0.
- **Higher-order Lagrange or windowed sinc.** Better at high frequencies, at several times the
  cost per voice, with more latency at the clamp.
- **One `modulation` kind with a mode.** One node, but chorus and flanger want different
  ranges, defaults and parameters (voices against feedback); two kinds keep each dialog simple.
- **Flanger feedback through the graph.** Graph feedback delays are at least 1 ms (ADR-0031),
  longer than a flanger's shortest delays.
- **A chorus sweep centred on `delay`.** At 5 ms with 10 ms depth it would go below zero.

## Consequences

- A display for the two kinds is a follow-up, #149.
- Through-zero flanging and bucket-brigade or tape models are not built.
- Smooth random and other LFO shapes can be added to the `shape` choice later only at the end
  of its list.
