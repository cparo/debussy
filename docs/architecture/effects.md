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

## Frequency response

`audio_effects::frequency_response` gives the steady-state magnitude response of an effect kind at
given parameter values and sample rate: the stages it runs, designed by the same functions as
processing, with each stage's gain the exact response of the bilinear-transformed filter. Ramps
and band morphs are not included. Every kind in "High-pass and low-pass filters" and "Parametric
equaliser" has one; a band that is off adds no stage ([ADR-0092](../adrs/0092-equaliser-displays-plot-the-exact-digital.md)). The
equaliser's response includes the stages of each enabled pass stage, and a pass stage that is off adds none
([ADR-0094](../adrs/0094-the-parametric-eq-has-four-bands-framed-by.md)).
