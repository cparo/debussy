# Project Model

## Editing model

Editing is non-destructive. Audio files in a project are immutable once written. Tracks hold
clips, each referencing a source audio file with an offset into it, a length, and a position on
the timeline. Every edit changes clips only.

Audio files no longer referenced by the current state or by any named version are removed only by
an explicit cleanup action ([ADR-0011](../adrs/0011-editing-is-non-destructive-over-immutable-audio.md)).

Clip positions, offsets, lengths and fade lengths are integer frames at the project rate. A clip
also has a gain in dB and fade-in and fade-out lengths. Where clips overlap on a track, the clip
most recently placed or moved plays and covers the one beneath, with a short automatic crossfade
at each edge. Each completed gesture is one undoable command; selection, zoom and scrolling are not
edits ([ADR-0037](../adrs/0037-overlapping-clips-the-later-covers-with-a-short.md)).

A track is mono or stereo, fixed when it is created. A mono track's clips reference one stored
file; a stereo track's clips reference a left and a right mono file, read at one offset and
length. A track keeps its clips in layer order: a clip that is added, moved, or given a new
position goes to the top layer, and a trim, gain or fade change keeps its layer. A clip keeps its
id when it moves between tracks ([ADR-0042](../adrs/0042-tracks-have-a-fixed-layout-with-paired-mono-files.md)).

## Undo history

Every edit is a reversible command in an in-memory history of unlimited depth. The history belongs
to the editing session and is discarded when the project closes; it is never persisted ([ADR-0012](../adrs/0012-undo-is-unlimited-per-session-and-named-versions.md)).

Applying an edit returns the edit that reverses it exactly; a refused edit changes nothing. A
gesture of several changes is one batch edit. Saving a named version is not an edit. Ids are never
reused within a session. Randomized edit sequences in tests come from a seeded in-house generator
([ADR-0042](../adrs/0042-tracks-have-a-fixed-layout-with-paired-mono-files.md)).

## Named versions

A user may save the current project state as a named version. Named versions persist in the
project bundle and reference the same audio files as the state they captured.

A named version can be restored either as the current state, which is itself an undoable edit,
or into a new project, which copies the audio files it references ([ADR-0012](../adrs/0012-undo-is-unlimited-per-session-and-named-versions.md)).

## Routing

The project's routing is part of its editable state, in the same undo history and named versions as
its tracks. Each track is a source with one output of the track's layout. Every project has one
stereo output, which cannot be removed. Other nodes are effects, mixers, buses and feedback delays joined
by connections ([ADR-0043](../adrs/0043-routing-is-undoable-project-state-tracks-are-bare.md)).

A new track is connected to the output, panned centre, in the same undo step, and removing a track
removes its connections. An effect node is saved as a stable kind id with its non-default parameter
values keyed by name; an unknown kind id is kept and plays as silence. A parameter change is one
undo step per completed gesture. The model checks that a connection's endpoints exist, that the
output is never a source and a track never a target, and that no connection is duplicated; effect
ports and cycles are checked by `signal_graph` before an edit is recorded ([ADR-0043](../adrs/0043-routing-is-undoable-project-state-tracks-are-bare.md)).

A mixer is its own node kind, saved as its channel count with its non-default parameter values
keyed by name (see `docs/architecture/signal-graph.md` "Mixer"). Its channel count only grows; the
model checks its ports and refuses to drop a connected channel when an undo shrinks it ([ADR-0044](../adrs/0044-mixing-is-a-mixer-node-with-per-channel-gain-pan.md)).

## Project bundle

A project is a directory named `<name>.debussy`, presented by macOS as a single document,
containing:

- `project.json` — the current state, with an integer `format_version` field;
- `versions/` — one JSON file per named version, in the same format as `project.json`, named by
  the version's ISO 8601 basic UTC stamp with milliseconds (`20261107T153012.345Z.json`) and
  holding its name;
- `audio/` — the project's audio files;
- `cache/` — derived data such as waveform summaries, which may be deleted at any time and
  regenerated.

A reader must refuse a `format_version` newer than it supports and must migrate older ones ([ADR-0013](../adrs/0013-a-project-is-a-bundle-directory-with-versioned.md)).

## Saved format

`project.json` and version files are JSON of the format types in `project_storage`, not of the
model's own types. Every field is required except a version's `name` and `saved_at`, which only
version files carry; unknown fields are refused. The file holds `format_version`, `sample_rate`,
`tracks` (top first, each with its clips bottom layer first), `nodes` and `connections`. Times are
frames at the project rate, gains dB, and kinds are tagged with a snake-case `kind` field ([ADR-0049](../adrs/0049-saved-projects-use-their-own-format-types-crash.md)).

## Saving

Every file is written to `<file>.tmp` in the same directory, synced, renamed over the old file,
and the directory synced. A `.tmp` file found on open is deleted. A save writes `project.json`,
writes only new or changed versions, and deletes the files of removed ones ([ADR-0049](../adrs/0049-saved-projects-use-their-own-format-types-crash.md)).

## Loading

A project file is untrusted input. Load refuses a file over 64 MB, nesting deeper than 64 levels,
more than 10,000 tracks, 1,000,000 clips, 10,000 nodes, 100,000 connections or 10,000 versions,
a sample rate outside the four project rates, names over 1024 bytes, and audio file names that are
not plain names inside `audio/`. Every clip is checked against its track, ids must be unique, and
every connection must join existing endpoints, never from the output or into a track, with a pan
from −1 to 1. Any failure refuses the whole bundle ([ADR-0049](../adrs/0049-saved-projects-use-their-own-format-types-crash.md)).

## Cleanup

Audio files in `audio/` that neither the current state nor any version uses are deleted when a
project is opened and when it is closed, never while it is open, so undo can always reach its
audio. Only plain `.wav` file names are deleted ([ADR-0049](../adrs/0049-saved-projects-use-their-own-format-types-crash.md)).

## Stored audio

Each channel of recorded or imported audio is stored as its own mono WAV file of 32-bit float
samples at the project sample rate, using RF64 when a file exceeds 4 GB ([ADR-0014](../adrs/0014-project-audio-is-stored-as-mono-32-bit-float-wav.md)).
