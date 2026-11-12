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
| `amplifier` | Amplifier | "Amplifier" |

A kind id this build does not know plays as silence and keeps its saved values.

## Effect presets

A preset is a name, an effect kind and values for some of its parameters. Choosing one in a node's
dialog sets every parameter to the preset's value or its default, as one undo step; while playing,
a preset that would change a stopped-only value is refused. The dialog shows the preset whose
values, over the defaults, equal the node's, or "Custom" if none does: the preset is not saved, and
a new node at its defaults shows the preset that has no values. The presets are
`audio_effects::PRESETS` ([ADR-0099](../adrs/0099-dynamics-is-one-feed-forward-effect-with-level.md)).

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

## Frequency response

`audio_effects::frequency_response` gives the steady-state magnitude response of an effect kind at
given parameter values and sample rate: the stages it runs, designed by the same functions as
processing, with each stage's gain the exact response of the bilinear-transformed filter. Ramps
and band morphs are not included. Every kind in "High-pass and low-pass filters" and "Parametric
equaliser" has one; a band that is off adds no stage ([ADR-0092](../adrs/0092-equaliser-displays-plot-the-exact-digital.md)). The
equaliser's response includes the stages of each enabled pass stage, and a pass stage that is off adds none
([ADR-0094](../adrs/0094-the-parametric-eq-has-four-bands-framed-by.md)).

## Dynamics

`dynamics` is one feed-forward processor that is a compressor, expander, gate or limiter by its
values ([ADR-0099](../adrs/0099-dynamics-is-one-feed-forward-effect-with-level.md)). Per frame:

1. **Detect.** Peak takes `max(|L|, |R|)`; RMS takes `max(L², R²)`. Both channels share one gain.
2. **Smooth the level.** A one-pole with the attack time constant while the detected value is above
   the level and the release time constant while it is below; release waits `hold` after the last
   frame that raised the level. RMS smooths power and takes its square root. The gain is not
   smoothed again.
3. **Curve.** With threshold `T` and knee `W`, the smoothed level `x` in dB gets a gain of
   `(x − T)·(ratio below − 1)` below `T − W/2`, clamped at −`range`, and `(x − T)·(1/ratio above −
   1)` above `T + W/2`; inside the knee a quadratic joins them with level and slope continuous.
   `ratio above` at its maximum is infinite. `makeup` is added.
4. **Ceiling.** When enabled, the level after the curve and makeup is held at or below `ceiling`,
   with a quadratic knee `ceiling knee` wide. The ceiling acts on the smoothed level: a soft limit
   that can overshoot during attack. There is no peak-hold or windowed-maximum path.
5. **Apply** the gain to the input delayed by `lookahead`.

`dsp::transfer_db` gives the steady-state curve, for processing and display alike. The curve's
continuous values ramp over the parameter ramp time; turning the ceiling on or off ramps its
share of the gain.

| Name | Group | Range | Default |
|---|---|---|---|
| `threshold` | curve | −80 to 0 dB | −18 |
| `ratio above` | curve | 1 to 100, logarithmic; 100 is ∞ | 4 |
| `ratio below` | curve | 1 to 100, logarithmic | 1 |
| `knee` | curve | 0 to 24 dB | 6 |
| `range` | curve | 0 to 96 dB | 96 |
| `ceiling enabled` | ceiling | off (0) or on (1) | off |
| `ceiling` | ceiling | −24 to 0 dB | −1 |
| `ceiling knee` | ceiling | 0 to 12 dB | 0 |
| `attack` | timing | 0.01 to 200 ms, logarithmic | 10 |
| `release` | timing | 1 to 3000 ms, logarithmic | 150 |
| `hold` | timing | 0 to 500 ms | 0 |
| `detector` | timing | peak (0), RMS (1) | peak |
| `lookahead` | timing | 0 to 10 ms; sets latency | 0 |
| `makeup` | output | −24 to +24 dB | 0 |

The latency is `lookahead` in whole frames at the graph's rate. `lookahead` is a latency-setting
parameter (see `docs/architecture/signal-graph.md` "Effect nodes").

Presets: **Compressor** (the defaults); **Expander** (threshold −40, ratio below 2, ratio above 1);
**Gate** (threshold −50, ratio below 100, ratio above 1, range 80, attack 0.1, hold 50, release
100); **Limiter** (ratio above 1, ceiling on at −1, lookahead 5, attack 0.2, release 100).

## Amplifier

`amplifier` is a memoryless waveshaper between a pre-amp and an output gain, per channel. Per frame:

1. **Pre-amp.** `u = drive·x + bias`, with `drive` as a linear gain.
2. **Shape.** `f(u)`, one of: warm `(2/π)·atan((π/2)·u)`, crunched `tanh(u)`, harsh `u` clamped to
   ±1. Each has slope 1 at 0 and is bounded by ±1. `audio_effects::Shape::apply` is the function.
3. **Makeup.** Multiply by `makeup` as a linear gain.
4. **DC filter.** A 4th-order Butterworth high-pass at 20 Hz, at the graph's rate.

With `oversampling` on, steps 1 and 2 run at 2 or 4 times the graph's rate inside a
`dsp::Oversampler`: the `Quality::Device` resampler up, the shaper on every sub-sample, the same
resampler down. Its delay is a whole number of base frames and is the node's latency; at factor 1
there is no filtering and no latency. `drive` and `makeup` ramp in dB and `bias` ramps linearly,
per base frame, held over its sub-samples. A change of `shape` crossfades linearly from the old
shape's output to the new one's over the parameter ramp time.

| Name | Group | Range | Default |
|---|---|---|---|
| `drive` | pre-amp | −12 to +36 dB | 0 |
| `bias` | pre-amp | −1 to +1 | 0 |
| `shape` | shaper | warm (0), crunched (1), harsh (2) | warm |
| `makeup` | output | −36 to +12 dB | 0 |
| `oversampling` | output | off (0), 2× (1), 4× (2); sets latency | off |

`oversampling` is a latency-setting parameter (see `docs/architecture/signal-graph.md`
"Effect nodes"). `audio_effects::amplifier_curve` gives the static curve
`makeup·(f(drive·x + bias) − f(bias))` for display. There are no presets
([ADR-0102](../adrs/0102-the-amplifier-is-a-memoryless-waveshaper-with-pre.md)).
