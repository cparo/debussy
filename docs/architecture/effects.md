# Effects

## Effect kinds

Each built-in effect is named by a stable kind id, saved with its node; parameter values are saved
by parameter name (see `docs/architecture/project.md` "Routing"). Kind ids, parameter names and the
order of a choice parameter's options are part of the saved format. Every built-in effect has one
stereo input named `in` and one stereo output named `out`, processing left and right with separate
state.

| Kind id | Name shown | Section |
|---|---|---|
| `high-pass` | High-pass filter | "High-pass and low-pass filters" |
| `low-pass` | Low-pass filter | "High-pass and low-pass filters" |
| `parametric-eq` | Parametric EQ | "Parametric equaliser" |
| `dynamics` | Dynamics | "Dynamics" |

A kind id this build does not know plays as silence and keeps its saved values.

## Filters

Every filter is built from a trapezoidal-integrator (TPT) state-variable filter, whose output is a
mix of its input, band-pass and low-pass signals, and a trapezoidal one-pole filter. Frequencies are
prewarped for the bilinear transform and held at or below 0.49 of the sample rate. While a value
ramps, coefficients are recomputed every sample. Frequency and Q ramp in their logarithms; gain
ramps in dB ([ADR-0088](../adrs/0088-equalisers-are-built-on-a-trapezoidal-state.md)).

## High-pass and low-pass filters

`high-pass` and `low-pass` are Butterworth filters: a cascade of two-pole sections with the
Butterworth pole Qs, plus a one-pole section for an odd order. They are 3 dB down at the cutoff and
fall 6 dB per octave per pole beyond it.

| Name | Range | Default | Meaning |
|---|---|---|---|
| `cutoff` | 20 to 20 000, logarithmic | 80 (high-pass), 12 000 (low-pass) | Cutoff in Hz |
| `order` | 1 to 8, whole numbers | 2 | Poles: 6 dB per octave each |

`order` changes only while playback is stopped or paused: the UI does not offer it while playing
and no automation may drive it. A new order takes effect at once, from rest
([ADR-0088](../adrs/0088-equalisers-are-built-on-a-trapezoidal-state.md)).

## Parametric equaliser

`parametric-eq` is an optional high-pass stage, then four bands in series, then an optional
low-pass stage ([ADR-0094](../adrs/0094-the-parametric-eq-has-four-bands-framed-by.md)). The
parameters are listed, and shown, in that order.

Band N (1 to 4) has five parameters, grouped as `band N`:

| Name | Range | Default | Meaning |
|---|---|---|---|
| `band N enabled` | off (0) or on (1) | off | Whether the band acts |
| `band N type` | bell (0), low shelf (1), high shelf (2), notch (3) | bell | The band's response |
| `band N frequency` | 20 to 20 000, logarithmic | 100, 500, 2000, 8000 | Centre or shelf mid-point in Hz |
| `band N gain` | −24 to +24 | 0 | Cut or boost in dB; a notch ignores it |
| `band N q` | 0.1 to 18, logarithmic | 0.7071 | Width, or shelf slope |

Turning a band on or off, or changing its type, moves its coefficients linearly to the new
response over the parameter ramp time; a band that is off passes its input through and is not
processed ([ADR-0088](../adrs/0088-equalisers-are-built-on-a-trapezoidal-state.md)).

Each pass stage, grouped as `high-pass` or `low-pass`, has three parameters:

| Name | Range | Default | Meaning |
|---|---|---|---|
| `high-pass enabled`, `low-pass enabled` | off (0) or on (1) | off | Whether the stage acts |
| `high-pass cutoff`, `low-pass cutoff` | as `cutoff` in "High-pass and low-pass filters" | 80, 12 000 | Cutoff in Hz |
| `high-pass order`, `low-pass order` | as `order` in "High-pass and low-pass filters" | 2 | Poles |

A stage is the filter of "High-pass and low-pass filters", with the same design and the same
stopped-only order. Turning it on or off crossfades its output with its input over the parameter
ramp time; a stage that is off passes its input through and is not processed
([ADR-0094](../adrs/0094-the-parametric-eq-has-four-bands-framed-by.md)). Saved values for bands
5 to 8 match no parameter and are ignored.

## Dynamics

`dynamics` is a compressor, limiter, gate or saturator by its settings. In order, a sample is
amplified by `preamp` and delayed by `lookahead`, multiplied by a gain the transfer curve gives for
the signal's level and by its side's output gain, passed through a DC blocker when `asymmetric` is
on, and clipped at its side's ceiling. The parameters are listed, and shown, in this order, grouped
as `input`, `envelope`, `gain function`, `positive`, `negative` and `quality`; each label is the
name less its group ([ADR-0104](../adrs/0104-the-dynamics-controls-mirror-the-positive-and.md)):

