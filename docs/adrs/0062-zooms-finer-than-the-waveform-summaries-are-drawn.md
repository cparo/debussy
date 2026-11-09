# ADR-0062 — Zooms finer than the waveform summaries are drawn from the audio on every refresh, with no tile cache

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-08
- **Issue:** —
- **Architecture:** [Transport and seeking](../architecture/user-interface.md#transport-and-seeking)

## Context

Lanes could not zoom in past 64 frames per waveform column, the finest waveform-summary bucket:
about 1.4 s across a lane at 48 kHz. Editing at the sample level needs a closer view, down to
where single samples are visible. Below 64 frames per column the summaries have nothing finer to
offer, so those columns must come from the audio itself.

At that zoom the spectrogram's columns, each one 2048-frame window started at its column's first
frame, also stopped lining up with the waveform: at 2 frames per column a window reaches about
43 ms past its column, a whole lane's width.

## Decision

The finest zoom is 2 frames per waveform column (about 43 ms across a lane at 48 kHz). Columns
finer than 64 frames are summarised from the stored audio on every lane refresh, with no cache;
each column also takes in the first sample of the next, so the outline joins up when single
samples are visible. Each spectrogram column's window is centred on its column, and is silent
where it runs past either end of the file.

## Alternatives considered

- **A tile cache of raw-audio columns** — below 64 frames per column a lane covers at most 65,536
  frames per channel, which reads and summarises in well under a frame's time; spectrograms
  already compute every refresh from the audio the same way. A cache adds invalidation and memory
  limits for no visible gain.
- **Finer summary levels on disk** (a level at 2, 4, … frames per bucket) — grows every cache file
  by a large fraction of the audio itself to serve a view that only ever shows a few thousand
  samples.
- **Stop at 1 frame per column** — at one sample per column the min/max outline is just the
  samples, and 2 already shows each sample; one more √2 step pair buys little.
- **Keep spectrogram windows at the column's start** — the misalignment grows to a full lane width
  at the finest zoom.

## Consequences

Every refresh at the finest zooms reads up to 65,537 frames per channel from disk through the
already-open `WavReader`s; a slow disk would show as lag only at those zooms. The default zoom
level moves from 8 to 18 so a project still opens at 1024 frames per column. Centring moves each
spectrogram window by half a column less half a window at every zoom; at coarse zooms that is a
small fraction of a column and does not show.
