# ADR-0135 — Takes interrupted by a crash are recovered from a marker file

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-14
- **Issue:** #110
- **Architecture:** [Take recovery](../architecture/recording.md#take-recovery)

## Context

A take's WAV files join the project only when the pass finishes. If Debussy crashes mid-take,
`WavWriter::finish` never runs, so the files' header sizes are zero, and nothing references them,
so cleanup deletes them on the next open (ADR-0049, ADR-0078 "Consequences"). A long take lost to
a crash is the worst loss a recording program can cause. Recovery means writing a new persisted
file and reading it, and the takes' headers, as untrusted input on open, so the choice was put to
the user under the `product` and `security` risk tags (#110).

## Decision

While a pass is recorded, a marker file, `recording.json`, in the bundle describes it: each armed
track and its files, the device latency, and, once capture begins, the start frame and plan
latency. It is written crash-safely and removed when the takes are placed or discarded.

On open, the marker is read within fixed limits (1 MB, at most 10,000 tracks, one or two plain
`.wav` names per track, no name twice). Each file's header sizes are rebuilt from its length,
accepting only the layout the writer produces. The takes are placed exactly as a normal stop
places them, automatically, as one undo step that is autosaved before the marker goes, with a
status line. Files the project already uses are skipped. A marker that is invalid, has no start,
or leaves nothing to place is discarded and its files left to cleanup; it never refuses the
bundle.

The writer thread flushes and syncs each take file about once a second, so power loss or an OS
crash also keeps most of the take.

## Alternatives considered

- **No recovery** — the status quo; loses whole takes to a crash, which users of a recorder do
  not forgive.
- **Repair without a marker** — repair any unreferenced WAV in `audio/` on open. Without the
  start frame and latencies the takes cannot be placed where they were played, and nothing says
  which track a file belonged to; files from elsewhere would be guessed at.
- **Ask before recovering** — a dialog at open. Adds a modal decision at a stressful moment; an
  automatic step that undo removes gives the same choice afterwards.
- **No periodic sync** — the marker alone survives an application crash, since the OS still
  writes the page cache, but power loss or a kernel panic could lose everything since the take
  began. A sync a second costs little next to the write itself.

## Consequences

- A new persisted file with its own `format_version`, independent of `project.json`'s.
- `project_storage` depends on `wav_files` to repair headers on open; a workspace crate, so no new
  third-party code.
- A crash between the autosave and removing the marker is harmless: the files are then
  referenced and skipped. A crash during the count-in leaves no start, so its files are deleted.
- Up to about a second of audio before a power loss can be lost; a partial last frame is cut.
- Overruns counted before the crash are not reported on recovery.
