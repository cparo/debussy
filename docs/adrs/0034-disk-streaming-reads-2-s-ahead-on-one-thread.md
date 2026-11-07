# ADR-0034 — Disk streaming reads 2 s ahead on one thread, primes before playing, and reports underruns

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-06
- **Issue:** #32
- **Architecture:** [Disk streaming](../architecture/engine.md#disk-streaming)

## Context

[ADR-0008](0008-project-audio-is-streamed-from-disk.md) streams project audio from disk, for at
least 32 tracks and projects several hours long, and names an underrun policy only as a
consequence: a late buffer plays silence and is reported, not waited for. The read-ahead depth,
the behaviour on seeks, underrun reporting and threading were open, and were put to the developer.

## Decision

- **Depth:** 2 seconds of each playing track, read ahead of the playhead. At 96 kHz, mono `f32`,
  that is about 768 KB per track, or 25 MB for 32 tracks.
- **Threads:** one reader thread, serving tracks in earliest-deadline order.
- **Start and seek:** playback starts once every track has 100 ms buffered at the new position.
  Meanwhile the real-time thread outputs silence; it never blocks. Loop starts are read ahead so
  a loop wraps seamlessly.
- **Underruns:** a late buffer plays silence, which ADR-0008 already requires. Each underrun is
  counted in a per-track atomic counter that the real-time thread writes without allocating. The
  UI polls the counters and shows a non-modal notice naming the track, suggesting a larger buffer
  or a faster disk. Underruns never stop the transport or recording.

## Alternatives considered

- **0.5 s or 5 s of read-ahead** — 0.5 s leaves little margin for a disk stall; 5 s costs 2.5×
  the memory and a longer refill after each seek, for margin an SSD does not need.
- **Start at once after a seek** — no wait, but the first moments after every jump would drop
  out and count as underruns.
- **Counters only, or stop on underrun** — counters alone leave users hearing dropouts without
  knowing why; stopping is disruptive during live recording.
- **A pool of reader threads** — overlaps I/O on slow or networked disks, but adds coordination;
  the real-time side is the same either way, so a pool can be added later.

## Consequences

- A slow spinning disk under heavy load can still underrun at 2 s; the notice tells the user.
- Seeks have a short startup wait, typically a few milliseconds on an SSD.
- `disk_streaming` (#32) can be built on `audio_buffers` and `wav_files` now.
