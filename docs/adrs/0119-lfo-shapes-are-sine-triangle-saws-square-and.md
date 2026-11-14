# ADR-0119 — LFO shapes are sine, triangle, saws, square and sample-and-hold, without smooth random

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-13
- **Issue:** #62
- **Architecture:** [LFO](../architecture/modulation.md#lfo)

## Context

The plan for issue #62 offered six LFO shapes, one a smooth random shape: random points joined
by cosine or Hermite interpolation. Patent research
([modulation-patents.md](../research/modulation-patents.md)) found three in-force candidates.
Chris Paro checked their status on 2026-11-13 (all in force) and stage 2 read their claims:

- US10109265B2 (Casio): does not read. Its independent claims need an effect replaced on request
  and resumed from a parameter kept varying; tempo-synced LFOs appear only in dependent claims.
- US8060226B2 (Creative Technology): does not read. Its claims need interpolated input samples
  written at a fractional write address, or a fractionally modulated write pointer. The design
  writes whole input samples one frame per sample and interpolates on the read.
- EP3105753B1 (Jobomusic AG), and its US family member US9741329B2 found in stage 2: **Unclear**
  for the smooth random shape alone. Claim 1 is a waveform interpolated between control points
  changed by control signals; whether a sub-audio LFO is the claimed "digital audio signal" and a
  random draw a "control signal" decides it. The periodic shapes and sample-and-hold have no
  interpolated control points and do not read.

The basic art (LFO-modulated delay chorus and multi-voice phase-offset chorus, 1975 to 1980) is
expired. The question went to Chris Paro through `flag-risk` (tag `patent`).

## Decision

The LFO shapes are sine, triangle, saw up, saw down, square and sample-and-hold. No smooth-random
shape is built. Sample-and-hold values are a pure function of seed, cycle and stream through
`dsp::Noise` (cleared in ADR-0035). The phase accumulator, stereo phase offset and tempo-synced
period are built in-house on the evidence above.

## Alternatives considered

- **Smooth random by a low-pass on sample-and-hold values.** A plausible design-around, but it
  needs its own claims check before it ships; left for #150.
- **Build the interpolated smooth random as planned**, on a prior-art or claim-scope position.
  Rejected: possible US and EP (DE, GB, FR, ES, BE, IT, SE, CH) exposure until 2035-02-09.
- **Wait for expiry** on 2035-02-09. Equivalent for now; a design-around may come sooner.

## Consequences

- The chorus and flanger offer sine and triangle only.
- A smooth random shape needs a new ADR with a claims check of whatever design it uses.
- The evidence rests on web searches and Google Patents pages, not a patent database query; the
  note lists its gaps.
