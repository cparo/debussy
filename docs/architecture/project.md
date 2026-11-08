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
keyed by name (see `docs/architecture/signal-graph.md` "Mixer"). The model checks its ports and
refuses to drop a connected channel ([ADR-0044](../adrs/0044-mixing-is-a-mixer-node-with-per-channel-gain-pan.md)); the interface removes only an unconnected last
channel ([ADR-0061](../adrs/0061-node-positions-and-the-outputs-height-are-saved-as.md)).

A node's position on the patch board and the height of the output's L are optional model state,
moved as one undo step per drag and kept in named versions; without them the interface lays the
board out automatically ([ADR-0061](../adrs/0061-node-positions-and-the-outputs-height-are-saved-as.md)).

A connection end at a stereo port may name its left or right side, carrying that channel as a
mono signal; the model refuses a side of a port it knows to be mono. In the running graph a sided
output is taken through a node that picks the channel, and a sided input is fed through a node
that places the signal on that side ([ADR-0058](../adrs/0058-stereo-ports-are-connected-by-side-and-new-tracks.md)).

A new project has one mixer feeding the output: the project mixer. A new track joins it in the
same undo step: a mono track on the lowest free channel, a stereo track's left and right on two
channels panned hard left and right, adding channels as needed; the channels' other values are
reset. A project without a project mixer connects new tracks straight to the output ([ADR-0058](../adrs/0058-stereo-ports-are-connected-by-side-and-new-tracks.md)).

The running graph follows the model through `engine_bridge`. After every change, including undo,
redo, version restore and load, the bridge builds the new routing into a scratch graph to apply
`signal_graph`'s rules. Only if that succeeds does it change the running graph, by the fewest calls
from what it holds, and commit. A change the graph refuses is reverted and forgotten, leaving the
model and the running graph as they were. An effect kind id this build does not know becomes a
silent stand-in with stereo ports enough for its saved connections ([ADR-0054](../adrs/0054-a-new-engine-bridge-crate-keeps-the-running-graph.md)).

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
`tracks` (top first, each with its clips bottom layer first), `nodes` and `connections`. Since
format version 2 a connection may carry `from_side` and `to_side` (`left` or `right`); version 1
files migrate unchanged ([ADR-0058](../adrs/0058-stereo-ports-are-connected-by-side-and-new-tracks.md)). Since version 3 a node may carry a
`position` (`x`, `y`) and the project an `output_y`, both patch-board coordinates within
±1,000,000; version 2 files migrate unchanged and use the automatic layout ([ADR-0061](../adrs/0061-node-positions-and-the-outputs-height-are-saved-as.md)). Times are
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

## Application settings

The application keeps per-machine settings, outside any project, in
`~/Library/Application Support/Debussy/settings.json`: a `format_version` (1) and the chosen audio
device's id. The file is written crash-safely and read as untrusted input; a missing, newer or
invalid file gives the defaults ([ADR-0059](../adrs/0059-the-window-imports-exports-and-picks-one-audio.md)).

## Stored audio

Each channel of recorded or imported audio is stored as its own mono WAV file of 32-bit float
samples at the project sample rate, using RF64 when a file exceeds 4 GB ([ADR-0014](../adrs/0014-project-audio-is-stored-as-mono-32-bit-float-wav.md)).
