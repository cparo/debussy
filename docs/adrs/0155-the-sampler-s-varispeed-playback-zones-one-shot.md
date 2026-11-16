# ADR-0155 — The sampler's varispeed playback, zones, one-shot and gated modes and choke groups may be built in-house

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-16
- **Issue:** #174
- **Architecture:** [Patent policy](../architecture/patents.md#patent-policy)

## Context

Issue #174 plans a sampler built in-house, and ADR-0023 requires patent evidence first. The design,
element by element, is in `docs/research/sampler-patents.md` "Planned design": varispeed playback
stepped by `2^((note − root)/12)`, 4-point Hermite reads, key-range zones with a root key,
one-shot or gated playback, choke groups with a 3 ms fade, velocity to gain, and the synth's voice
allocation and ADSR. Searches found eight candidates whose terms could still run.

## Decision

The sampler as planned may be built in-house:

- **Status check.** Chris Paro checked all eight in Google Patents on 2026-11-16. All were marked
  "Active" at first; US11688377B2 (Intelliterran) was then found expired, fee related.
- **Claims.** Six in-force candidates had their independent claims read, and none reads:
  - US10224062B1 (Microsoft): needs interpolation between two filters' coefficient sets.
  - US9330649B2 (Apple): needs velocity-layer samples chosen afresh on a repeated velocity.
  - US8461445B2 (Yamaha): needs physical pads grouped into a zone, and a designated
    technique pad in the claims that mention choke-like instructions.
  - US9076264B1 (iZotope): claims a step-sequencer display.
  - US8035021B2 (Yamaha): needs page-wise burst reads from non-volatile memory.
  - US9514724B2 (Casio): needs sampling from a sound receiver followed by automatic pattern
    playback.

  Chris Paro read US12159610B2 (Intelliterran): it does not read. It interprets the duration and
  frequency of taps on a foot-operable switch to control loops or other programming signals, and
  the sampler is software only.
- **Prior art.** Pitch by address increment with interpolation (US4667556A, 1984; US5111727A,
  1990) and one-shot waveforms (US5288940A, 1991) are expired.
- **Already cleared.** The Hermite kernel was cleared in ADR-0120, and voice allocation, the
  ADSR and the sustain pedal in ADR-0148.

## Alternatives considered

- **Ship without choke groups until the Intelliterran claims were read.** This would have lost the
  open/closed hi-hat case. Once a human had read the claims and checked the status, it was no
  longer needed.
- **Accept a prior-art position on choke without reading the claims.** This rests on SoundFont
  `exclusiveClass` and MPC mute groups. It was rejected because it accepts risk on claims nobody had
  read.

## Consequences

- The sampler's design must stay clear of the claims read. Each of these changes needs a fresh
  reading, listed with end dates in the research note's "Open uncertainties":
  - pitch-dependent filter blending (US10224062B1, to 2038);
  - velocity layers re-chosen on repeated velocities (US9330649B2, to 2034);
  - grouping pads or a designated choke pad (US8461445B2, to 2029);
  - a step-sequencer grid (US9076264B1, to 2031);
  - streaming zone samples from flash in pages (US8035021B2, to 2030);
  - sampling into the instrument followed by automatic pattern playback (US9514724B2, to 2035);
  - foot-switch tap control of loops (US12159610B2, to 2034).
- The search had gaps: no database query filtered by class ran, and some hits were never opened.
  Both are recorded in the note.
