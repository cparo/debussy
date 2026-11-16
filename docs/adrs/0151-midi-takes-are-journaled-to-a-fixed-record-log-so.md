# ADR-0151 — MIDI takes are journaled to a fixed-record log so a crash cannot lose them

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-16
- **Issue:** #175
- **Architecture:** [Take recovery](../architecture/recording.md#take-recovery)

## Context

ADR-0150 collects a MIDI take's messages in memory on the writer thread, so a crash during a
pass lost every MIDI take while its audio takes were recovered from their WAV files and the
recording marker (ADR-0135). MIDI recovery needed a file on disk, written as the take goes, that
survives being cut off at any byte, and a marker that can name it. Both are persisted formats,
so the choice was put to a human (risk:product).

## Decision

Each MIDI take is journaled to `take-<n>.midilog` in `audio/`, written by the recording writer
thread beside the WAV files and synced as often (about once a second) and when the take ends.
The file is a 16-byte header (magic `DBMIDILG`, a little-endian `u32` version 1, a `u32` sample
rate), then one 16-byte record per message: a little-endian `u64` take frame, the status and two
data bytes (an unused one 0), and five zero bytes.

The recording marker goes to version 2: a track entry may name one `.midilog` file, and the
marker carries the MIDI placement latency. Version 1 markers are still read. On open, a journal
is read as untrusted input: a wrong magic, version or rate, a record that is not one channel
message, a frame that goes backwards, or more records than the project's MIDI event limit drops
that take; a partial last record is ignored. The take's length is the frame after its last
message, and it is placed as a live take is, in the same undo step as the pass's audio takes.

A journal is deleted once its take is placed or discarded, and cleanup deletes any left over. A
journal that is gone is taken as placed. A recovered MIDI clip identical to one already on its
track is not added again, since the window between saving the placement and deleting the
journal cannot be closed.

## Alternatives considered

- **Append JSON lines.** Readable, but variable-length records make a cut-off tail harder to
  tell from corruption, cost parsing and allocation per message, and are several times larger.
- **A Standard MIDI File written as the take goes.** Its track chunk's length sits in the
  header and needs repair like a WAV file's, it stores delta times in ticks rather than frames,
  and a reader for untrusted SMF is more surface than this needs.
- **Leave MIDI out of recovery.** A crash would keep losing performances that the audio beside
  them survives.

## Consequences

- Recording a MIDI track writes a small file per take (16 bytes a message) and syncs it once a
  second; a crash loses at most about a second of messages.
- A version 2 marker is discarded by builds before this one, losing its takes; they are not
  recovered by an older build.
- The journal duplicates what the writer collects in memory, which stays the source for a pass
  that ends normally.
- A take whose journal fails to write goes on in memory but is not protected; the status line
  does not say so.
