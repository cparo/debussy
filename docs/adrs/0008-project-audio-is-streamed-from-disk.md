# ADR-0008 — Project audio is streamed from disk

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-06
- **Issue:** —
- **Architecture:** [Audio storage](../architecture/engine.md#audio-storage)

## Context

Band recordings are long and multi-track: 32 tracks for an hour at 48 kHz in `f32` is about 22 GB,
more than RAM holds. The real-time thread must not do file I/O (CLAUDE.md, "Real-Time Audio
Thread").

## Decision

Project audio lives on disk and is streamed: a non-real-time thread reads ahead into preallocated
buffers and hands them to the real-time thread. Memory holds only bounded caches. The design
target is at least 32 tracks and projects several hours long.

## Alternatives considered

- **Everything in RAM, 16 tracks, ~10 minutes** — simpler, but caps the product below its primary
  use case.
- **In RAM now behind a storage seam, disk later** — defers the hardest part of the engine to a
  redesign once real projects depend on it.

## Consequences

- Needs a read-ahead scheduler, buffer handoff over lock-free queues, and a policy for underruns
  (a late buffer plays silence and is reported, not waited for).
- Waveform and spectrogram views need precomputed summaries stored alongside the audio, since the
  full audio cannot be scanned for each redraw.
- The on-disk audio format for recorded and imported audio is a persisted-format decision still
  to be made.
