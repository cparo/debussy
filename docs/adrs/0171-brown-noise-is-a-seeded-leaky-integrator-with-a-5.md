# ADR-0171 — Brown noise is a seeded leaky integrator with a 5 Hz pole

- **Status:** Accepted
- **Decided by:** agent, ratified by Chris Paro
- **Date:** 2026-11-26
- **Issue:** #207
- **Architecture:** [Patent policy](../architecture/patents.md#patent-policy)

## Context

The synthesizer's noise source (#209, then its dialog, #210) needs brown noise, whose power falls
6 dB per octave, beside `dsp::Noise` (white, ADR-0035) and `dsp::Pink` (ADR-0122). It runs on the
real-time audio thread, so it must not allocate, and a project must render the same every time.
`docs/architecture/patents.md` "Patent policy" requires patent evidence before an algorithm is
built in-house.

## Decision

`dsp::Brown` filters `Noise::uniform` values through one leaky integrator,
`y[n] = x[n] + a·y[n−1]`, with `a = exp(−2π·5 Hz / fs)`, the same corner as `Pink`'s lowest pole.
It reuses `Pink`'s first-order section with no zero, and the same normalisation: a gain computed
once per rate makes the magnitude 1 at 1 kHz (or a quarter of the rate, if lower), so brown and
pink noise are level-matched there. A rate too low for the pole gives the white values unfiltered.
State is in `f64`; it allocates nothing. The same seed and rate give the same sequence.

Patent evidence is in [brown-noise-patents.md](../research/brown-noise-patents.md). Stage 1 found
three candidates and named two more whose pages it could not fetch. Chris Paro checked all five on
2026-11-26 and found them in force: US9789024B2 (white cane, Alexander), US7460675B2 (Soft DB),
US8885844B2 (Continental, now Aumovio), US10827829B1 and US11918116B1 (Steelcase). Stage 2 read
their independent claims and none reads. The white cane claims need a cane with a noise generator
at its tip, and name Brownian noise only as one of a list. Soft DB's need a filter derived from a
measured room response. Continental's need silence detected in an audio system's output. The
Steelcase claims cover desk height control and do not mention sound. A one-pole low-pass of white
noise called brown noise is described in the Nord Modular manual (2003) and in Audacity's brown
noise code (as quoted on its forum in 2013). Gilman et al. (1963) on red noise is known only from
a catalogue record.

## Alternatives considered

- **A pure integrator (random walk)**: a true 1/f² slope down to DC, but it wanders without bound
  and needs clamping, reflection or a DC blocker, which bends the spectrum anyway.
- **A lower pole (1 Hz or below)**: a straighter slope at the bottom of the audible band, but a
  longer correlation, so slower settling and larger low-frequency excursions for no audible gain.
  5 Hz matches `Pink`.
- **Pink noise through a second leaky integrator** or **FFT shaping**: no better slope than one
  pole, at more cost; FFT shaping works in blocks, not as a stream.

## Consequences

- `Brown` is listed in `docs/architecture/patents.md` "Patent policy".
- The discrete integrator's slope bends near the Nyquist frequency (by sin(ω/2)); measured within
  1 dB per octave up to 12.8 kHz at 48 kHz.
- The evidence rests on web searches and Google Patents pages, not a patent database query, and
  the claims were not checked against the granted PDFs. Related Steelcase family patents
  (US9486070B2, US10038952B2, US10085562B1) were not opened (see the note's open uncertainties).