| Name | Range | Default | Meaning |
|---|---|---|---|
| `preamp` | −24 to +48 | 0 | Input gain in dB |
| `link channels` | off (0) or on (1) | on | Whether left and right share one level |
| `attack` | 0, or 0.25 to 100 logarithmic | 10 | The window's future side, in ms; stopped-only |
| `attack exponent` | 0.1 to 16, logarithmic | 1 | Shape of the attack side; stopped-only |
| `release` | 0, or 0.25 to 5 000 logarithmic | 100 | The window's past side, in ms; stopped-only |
| `release exponent` | 0.1 to 16, logarithmic | 1 | Shape of the release side; stopped-only |
| `lookahead` | 0 to 100 | 0 | The window's reach ahead and the effect's delay, in ms; stopped-only |
| `asymmetric` | off (0) or on (1) | off | Whether negative samples use the `negative` values |
| `positive steepness` | 0.01 to 10, logarithmic | 1 | Sharpness of the curve's shoulder |
| `positive exponent` | 1/16 to 16, logarithmic | 1 | Below 1 raises quiet levels, above 1 lowers them |
| `positive output gain` | −24 to +24 | 0 | Gain applied with the curve's, in dB |
| `positive ceiling` | −48 to +24 | 0 | Clip level in dB |
| `negative steepness`, `negative exponent`, `negative output gain`, `negative ceiling` | as the positive ones | as the positive ones | The same for negative samples; enabled by `asymmetric` |
| `oversampling` | 1× (0), 2× (1), 4× (2), 8× (3) | 1× | Rate of the stages from the gain to the clip; stopped-only |

- **Level.** The level is a weighted RMS of the preamplified input. With time `t` in ms relative
  to the sample being shaped, positive ahead, the window's peak (weight 1) is at
  `p = lookahead − attack`; ahead of it the weight is `1 − ((t − p)/attack)^attack exponent` up to
  `p + attack`, behind it `1 − ((p − t)/release)^release exponent` back to `p − release`, and 0
  beyond. A side of zero length has no weight. Squares are summed over 0.5 ms blocks; at the end
  of each block the level is `√(Σ wₖ·Sₖ / (block length · Σ wₖ))` over the window's blocks, each
  weighted at its centre, so a full-scale square wave reads 1. Linked, both channels use the mean
  of their two weighted sums.
- **Gain.** The curve maps levels in dB. With the level normalised as `u = (dB + 96) / 96` (0 at
  −96 dB, 1 at 0 dB), steepness `s` and curve exponent `c`, the output is
  `−96·(1 − tanh((s·u)^c) / tanh(s^c))` dB, so −96 dB and 0 dB map to themselves; at or below
  −96 dB, and at silence, the level passes unchanged. The gain is the output level less the input
  level ([ADR-0102](../adrs/0102-the-dynamics-curve-maps-normalised-db-levels.md)). The gain moves linearly from its previous value to the new
  one over the block after it is computed. When asymmetric, a negative sample takes the gain of
  the `negative` steepness and exponent from the same level. Each side's output gain multiplies
  the sample with its curve gain, before the DC blocker; unless asymmetric, both sides use the
  positive one.
- **Waveshaper.** With `attack` and `release` both 0, each sample of each channel is its own
  level, so a sample's magnitude is mapped through the curve and its sign kept; `link channels`
  has no effect.
- **DC blocker.** A one-pole high-pass at 7 Hz, cleared when `asymmetric` is turned on.
- **Clip.** The output is held between `−negative ceiling` and `+positive ceiling` (both
  `positive ceiling` unless asymmetric), each as a linear gain. Oversampled, the down-sampling filter may ring slightly past
  it.
- **Oversampling.** The delayed signal is up-sampled by `dsp::Oversampler`, the gain and output
  gain, DC blocker and clip run at the higher rate, and it is filtered back down. Each direction is a
  linear-phase Kaiser-windowed sinc cut off at half the base rate, 80 dB down, delaying 16 base
  frames.
- **Latency** is `lookahead` in frames, plus 32 frames when oversampled.
- `preamp`, both steepnesses, exponents, output gains and ceilings ramp over the parameter ramp time;
  `link channels` and `asymmetric` take effect at once. The stopped-only parameters are read when
  the node is prepared.

([ADR-0099](../adrs/0099-the-dynamics-effect-is-a-weighted-rms-window.md)) No mode derives a gain
from a block's peak ([ADR-0100](../adrs/0100-weighted-rms-dynamics-may-be-built-in-house-no.md)).

## Frequency response

`audio_effects::frequency_response` gives the steady-state magnitude response of an effect kind at
given parameter values and sample rate: the stages it runs, designed by the same functions as
processing, with each stage's gain the exact response of the bilinear-transformed filter. Ramps
and band morphs are not included. Every kind in "High-pass and low-pass filters" and "Parametric
equaliser" has one; a band that is off adds no stage ([ADR-0092](../adrs/0092-equaliser-displays-plot-the-exact-digital.md)). The
equaliser's response includes the stages of each enabled pass stage, and a pass stage that is off adds none
([ADR-0094](../adrs/0094-the-parametric-eq-has-four-bands-framed-by.md)).
