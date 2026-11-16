# ADR-0148 — The synth's oscillators use MinBLEP, with PolyBLEP excluded by a Korg patent

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-16
- **Issue:** #173
- **Architecture:** [Patent policy](../architecture/patents.md#patent-policy)

## Context

Issue #173 plans a polyphonic subtractive synthesizer and requires patent research before its
oscillators, envelope, voice allocator and filter are built in-house (ADR-0023). The plan used
PolyBLEP oscillators. The research ([synth-patents.md](../research/synth-patents.md)) ran three
candidate passes, the last a class-filtered search of US grants in Google Patents (CPC G10H 1/18,
1/183, 1/057, 7/00, 2250/545) and full-text queries for band-limited steps and voice stealing.
Chris Paro checked every candidate's status on 2026-11-16; stage 2 read the claims of each one in
force:

- US7589272B2 (Korg, to about 2028-07-04): **reads** on the PolyBLEP saw and pulse, with high
  confidence. Claims 2, 15 and 60 recite a naive waveform summed around each discontinuity with an
  odd-symmetric alias-reducing pulse with essentially no DC bias; the PolyBLEP residual is that
  pulse. A triangle integrated from a PolyBLEP square reads through the square. Brandt's
  minimum-phase step (MinBLEP, ICMC 2001), which the patent's specification sets apart for its DC
  bias, does not read, and predates the patent's 2005-01-03 priority.
- US11202147B2 (Rossum): does not read, as for the equalisers (ADR-0088). The cutoff is modulated
  in log2 Hz, always positive and clamped below 0.49·fs.
- US8017856B2 (Roland): does not read; its claims need several timbre parts assigned to one note.
- US9064485B2 (Yamaha; its EP family member EP2775475B1 is not in force): does not read; its claims
  need chord information and a glide determination (medium confidence, claims seen in pieces).
- US11735152B2 (Roland): does not read; its claims need per-voice pitch-variation sets for
  simulated analog voices.
- US8053658B2 (Roland): does not read; its claims need the time between note starts to set the new
  note's attack.
- US9799313B2 (Yamaha): does not read; its claims need a decision, on a key-off, whether to sound
  another held note.
- US7728217B2 (Infineon) has lapsed for unpaid fees. Every older envelope and voice-stealing
  patent found has expired by term.

The question of the oscillators went to Chris Paro through `flag-risk` (tag `patent`).

## Decision

- The saw and pulse oscillators are naive phase accumulators corrected at each discontinuity by a
  minimum-phase band-limited step (MinBLEP) read from a table built in `prepare`; the triangle is a
  leaky integral of the MinBLEP square. No PolyBLEP, and no other zero-DC odd-symmetric residual,
  is built while US7589272B2 is in force.
- Built in-house on the evidence above: the ADSR envelope of exponential one-pole segments,
  retriggered from its current level; the per-voice state-variable low-pass with its cutoff
  modulated per sample in log2 Hz below 0.49·fs; voice allocation of a free voice, then the
  quietest released voice, then the oldest held voice, with a short fade-out of a stolen voice;
  12-TET note-to-frequency conversion, pitch bend scaled by a range in semitones, velocity scaling
  and CC 64 sustain. The LFO is reused under ADR-0119.

## Alternatives considered

- **PolyBLEP, as planned.** Reads on US7589272B2 with high confidence; rejected until the patent
  ends (about 2028-07-04), when it can be revisited by a new record.
- **Band-limited wavetables, one per octave.** Avoids the claims and is old art, but costs memory
  and an octave crossfade, and pulse-width modulation needs two phase-shifted saws.
- **Oversampled naive oscillators through the polyphase resampler (ADR-0036).** Avoids the claims,
  but costs the most per voice and leaves audible aliasing on high notes.
- **A licence, or leaving the oscillators out until 2028.** Neither is needed with a design that
  avoids the claims.

## Consequences

- MinBLEP needs a table built off the real-time thread and a residual longer than PolyBLEP's two
  samples, so each voice keeps a short buffer of pending corrections; its minimum-phase residual
  carries a little DC, which the voice's filter path must tolerate or remove.
- Features that would need a fresh reading first: hard sync (Korg claims 58 and 59), unison or
  stacked voices (US8017856B2), per-voice analog drift or a voice pool shared between parts
  (US11735152B2, to 2041), a legato mode or note-timing-dependent envelopes (US8053658B2), a mono
  note-priority mode (US9799313B2), and a chord-aware pitch bend (US9064485B2).
- The class-filtered search covered US grants only; non-US families and classes G10H 1/02, 1/46,
  5/00 and 7/08 to 7/12 were not searched by class (see the note's Open uncertainties).
