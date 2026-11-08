# ADR-0060 — The window is laid out as top bar, track pane beside the patch board, and bottom bar, with seeking that sets the return point

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-07
- **Issue:** —
- **Architecture:** [Layout](../architecture/user-interface.md#layout)

## Context

The first window (ADR-0056) was one column: a transport row holding every action, 64-pixel
waveform rows showing only a track's first channel, and a mixer strip. The routing graph could
already be edited and undone in the model, but the window offered no way to see or change it.
`Debussy Wireframe.png` sets out the intended window; its open questions (seeking, zoom, theme,
menus, the spectrogram, the order of delivery) were settled in a design session with the
developer.

## Decision

- The window is a top bar, a track pane (60 %) beside the port gutter and patch board (40 %), and
  a bottom bar. Until the patch board lands the right pane holds the mixer strips.
- Each channel has its own 128-pixel lane; a stereo track shows left above right under one header.
- Seeking (a click in a lane, or on the position bar) works during playback and sets the return
  point that stop goes back to. Rewind is gone; seeking to the start replaces it.
- Zoom steps by √2, anchored on the playhead if visible, else the view's centre. No gesture zoom;
  a sideways two-finger scroll pans.
- Audio is orange and control lavender in both tones; light/dark follows the system appearance
  and the toggle is not saved.
- Menus are flat buttons floated over the window (`pin` in a `stack`), closed by a click elsewhere.
  Unavailable items are greyed out, never hidden.
- Spectrograms are computed on demand from the stored audio, one 2048-point frame per spectrogram
  column (a quarter of the waveform columns), reduced to 64 log-spaced bands.
- Delivery is three pull requests: the shell, a read-only patch board, then graph editing.

## Alternatives considered

- **A menu bar crate or the native macOS menu bar** — iced 0.14 has neither built in; a crate
  would be a new dependency for a handful of menus, and the native menu bar is platform code
  outside the platform crates.
- **Zoom in steps of 2** — too coarse at the waveform's useful range; √2 steps keep two clicks
  equal to one doubling.
- **Saving the light/dark choice** — a setting to persist for a preference the system already
  holds; following the system covers the common case.
- **Stop always returning to where playback started** (the engine's rule) — loses the place the
  user chose when they pause, resume and stop; a return point set by seeking matches what they
  last asked for.
- **Spectrograms from the cached overview** — its 16 384-sample columns are too coarse at most
  zoom levels and no cache is built yet; computing on demand works at every zoom, at the cost of
  reading the audio for each redraw.

## Consequences

- Spectrogram redraws read one frame of audio per column per channel: panning a long project in
  spectrogram view costs disk reads that waveform view does not. Using the overview cache when
  zoomed far out is left open.
- New and open project, cut, copy, paste, record and automation curves are shown disabled until
  they exist.
- The patch board, its port gutter, and node dialogs follow in the next two pull requests.
