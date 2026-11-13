# ADR-0114 — The delay's feedback low-pass is a first-order one-pole

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-13
- **Issue:** #61
- **Architecture:** [Delay](../architecture/delay-and-reverb.md#delay)

## Context

[ADR-0112](0112-the-delay-reads-whole-frame-taps-from-a-4-second.md) put a 2nd-order
state-variable low-pass (Q 0.707, 12 dB per octave) in the delay's feedback path. Trying it,
Chris Paro asked for a gentler roll-off.

## Decision

- The feedback low-pass is the trapezoidal one-pole low-pass (`dsp::OnePole`, ADR-0088) at
  `low-pass` Hz: 6 dB per octave per repeat.
- Everything else in ADR-0112 stands: the whole-frame taps, the tempo sync, the crossfades, the
  modes, the saturation and the parameters, whose names and ranges do not change.

## Alternatives considered

- **Keep the 2nd-order filter.** Repeats darken fast: each one loses 12 dB per octave above the
  cutoff, so a few repeats in, only a dull thump is left.
- **A 2nd-order filter at a lower Q.** It still falls 12 dB per octave well above the cutoff.

## Consequences

- At the same setting, later repeats keep more of their top end. Saved projects keep their
  values and sound brighter in the feedback.
- The one-pole is covered by ADR-0088, so there is no new patent question.
