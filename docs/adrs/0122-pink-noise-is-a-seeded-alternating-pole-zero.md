# ADR-0122 — Pink noise is a seeded alternating pole-zero filter cascade

- **Status:** Accepted
- **Decided by:** agent, unratified
- **Date:** 2026-11-13
- **Issue:** #149
- **Architecture:** [Patent policy](../architecture/patents.md#patent-policy)

## Context

The chorus and flanger display (ADR-0121) needs a broadband test signal with equal power per
octave, so that the gain shows evenly across a log frequency axis. `dsp` had only white noise
(`Noise`, ADR-0035). `docs/architecture/patents.md` "Patent policy" requires patent evidence
before an algorithm is built in-house.

## Decision

`dsp::Pink` filters `Noise::uniform` values through a cascade of first-order IIR sections,
`(1 − q·z⁻¹) / (1 − p·z⁻¹)`, with one real pole per octave from 5 Hz up to the Nyquist frequency
and a real zero half an octave above each, `r = exp(−2π·f/fs)` (at most 16 sections). A gain
computed once per rate makes the cascade's magnitude 1 at 1 kHz. State is in `f64`; it allocates
nothing. The same seed and rate give the same sequence.

Patent evidence is in [pink-noise-patents.md](../research/pink-noise-patents.md). Stage 1 found
four candidates with a printed term still running. Chris Paro checked their status on 2026-11-13:
US10615764B1 (HRL), US10841717B2 (Meyer Sound) and US8653354B1 (Sonivox) in force, US8994919B2
(ASML) lapsed. Stage 2 read the independent claims of the three in-force patents: none reads.
HRL claims summed stochastic generators updated by probability, Meyer Sound a composite of noise
and random impulses at a set crest factor, Sonivox pitched musical output with slit-spacing
parameters. Alternating real poles and zeros for 1/f filtering are described in Bristow-Johnson
(music-dsp, 1998) and in J. O. Smith's spectral-tilt filters (arXiv:1606.06154, 2016). The
earlier sources in that note, Keshner (1982) and Corsini and Saletti (1988), are known only from
index entries.

## Alternatives considered

- **Voss-McCartney row summing** — cheap, but its spectrum ripples and it is close to what the
  HRL patent claims.
- **Shaping white noise in the frequency domain** (FFT, scale by 1/√f, inverse) — exact slope,
  but it works in blocks, with seams or overlap, rather than as a stream.
- **Fewer, wider sections** (two per decade, or Bristow-Johnson's three) — more slope ripple.
  The cost of one section per octave is negligible here.

## Consequences

- `Pink` is listed in `docs/architecture/patents.md` "Patent policy".
- The evidence rests on web searches and Google Patents pages, not a patent database query, and
  the quoted claims were not checked against the granted PDFs (see the note's open
  uncertainties). A use of pink noise in a loudspeaker test signal mixed with impulses, or shaped
  into pitched tones, would need its own check.
