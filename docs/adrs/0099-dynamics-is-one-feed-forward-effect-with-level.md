# ADR-0099 — Dynamics is one feed-forward effect with level ballistics, a soft ceiling and presets, and no windowed-peak limiter path

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-12
- **Issue:** #59
- **Architecture:** [Dynamics](../architecture/effects.md#dynamics)

## Context

Issue #59 asks for a compressor, limiter and gate. These are one transfer curve with different
values, so a single effect with presets avoids three near-copies. The audio core is built
in-house (CLAUDE.md "Dependency Policy"), and needs patent evidence first
(`docs/architecture/patents.md` "Patent policy").

The evidence is in `docs/research/dynamics-patents.md`. It lists eleven candidates. Chris Paro
checked their status on 2026-11-12: US9661438B1 and US7729673B2 have lapsed for fees, EP3588776B1
is revoked, and the other eight are in force. A reading of their independent claims on the same
day found six that do not read on the design: US9608588B2 (adaptive time constants), US9979369
(gain division and multiplication by a stored final gain), US11252507B2 (timer from a threshold
crossing before attack), US10636448B2 (cross-fading two lookaheads), US8983092B2 (displacement
model) and US11011180B2 (transient-adaptive times). Two were Unclear, both on the planned
brickwall path, a sliding-window maximum then a moving average of the same length over the
lookahead window, combined by minimum:

- **US9559650B2** (Music Tribe, to 2034): whether a sample-peak level is a "loudness level".
- **US11394356** (Amazon, to 2041): whether a gain from an average of window maxima is "based on
  the threshold value and the first maximum amplitude value".

That was raised with `flag-risk` (tag `patent`).

## Decision

- **One kind, `dynamics`, with presets.** Compressor, Expander, Gate and Limiter are entries in
  `audio_effects::PRESETS` that set some parameters over the defaults. A kind with presets is
  offered only through them, and the preset is not saved.
- **Feed-forward, with ballistics on the level.** The detector is peak (`max(|L|,|R|)`) or RMS
  (`max(L²,R²)` as a power). One one-pole uses attack while rising and release while falling, and
  `hold` delays release after the last rise. The static curve is applied to the smoothed level and
  the gain is not smoothed again.
- **Curve.** A threshold, a slope above it (compression) and below it (expansion, clamped at
  `range`), and a quadratic knee that joins the two lines with level and slope continuous. The
  knee is Giannoulis, Massberg and Reiss (2012) applied to both sides. Makeup is added. The
  optional ceiling is a quadratic-knee shoulder on the output level.
- **No windowed-peak path** (decided by Chris Paro in answer to the flag). The ceiling acts on the
  smoothed level only, so it is a soft limit. The Limiter preset uses a 5 ms lookahead and a
  0.2 ms attack, and overshoots by about 0.1 dB on a steady 0 dBFS sine and 0.15 dB on a +6 dB
  burst.
- **Lookahead** delays the audio by up to 10 ms. It is the node's latency and sets latency
  (ADR-0100).
- **Saved identifiers.** The kind id, the parameter names and the order of the detector options
  are part of the format. They are stored through the existing `SavedNodeKind::Effect`, so
  `format_version` is unchanged, as ADR-0088 did.
- **∞ display.** A parameter may name a label shown at its maximum. `ratio above` and
  `ratio below` show ∞ there.

## Alternatives considered

- **Brickwall ceiling path** (sliding max, then a moving average, then the minimum with the curve
  gain). It guarantees the ceiling with lookahead, but two in-force patents are Unclear on it.
  Rejected in the flag; issue #135.
- **No lookahead at all.** It is furthest from both patents, but it gives a weaker limiter, and
  row 1's lookahead reading is Unclear only together with the windowed path. Not chosen.
- **Building as designed on a prior-art position** (musicdsp.org 2009, the expired US6535846B1).
  Not chosen without counsel.
- **Separate compressor, limiter and gate kinds.** They would duplicate the curve, and saved
  projects would hold three ids for one processor.
- **Smoothing the gain rather than the level.** It is the other common topology. Smoothing the
  level keeps the static curve exact at steady state and lets a display share `transfer_db`.

## Consequences

- The limiter is not a true-peak or brickwall limiter. A future brickwall path must reread
  US9559650B2 and US11394356 (#135).
- An Unclear point remains on US9559650B2 claim 55 for RMS mode with lookahead. It turns on "a
  non-linear function of at least two" level values, and the design takes each gain from one
  smoothed value. Reread it if the gain ever combines several level values.
- US10636448B2 must be reread if the lookahead is ever changed while playing by cross-fading.
- Sidechain input (#133) and a curve display with a gain-reduction meter (#134) are follow-ups.
