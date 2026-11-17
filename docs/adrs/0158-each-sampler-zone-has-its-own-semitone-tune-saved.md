# ADR-0158 — Each sampler zone has its own semitone tune, saved as format 12

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-16
- **Issue:** #174
- **Architecture:** [Sampler](../architecture/instruments.md#sampler)

## Context

In a kit, a sample usually sits on one key with its root on that key. Drummers and producers still
retune drums: a snare raised a semitone to sit with the song, or one tom sample played as low, mid
and high toms. The sampler offered only two ways to do this. One was its `tune` parameter, which
shifts every zone at once. The other was moving a zone's root away from its key, which reads as a
pitch when a one-shot row names its keys by drum (ADR-0156). Chris Paro chose a per-zone
semitone tune, accepting a change to the saved format.

## Decision

- **A tune for each zone.** Every zone has a `tune` in whole semitones, from −24 to 24, with 0 as
  the default. A voice's rate becomes `2^((note − root + zone tune + tune + bend · range)/12)`.
  This is the varispeed ratio cleared in ADR-0155, with one more constant offset.
- **Format 12.** A saved zone may carry `tune`, and it is left out when 0. Version 11 files
  migrate unchanged, and a tune outside ±24 is refused on open.
- **Edited per row.** Each zone row has a tune pick list ("−5 st" … "+12 st"), and a change is
  one undo step.

## Alternatives considered

- **Retuning by moving the root.** No format change is needed, but it reads backwards: the root
  goes down to tune up. In a one-shot row the root list names drums, so "root: low-tom" on a snare
  is meaningless.
- **Cents as well as semitones.** Fine detuning is sometimes wanted for drums, but it is a
  further control and a wider value range. Whole semitones cover retuning to a key; cents can come
  later as another field.
- **Only the sampler-wide `tune` parameter.** It can't make a tom set from one sample, or tune one
  drum without the rest.

## Consequences

- A project that uses a zone tune can't be opened by builds before format 12. No release has
  shipped formats 7 to 12; the last release, v0.1.0, wrote format 6.
- A zone's tune is a fixed setting, not a parameter, so it can't be automated.
