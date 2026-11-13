# ADR-0102 — The amplifier is a memoryless waveshaper with pre-amp bias, a Butterworth DC filter and optional polyphase oversampling

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-12
- **Issue:** #60
- **Architecture:** [Amplifier](../architecture/effects.md#amplifier)

## Context

Issue #60 asks for an amplifier simulation node in `audio_effects`, with a design pass and patent
evidence first, as amp modelling has active patents. Chris Paro's design: three static shaping
functions behind a pre-amp (gain plus bias) and a makeup gain, a DC-removing high-pass, and
optional 2× or 4× oversampling against aliasing. The tone stack is left to #137.

Patent evidence is in [`docs/research/amplifier-patents.md`](../research/amplifier-patents.md).
Six candidates had a printed term end in the future; Chris Paro checked their status on
2026-11-12: four (US8271109B2, US8275477B2, US8645144B2, US8258870B2) have lapsed for unpaid fees
and two are in force. Reading the claims found neither in-force patent reads on the design:
US8249275B1 (Cirrus Logic) claims switching among gain values whose time-weighted average is the
target, and US12177646B2 (Dolby) claims gains from object-audio bitstream metadata. Oversampling a
nonlinearity (US5789689, 1997), bias for even harmonics (US4995084, 1987) and crossfading shaping
tables (US4868869, 1988) appear in patents whose terms have ended. The claim quotes were joined
from Google Patents' text and should be checked against the granted documents. The search used
web search, not patent-database queries.

## Decision

- Per channel and frame: `u = drive·x + bias`, then `f(u)`, then `·makeup`, then a 4th-order
  Butterworth high-pass at 20 Hz. Bias is added after the drive gain, so it sets the operating
  point on the curve independently of drive.
- Three shapes, each with slope 1 at 0 and bounded by ±1: warm `(2/π)·atan((π/2)·u)`, crunched
  `tanh(u)`, harsh `clamp(u, −1, 1)`. A change crossfades linearly from the mix of shapes heard at
  that moment to the new shape over the parameter ramp time, so a second change mid-fade starts
  from where the first had got to instead of jumping.
- The DC filter reuses the filters' `Cascade`; it removes the steady offset `makeup·f(bias)`.
- Oversampling is `dsp::Oversampler`: the ADR-0036 resampler at `Quality::Device` up and down,
  the downsampler padded to an even length and the chain topped up with a short delay at the high
  rate, so the latency is a whole number of base frames (71 at 2× and 4× at 44.1, 48 and 96 kHz).
  The factor is a latency-setting parameter (ADR-0100). Ramps advance per base frame.
- The kind id is `amplifier`; its parameter names are new saved names under
  `SavedNodeKind::Effect`, and `format_version` is unchanged.

## Alternatives considered

- **Bias before the drive gain.** Drive would scale the offset too, so turning drive up would also
  move the operating point, and the two controls would not be independent.
- **A one-pole DC blocker.** It is cheaper, but its 6 dB per octave slope leaves more sub-audio
  content near 20 Hz; the 4th-order cascade already exists and is tested.
- **Half-band or minimum-phase oversampling filters.** They would be cheaper or lower in latency,
  but they would be a second filter design to prove and patent-check. The existing resampler is
  cleared and exact, and its latency is acceptable.
- **Switching shapes instantly.** A jump between transfer functions clicks at high drive.
- **Antiderivative anti-aliasing instead of oversampling.** It works per shape and needs each
  shape's antiderivative; oversampling covers any shape and is already covered by ADR-0036.

## Consequences

- At 4× the shaper runs four times per frame and each channel pays about two filter lengths of
  multiply-adds per frame. Measured on a 15 kHz sine at high drive, aliasing falls from −11 dB
  (off) to −18 dB (2×) and −46 dB (4×) relative to the fundamental. 2× helps less because the 5th
  harmonic folds back below 24 kHz at the doubled rate.
- Changing oversampling is refused while playing and replaces the node (ADR-0100).
- The tone stack is #137. The algorithms are listed in `docs/architecture/patents.md`
  "Patent policy".
