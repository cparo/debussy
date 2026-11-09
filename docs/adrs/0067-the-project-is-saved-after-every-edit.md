# ADR-0067 — The project is saved after every edit

- **Status:** Accepted
- **Decided by:** agent, ratified by Chris Paro
- **Date:** 2026-11-08
- **Issue:** —
- **Architecture:** [Saving](../architecture/project.md#saving)

## Context

The project was written only by File → Save. Quitting or closing the window exited without saving,
so every edit since the last Save was lost; a renamed track came back with its old name on the next
launch. Saving and loading were correct; nothing called them.

## Decision

The interface saves the project after every message that changes the session's state or named
versions. `Session` counts those changes in a revision; `App::update` saves when the revision
differs from the one last saved or tried. There is no File → Save. Quit is handled by the
app before the window exits, so a rename being typed is finished and saved.

## Alternatives considered

- **Save on quit** — misses a crash, a forced quit, and a window close that bypasses the Quit
  message; edits are lost exactly when the user cannot react.
- **Prompt about unsaved changes on quit** — keeps losing work on a crash, adds a dialog to every
  quit, and still needs the same change tracking.

## Consequences

- Every gesture writes `project.json`; edits are already one step per gesture (drop, release,
  finished rename, committed delay time), so drags and typing do not save per frame. A large project
  pays one write per step.
- A failed autosave is shown on the status line and not retried until the next change, so a
  persistent failure does not write every frame.
- There is no "discard changes since last save": undo and named versions serve that purpose.
