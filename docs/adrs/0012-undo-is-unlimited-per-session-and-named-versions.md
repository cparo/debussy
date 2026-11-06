# ADR-0012 — Undo is unlimited per session and named versions persist earlier states

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-06
- **Issue:** —
- **Architecture:** [Undo history](../architecture/project.md#undo-history)

## Context

With non-destructive editing ([ADR-0011](0011-editing-is-non-destructive-over-immutable-audio.md)),
each edit is a small metadata change, so deep undo is cheap. The developer also wanted to recover
states from before the current editing session.

## Decision

Undo/redo is an unlimited in-memory command history, discarded when the project closes. A user can
save the current state as a named version, persisted in the bundle. A named version can be
restored as the current state (an undoable edit) or into a new project.

## Alternatives considered

- **Persist the undo history** — makes every command type part of the persisted format, needing
  migrations whenever a command changes, for a benefit named versions cover more simply.
- **Limited undo depth** — saves little memory, since commands only hold metadata.

## Consequences

- Named versions reference the same audio files, so a version costs only its metadata. Cleanup
  must keep every file that any named version references.
- Named versions share `project.json`'s format and migrations
  ([ADR-0013](0013-a-project-is-a-bundle-directory-with-versioned.md)).
- Restoring into a new project copies only the audio the version references.
