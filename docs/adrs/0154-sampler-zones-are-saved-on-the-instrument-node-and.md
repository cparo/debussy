# ADR-0154 — Sampler zones are saved on the instrument node and their samples are loaded whole

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-16
- **Issue:** #174
- **Architecture:** [Sampler](../architecture/instruments.md#sampler)

## Context

Issue #174 adds a sampler: an instrument that plays the user's own WAV files, mapped to notes.
ADR-0147 made instruments their own node kind, saved as an instrument id and a parameter map;
parameters are numbers, and a sample map is not. The map needs a home in the model and the saved
format, its files must survive cleanup, and the audio thread may not read files or allocate, so
the samples must reach it already in memory.

## Decision

- **Zones are a field of `NodeKind::Instrument`.** A `zones` list sits beside the parameters. Only
  a `sampler` instrument may have zones; changing the instrument removes them.
- **Format version 11.** A saved instrument node may carry `zones`, omitted when empty. Version 10
  files migrate unchanged.
- **Zone files are bundle audio.** They are imported through the ordinary import path into
  `audio/`, at the project rate, and cleanup treats them as used.
- **Samples are loaded whole.** The engine bridge reads each zone's file fully into memory off the
  audio thread, caching by file name, and builds a new sampler node when the map changes. A zone
  sample is at most 60 s at the project rate; a longer import is refused.

## Alternatives considered

- **A separate top-level sampler map, keyed by node id** — keeps `NodeKind` unchanged, but splits
  one node's state across two places that must be kept consistent through undo, versions and node
  deletion.
- **Encoding the map in parameters** — parameters are automatable numbers; file names and per-zone
  modes are not, and the map's length varies.
- **Streaming samples from disk** — handles arbitrarily long samples, but needs a disk-reader
  thread, prefetch buffers per voice and underrun handling. One-shots and instrument notes are
  short; the cap keeps memory bounded (60 s of stereo at 48 kHz is about 23 MB per zone).

## Consequences

- Opening a version 11 project in an older build is refused, as with every format change.
- Long samples (loops, stems) cannot be zones; they stay on audio tracks. Streaming can be added
  later behind the same model if a need appears.
- Memory grows with the number of distinct zone files; repeated files are loaded once.
- The map editor UI follows in a second pull request (#174 part 2).
