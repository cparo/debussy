# ADR-0014 — Project audio is stored as mono 32-bit float WAV

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-06
- **Issue:** —
- **Architecture:** [Stored audio](../architecture/project.md#stored-audio)

## Context

Recorded and imported audio must be stored in a form the read-ahead thread can stream cheaply
([ADR-0008](0008-project-audio-is-streamed-from-disk.md)), matching the engine's `f32` format
([ADR-0005](0005-the-engine-processes-audio-as-32-bit-float.md)). This is a persisted format, so
it was put to the developer as a `product` risk.

## Decision

Store each channel as a mono 32-bit float WAV at the project rate, switching to RF64 above 4 GB.

## Alternatives considered

- **Mono FLAC** — about half the disk space, but it adds decoding to the read-ahead path, and FLAC
  is integer-only, so `f32` imports would be quantised.
- **Interleaved WAV per take** — fewer files, but using one channel of a stereo take means reading
  both.

## Consequences

- No decoding when streaming, and samples load straight into engine buffers.
- About 690 MB per channel-hour at 48 kHz; 32 tracks for an hour is about 22 GB.
- WAV and RF64 reading and writing are simple enough to build in-house; that buy/build choice is
  made with the I/O crate.
- WAV is not patent-encumbered, so it falls outside the patent question for import/export formats.
