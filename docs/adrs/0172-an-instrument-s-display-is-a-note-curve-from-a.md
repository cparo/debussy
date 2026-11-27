# ADR-0172 — An instrument's display is a note curve from a registry column of its own

- **Status:** Accepted
- **Decided by:** agent, ratified by Chris Paro
- **Date:** 2026-11-26
- **Issue:** #208
- **Architecture:** [Synth note display](../architecture/effect-displays.md#synth-note-display)

## Context

The Synthesizer's dialog showed a blank display: instruments had no `audio_effects::Curve`, so
`effect_display::display_area` got `None`. #208 (part of the synth expansion, #20) asks for a
picture of one note, an A4 at velocity 127 held then released, as its waveform beside an
absolute-level spectrogram, in an area half an effect's height. It must follow parameter changes
and pick up the noise chain (#209) without further work. `audio_effects` and `instruments` are
independent core crates; only `debussy_ui` depends on both.

## Decision

- `instruments::InstrumentKind` has a `display: Option<NoteCurveFn>` column, `NoteCurveFn =
  fn(&[f32], u32) -> Option<NoteCurve>`, and `instruments::note_curve` looks it up, mirroring
  `audio_effects::curve`. The display asks `note_curve` for an instrument and `curve` for an
  effect, through a UI-side `Picture { Effect(Curve), Note(NoteCurve) }`.
- `NoteCurve` copies the values (with those that cannot change the note reset, so the display's
  cache ignores them) and the rate, capped at 48 kHz; `render` simulates lazily, and the display
  keeps the rendered `Note` while the curve is unchanged.
- The preview shares the live voice code: `Synth::process`'s per-sample frame building is a
  function (`Block::frame` over `Values`) both call, and the preview drives a one-voice `Pool`, its
  own `MinBlep`, `Lfo` (phase 0, fixed seed) and the existing `render`. A test checks the preview
  equals a live synth's output for the same note sample for sample.
- The span is fitted to the envelopes (longer attack + decay, plus 0.25 s hold, plus the amp
  release, at most 10 s), and the gate-off comes by three quarters of the span so the release
  always shows.
- The log-band reader and window sizing move from `audio_effects::gain_spectrogram` into
  `dsp::Bands` and `dsp::window_size`, shared by the modulation display and `Note::levels`.
- The note area is 160 logical pixels; `DisplayContext` carries the logical height, from which
  `plot::Canvas` derives its scale.

## Alternatives considered

- **A `Curve::Note` variant in `audio_effects`**: one lookup path, but `audio_effects` would depend
  on `instruments` (or the synth's voice code would move into the effects crate), coupling two core
  crates for a display.
- **A separate simplified model of the synth for the preview**: no change to `process`, but it
  would drift from what is heard, and #209's noise would need adding twice.
- **A fixed span**: simpler, but short envelopes would be a sliver and long ones cut off.
- **A copy of the band code in `instruments`**: no `dsp` change, but two copies of a subtle bin
  mapping.

## Consequences

- Rendering runs on the interface thread: about 31 ms for the 10 s worst case at 48 kHz plus 6 ms
  for the spectrogram (release build), once per change of the curve; a default patch's ~1 s span
  takes a few milliseconds.
- When #209 adds the noise chain to the voice's `render`, the display shows it with no further
  work; the noise must stay seeded so the picture repeats.
- The sampler has no note display; one could be added with a `display` function of its own,
  though it would need the project's audio.
- The preview is mono, which the synth is; a stereo instrument would need a second waveform lane.
