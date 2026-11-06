# ADR-0011 — Editing is non-destructive over immutable audio files

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-06
- **Issue:** —
- **Architecture:** [Editing model](../architecture/project.md#editing-model)

## Context

The product needs time-shifting, trimming, and cut/copy/paste across tracks. Project audio is
hours long and streamed from disk ([ADR-0008](0008-project-audio-is-streamed-from-disk.md)), so
rewriting audio files on each edit would be slow and would make undo expensive.

## Decision

Audio files are immutable once written. Tracks hold clips (source file, offset, length, timeline
position), and edits change only clips. Unreferenced audio is reclaimed only by an explicit
cleanup.

## Alternatives considered

- **Destructive editing** — a simpler model, but every undo step would need a backup of the
  affected audio, and edits on long files would block on disk writes.

## Consequences

- Edits and undo are cheap metadata changes.
- Disk use grows until the user runs cleanup; the UI should show reclaimable space.
- Effects are applied at playback and export, never baked into source files.
