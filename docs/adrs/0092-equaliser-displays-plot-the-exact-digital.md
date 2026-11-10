# ADR-0092 — Equaliser displays plot the exact digital magnitude response at the project sample rate on fixed axes, labelled in an in-house stroke font behind a Typeface seam

- **Status:** Accepted
- **Decided by:** agent, ratified by Chris Paro
- **Date:** 2026-11-10
- **Issue:** —
- **Architecture:** [Equaliser displays](../architecture/user-interface.md#equaliser-displays)

## Context

ADR-0091 gave every effect dialog a display area, but no kind registered a display, so the high-pass,
low-pass and parametric equaliser dialogs showed blank charcoal. An equaliser's display should show
what it does to the spectrum, which needs the filters' response, the sample rate it is designed at
(the `EffectDisplay` contract did not pass it), and readable axis labels drawn into the pixel buffer
the contract returns.

## Decision

- The display plots the exact magnitude response of the filters as designed at the project's sample
  rate. The TPT state-variable and one-pole filters are bilinear transforms of analog prototypes, so
  `SvfCoefficients::magnitude` and `OnePoleCoefficients::magnitude` evaluate the prototype at
  `s = j·tan(π·f/fs) / g`. `audio_effects::frequency_response` builds the stages with the same
  designer functions processing uses, and sums their gains in dB.
- It shows the steady-state target, not the ramps or band morphs.
- Axes are fixed: 20 Hz to 20 kHz, +12 to −96 dB.
- Labels are rasterised by `StrokeFont`, an in-house face of straight strokes, behind a
  `Typeface` trait, so a font-file rasteriser can replace it by changing one line.
- `EffectDisplay::draw` takes a `DisplayContext` (kind, tone, sample rate, size) in place of loose
  arguments. This amends ADR-0091's contract and does not reverse it: displays are still
  registered per kind and return pixels.

## Alternatives considered

- **Measure the response from an impulse response and an FFT.** Rejected: it costs more per draw
  and loses resolution at low frequencies, while the closed form is exact. The impulse response is
  kept as the tests' check on the closed form.
- **A font crate (such as `fontdue` or `ab_glyph`) and a bundled font.** Rejected for now: it adds
  a dependency and a font asset with its own licence, for a set of labels that needs about twenty
  glyphs. The `Typeface` seam leaves room to adopt one later.
- **Draw labels as `iced` text over the image.** Rejected: ADR-0091 made displays plain pixels,
  testable without a toolkit, and overlaying widgets would split one display across two systems.
- **Axes that rescale to the response.** Rejected: fixed axes keep a curve's shape comparable while
  dragging, and +12 dB covers the useful boosts. A larger boost is drawn pinned to the top edge.
- **Pass the sample rate as one more argument.** Rejected: the list was already long, and a
  context struct can grow without changing every display's signature again.

## Consequences

- Processing and display share designers, so a change to a filter's design shows up in its display
  with no further work.
- Boosts above +12 dB (the equaliser allows +24) are clipped at the top of the plot.
- The stroke font covers only digits, `k`, `H`, `z`, `d`, `B`, signs and space. Other text needs
  new glyphs, or the font crate this record defers.
- The display is redrawn whenever the dialog is drawn. That costs a few thousand evaluations of the
  response and a full-size raster, which is acceptable at present sizes.
