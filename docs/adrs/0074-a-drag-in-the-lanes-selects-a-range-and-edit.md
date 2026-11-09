# ADR-0074 — A drag in the lanes selects a range, and edit shortcuts act on it

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-09
- **Issue:** #102
- **Architecture:** [Range selection and shortcuts](../architecture/user-interface.md#range-selection-and-shortcuts)


## Context

`project_model` can cut, copy, delete and paste ranges across tracks
([ADR-0073](0073-range-edits-split-clips-without-new-fades-never.md)), but the interface had no
way to choose a range, and its Cut, Copy and Paste buttons were disabled (#102). A press in a lane
already seeks, so a selection gesture had to live alongside it. Paste needed a target track and
position, and there were no keyboard shortcuts at all. The developer chose among the options
below.

## Decision

- A press in a lane still seeks and makes its track the current track. Dragging from it selects
  the frames between the press and the pointer on the tracks from the pressed one to the one under
  the pointer. Shift-press moves the nearer edge to the press; Escape or a plain press clears the selection.
- Paste goes at the playhead, its first lane on the top selected track, or the current track
  without a selection. The clipboard is in-app and holds one excerpt.
- Shortcuts: ⌘X/⌘C/⌘V, Delete and Backspace for the range, ⌘Z and ⇧⌘Z for undo and redo, and
  Escape. Delete goes to the patch board while it has a selection. ⌘ is Ctrl off macOS.
- Cut, delete and paste are refused while playing, as undo and redo are
  ([ADR-0056](0056-the-interface-owns-the-session-and-reaches-i-o.md)); copy is allowed.

## Alternatives considered

- **Shift-drag to select** — keeps a plain drag free, but hides the main way to select behind a
  modifier.
- **Track headers to choose tracks, a drag for the time** — two gestures for one selection.
- **Paste at the selection's start** — leaves paste unusable without a selection, and the
  playhead is already where the user points.
- **Edit shortcuts without undo and redo** — leaves the most used edit command button-only.
- **Range edits while playing** — the running feeds would have to be rebuilt mid-play; undo is
  already refused then.
- **The system clipboard** — an excerpt refers to project audio files and means nothing outside
  the project.

## Consequences

- A press always moves the playhead, so starting a selection also seeks to where it began.
- A drag shorter than a few lane columns is a click, not a selection.
- Clips cannot yet be selected or moved by dragging; that gesture will have to coexist with this
  one.
