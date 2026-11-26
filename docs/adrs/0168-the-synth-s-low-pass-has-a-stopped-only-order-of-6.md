# ADR-0168 — The synth's low-pass has a stopped-only order of 6, 12, 18 or 24 dB per octave

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-26
- **Issue:** #206
- **Architecture:** [Synthesizer](../architecture/instruments.md#synthesizer)

## Context

Issue #206 asks for a choice of slope on the Synthesizer's low-pass, which until now was one
trapezoidal state-variable filter (12 dB per octave, ADR-0148). Patent policy (ADR-0023) needs
evidence before new filter topologies are built in-house, and a new choice parameter changes the
saved format (`docs/architecture/effects.md` "Effect kinds": names and option order are part of
it).

The research ([synth-filter-order-patents.md](../research/synth-filter-order-patents.md)) found
no in-force patent on a selectable 6/12/18/24 dB low-pass, on an SVF followed by a one-pole, or on
two equal-Q SVFs in series; every cascade patent it found (Moog, ARP, Rossum/Dow, Audio Precision,
Beltone and others) has expired by term. Chris Paro checked the three carried-over candidates on
2026-11-26; all are in force, and stage 2 read their claims:

- US11202147B2 (Rossum): does not read (high). Its claims need a resonant frequency that varies
  through zero; every stage shares the one cutoff, clamped positive and below 0.49·fs.
- US9722560B2 (Meyer Sound): does not read (high). Its claims need paired boost/cut shelving
  filters forming a band, with an order-dependent correction factor or added zeros; the synth has
  low-pass stages only, no band gain and no correction. Dependent claim 7's 6/12/18 dB slopes sit
  under those elements.
- US10311845B2 (Casio): does not read (high). Its claims need a crossfade, gradually decreasing
  the old filter and increasing a backup filter; the order here changes in one step, from rest.

Chris Paro answered the saved-format question (`flag-risk`, tag `product`) on 2026-11-26: add the
parameter with the default `12 dB`. Asked what `resonance` does at 6 dB, he chose to make the
order stopped-only, as the Equalizer's order is, and to grey out `resonance` while 6 dB is chosen
(the mechanism is ADR-0169).

## Decision

- A choice parameter `filter order` in group `filter`, options in this order: `6 dB`, `12 dB`,
  `18 dB`, `24 dB`; default `12 dB`. It is stopped-only and not automatable.
- Per voice: 6 dB is one TPT one-pole low-pass; 12 dB one SVF low-pass; 18 dB an SVF low-pass then
  the one-pole; 24 dB two SVF low-passes in series. Every stage in use takes the same per-sample
  cutoff, and both SVFs the same Q (`resonance`), not Butterworth pole Qs. No gain correction
  across orders.
- A new order takes effect at the next block, with every voice's filter stages reset to zero; no
  crossfade and no second, prepared filter set.
- `resonance` is enabled only while the order is 12, 18 or 24 dB.

## Alternatives considered

- **Crossfade to a second filter set on an order change.** Smooth while sounding, but the shape
  US10311845B2 (Casio, to about 2038) claims; rejected, as for the Equalizer (ADR-0088).
- **An order changeable while playing, applied from rest.** No crossfade, but a click on every
  change during a take, and the research flagged it as a trigger for re-reading Casio.
- **Butterworth pole Qs for the 24 dB order, with `resonance` on one section.** A flat passband,
  but the resonance would no longer act like the 12 dB filter's; equal sections are the usual
  synthesizer behaviour.
- **A resonant 6 dB order** (an SVF peak blended with a one-pole slope). Not a true 6 dB slope,
  and a new design needing its own research.
- **Default `24 dB`.** A common synth default, but older projects, which save only non-default
  values, would change sound.

## Consequences

- Projects saved before this load at `12 dB` and sound as they did. A later option must be added at
  the end of the list.
- Changing order with a voice still sounding (for example, a held note while stopped) steps its
  filter from rest, which can click. Adding any fade there, or allowing the change while playing,
  needs US10311845B2 read again first.
- The decisive claim clauses were read from Google Patents' page text, not the granted PDFs (see the
  note's Open uncertainties).
