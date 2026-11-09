# ADR-0068 — Lanes are rasterised in memory and blurred with a Gaussian, drawn through iced's image support

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-08
- **Issue:** —
- **Architecture:** [Layout](../architecture/user-interface.md#layout)

## Context

The lanes looked hard-edged: they are vector rectangles on an `iced` canvas, one per waveform
column, trace run or spectrogram cell, and `iced` 0.14 has no blur or filter for canvas geometry.
Smoothing them means producing pixels ourselves. Drawing pixels needs `iced`'s image support,
which is a feature (`image-without-codecs`) that ADR-0055 left off, so this is a `Cargo.lock`
change and was put to the developer (`flag-risk`, tag `license`).

## Decision

- **Buy:** `debussy_ui` turns on `iced`'s `image-without-codecs` feature. It adds `image` 0.25
  (no codecs), `byteorder-lite`, `moxcms`, `pxfm`, `kamadak-exif` and `mutate_once`, all under
  licences already in `deny.toml`'s allowlist.
- **Build the rest:** each lane is painted into an in-house raster (`debussy_ui/src/raster.rs`)
  at the window's scale factor, with partly covered edge pixels blended by coverage. It is
  blurred with a separable Gaussian of σ = 0.5 physical pixel (radius `ceil(3σ)`, edges repeated)
  and drawn as one image with nearest filtering. The playhead cursor stays a sharp vector
  rectangle over it.
- **Finer spectrograms:** 512 spectrogram columns per lane (one per two waveform columns) and
  192 log-spaced bands, up from ADR-0060's 256 columns and 64 bands. This amends that one point
  of ADR-0060; the rest of it stands.
- The raster is redrawn only when the lane's contents, the tone or the scale factor change; the
  app follows the scale factor through the window's `Rescaled` event.

## Alternatives considered

- **Smoothing in data space** (filtering bucket and spectrum values before drawing) — softens
  levels but leaves every column edge hard, which is what looked wrong.
- **A `wgpu` shader pass** — a true GPU blur, but it needs a custom `iced` shader widget, ties the
  lanes to the `wgpu` backend (the `tiny-skia` fallback would lose it), and is far more code.
- **No blur** — keeps zero new crates, but leaves the lanes looking as they did.
- **σ = 1 physical pixel** (the first cut), **256 × 64 spectrograms** (ADR-0060) — the developer
  tuned σ, the spectrogram columns and the bands by eye on a Retina display. At σ = 1 the lanes
  looked soft; at 256 × 64 the blurred spectrogram cells were still visible as blocks.

## Consequences

- Six more crates in the UI binary; `image` is used only for its pixel-buffer type, no decoding.
- Each cache miss costs a CPU raster and blur per lane (about 1–2 ms at 2x) and a texture
  upload; panning and zooming redraw, playback does not.
- Twice the spectrogram columns means twice the FFT frames per lane refresh, and three times
  the bands triples each column's size in memory.
- The blur is in physical pixels, so it looks the same width on any display.
- With image support on, later features (track icons, imported artwork) can draw images without
  another dependency decision.
