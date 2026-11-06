# Project Model

## Editing model

Editing is non-destructive. Audio files in a project are immutable once written. Tracks hold
clips, each referencing a source audio file with an offset into it, a length, and a position on
the timeline. Every edit changes clips only.

Audio files no longer referenced by the current state or by any named version are removed only by
an explicit cleanup action ([ADR-0011](../adrs/0011-editing-is-non-destructive-over-immutable-audio.md)).

## Undo history

Every edit is a reversible command in an in-memory history of unlimited depth. The history belongs
to the editing session and is discarded when the project closes; it is never persisted ([ADR-0012](../adrs/0012-undo-is-unlimited-per-session-and-named-versions.md)).

## Named versions

A user may save the current project state as a named version. Named versions persist in the
project bundle and reference the same audio files as the state they captured.

A named version can be restored either as the current state, which is itself an undoable edit,
or into a new project, which copies the audio files it references ([ADR-0012](../adrs/0012-undo-is-unlimited-per-session-and-named-versions.md)).

## Project bundle

A project is a directory named `<name>.debussy`, presented by macOS as a single document,
containing:

- `project.json` — the current state, with an integer `format_version` field;
- `versions/` — one JSON file per named version, in the same format as `project.json`;
- `audio/` — the project's audio files;
- `cache/` — derived data such as waveform summaries, which may be deleted at any time and
  regenerated.

A reader must refuse a `format_version` newer than it supports and must migrate older ones ([ADR-0013](../adrs/0013-a-project-is-a-bundle-directory-with-versioned.md)).

## Stored audio

Each channel of recorded or imported audio is stored as its own mono WAV file of 32-bit float
samples at the project sample rate, using RF64 when a file exceeds 4 GB ([ADR-0014](../adrs/0014-project-audio-is-stored-as-mono-32-bit-float-wav.md)).
