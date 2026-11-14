# ADR-0121 — Chorus and flanger displays plot the gain over 10 s of pink noise

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-13
- **Issue:** #149
- **Architecture:** [Modulation display](../architecture/effect-displays.md#modulation-display)

## Context

`chorus` and `flanger` shipped with no `Curve` (ADR-0120), so their dialogs showed a blank
charcoal display (#149). What they do changes over time with the LFO and differs between the
channels with `spread`, so no static curve shows it. The flanger's feedback and the chorus's
voice count shape the comb in ways an analytic sweep would have to re-derive.

## Decision

Both kinds give `Curve::Spectrogram(SpectrogramCurve)`. The effect's own per-frame processing
(`modulated_delay::response`, which shares `Running::new` with `prepare`) runs offline over
10 s of seeded pink noise (`dsp::Pink`, ADR-0122), the same noise into both channels. The plot
shows the output's level minus the noise's level per log-spaced band and time slice: the effect's
moving gain, without the noise's own grain. It is drawn in two stacked lanes, left above right,
with time across and 20 Hz to 20 kHz up, coloured on the waveform spectrum's ramp (now shared
as `theme::spectrum_colour`). The lane split and time marks moved from the impulse display into
`plot.rs` for both displays to share.

The simulation runs at the project's rate or 48 kHz, whichever is lower, with the whole note
rescaled to that rate. The analysis takes at most 480 columns (one every half window), each
stretched over whole pixels. Measured in a release build with 480 columns and 270 rows per lane,
the worst case (4-voice chorus) takes about 50 ms at any project rate, and the flanger about
20 ms. Before the two caps, a 192 kHz chorus took about 240 ms. Chris Paro chose the rate cap
over accepting that time or shortening the span at high rates.

## Alternatives considered

- **A plot of the LFO sweep** (delay over time) — cheap, but it shows the control, not the
  sound: no comb, feedback resonance or voice interference.
- **A static comb response** at the base delay — shows one moment of a moving effect, and the
  chorus's several voices have no single comb.
- **A spectrogram of the raw output** — the pink noise's own randomness fills every cell with
  grain; dividing by the dry spectrum leaves only what the effect does.
- **A mono plot** — hides `spread`, the one control that separates the channels.
- **Simulating at the project rate** — exact, but about 240 ms per redraw at 192 kHz, while a
  value is dragged; nothing on the display's axis lies above 24 kHz.
- **Shortening the span at high rates** — the time axis would differ by project rate.

## Consequences

- At rates above 48 kHz the picture comes from processing at 48 kHz, not the project's rate. The
  effect's parameters are in milliseconds and hertz, so the two agree up to interpolation and
  rounding of delay lengths.
- The display runs the effect for 10.5 s per redraw; a third modulated kind would plug in the
  same way, through `response::<S>`.
- A cell's gain is undefined where the noise has no power. Rows from the Nyquist frequency up
  (at 44.1 kHz and below, part of the top decade) stay background.
