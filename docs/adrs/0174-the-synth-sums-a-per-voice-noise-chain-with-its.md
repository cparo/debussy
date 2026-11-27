# ADR-0174 — The synth sums a per-voice noise chain with its own envelopes, velocity and LFO, and a voice stays busy while that chain sounds

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-26
- **Issue:** #209
- **Architecture:** [Synthesizer](../architecture/instruments.md#synthesizer)

## Context

Issue #209 (roadmap #20) asks for a noise source beside the synth's two oscillators: noise shaped
by a filter that tracks the note, with an envelope of its own, so a patch can add breath, chiff or
a percussive attack. The pink and brown generators already exist in `dsp` (ADR-0122, ADR-0171),
and `dsp::SvfCoefficients::bell` already gives a bell. Adding parameters changes the saved project
format, which went to a human through `flag-risk` (`product`). Chris Paro approved saving them by
name, with an absent name taking its default, and asked for the noise amp envelope to have its own
velocity sensitivity.

## Decision

- Each voice has a noise chain: white, pink or brown noise (`noise type`; each ×2 so all three
  share the uniform values' level at 1 kHz) through a bell, then a noise amp envelope, times
  `noise level` and the noise LFO's tremolo. It is summed with the oscillator chain after that
  chain's amp envelope and tremolo, before `gain`.
- The bell's centre is the note plus the pitch bend (not the oscillator LFO), plus `noise filter
  octave`, plus `noise filter envelope amount` times a noise filter envelope, plus the noise LFO,
  all in octaves, clamped as the cutoff is. Its gain is `noise filter gain` plus the noise LFO,
  clamped to ±24 dB; its width is `noise filter q`.
- The noise amp envelope peaks at `1 − noise amp velocity · (1 − velocity)`, the sensitivity read
  at the note-on and carried with a stolen voice's pending note.
- A second LFO, of its own rate, shape and depth, moves the bell's frequency (±4 octaves), its
  gain (±24 dB) or the noise's amplitude.
- A voice is free only when its amp envelope is idle, its noise chain no longer sounds and no
  note is pending. A released voice is one whose amp envelope releases, or has ended while its
  noise still sounds; the quietest is chosen by the louder of the two envelopes.
- The chain is on while `noise level` (its target or its ramp) is above 0. While off it adds
  nothing and never keeps a voice busy, so a synth at `noise level` 0 plays bit for bit as before.
- Voice *i*'s noise is seeded with a fixed seed plus *i*, so the note preview's lone voice sounds
  as a live synth's first voice does.
- The new parameters are saved by name, like every parameter; a project without them loads at
  their defaults, `noise level` 0 among them.

## Alternatives considered

- **Feed noise into the oscillators' low-pass, as a third mixer input.** Simpler, but the noise
  could not be shaped apart from the tone, which is what the issue asks for.
- **Share the amp envelope and `amp velocity`.** Fewer parameters, but a noise attack that decays
  under a sustained tone, the main use, would be impossible.
- **Let the voice end with its amp envelope.** Simpler lifetimes, but a noise release longer than
  the tone's would be cut off.
- **A noise LFO shared with the oscillator LFO.** Fewer parameters, but the bell could not move
  at a rate of its own.

## Consequences

- The synth has 50 parameters; the dialog's single row of groups grows long, which #210 lays out
  in two rows.
- Each sounding voice runs one more envelope pair, a generator and a bell per sample while the
  chain is on.
- No new algorithm: the generators and bell are the existing `dsp` ones, already cleared.
- A test pins the passage hash of a level-0 synth to the hash before this change (debug build,
  macOS).
