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

A track is audio (mono or stereo) or MIDI, fixed when it is created. A MIDI clip is a window onto
a shared, immutable note sequence: notes (start, length, channel, pitch, velocity) and control
changes (frame, channel, controller, value), timed in frames from the sequence's start. Its offset
and length window the sequence as an audio clip's window a file, so every clip operation and range
edit applies to it, and a split leaves both pieces sharing one sequence. A note plays if it starts
inside the window and is cut off at the window's end. Where MIDI clips overlap, the top clip plays
and a lower clip plays only where nothing above covers it, with no crossfade; a note plays only if
it starts in a stretch of its clip that shows, and is cut off where that stretch ends
([ADR-0145](../adrs/0145-midi-clips-play-through-a-per-track-real-time-note.md)). In a sequence, channels are 0–15, pitches,
controllers and values 0–127, velocities 1–127; every note is at least one frame long and ends
within the sequence; notes are sorted by (start, channel, pitch) and controls by frame; no two
notes on the same channel and pitch overlap. A MIDI clip has no gain and no fades, and a clip's
source must match its track's kind ([ADR-0144](../adrs/0144-midi-clips-are-windows-onto-shared-note-sequences.md)).

Quantizing a range moves each note of a MIDI clip on the chosen tracks that starts inside the
range a strength (0 to 1) of the way to the nearest grid line ("Musical time"), rounded to a
frame, keeping its length; a note is never moved before its clip's start or past its last frame.
Control changes do not move. A note then running past its sequence's end is cut there, and where
two notes on the same channel and pitch overlap, the earlier ends where the later starts, a note
left no frames long being dropped. Each changed clip gets a new sequence of its own, keeping its
position and layer; a sibling sharing the old sequence keeps it. The change is one undo step, and
one that moves nothing adds no step ([ADR-0152](../adrs/0152-midi-clips-are-edited-through-range-selection.md)).

## Musical time

A project has one tempo and meter for its whole length: `bpm` beats per minute (20 to 300, to
0.01), where a beat is a `beat_unit` note, and `beats_per_bar` beats in a bar (1 to 16), with
`beat_unit` 2, 4, 8 or 16. The default is 120 BPM in 4/4. Changing it is an undoable edit, refused
while playing, and named versions carry it. Frames stay the stored unit of time: tempo places a
grid over them and moves nothing. Beat `n` falls on frame `round(n × 60 × rate / bpm)`, computed
from `n` each time and never accumulated; bar 1 beat 1 is frame 0, and the grid extends before
frame 0 with negative beats. `project_model::tempo` holds the conversions
([ADR-0082](../adrs/0082-the-project-has-one-tempo-and-meter-saved-in.md)).

Quantize grid lines are bars, beats, eighth notes or sixteenth notes. Line `n` of a bar or beat
grid is bar or beat `n`; line `n` of a note-value grid falls on frame
`round(n × frames per whole note / 8 or 16)`, a whole note being `beat_unit` beats. Each line is
computed from its index, never accumulated; a frame's nearest line is the earlier of two equally
near ([ADR-0152](../adrs/0152-midi-clips-are-edited-through-range-selection.md)).

## Range edits

A range is a half-open span of timeline frames across chosen tracks. Cutting or deleting a range
removes what lies inside it on those tracks and splits clips at its edges. Nothing moves to close
the gap: a cut leaves silence.

The pieces of a split clip keep its layer. The left piece keeps the clip's id; when a clip
straddles the whole range, its right piece gets a new id and goes one layer above the left piece.
A new edge made by a split has no fade. A piece keeps the clip's fade-in or fade-out only at an
edge that was the clip's own, shortened to fit the piece.

Copying captures, for each chosen track in display order, the parts of its clips inside the range
as a lane: the track's layout and the clip parts, positioned relative to the range start, bottom
layer first. Pasting at a track and a position is an overlay: the excerpt's lanes go in order onto
that track and the tracks below it, and each pasted clip goes on the top layer of its track with a
new id. A paste is refused as a whole if its lanes run past the last track or if a lane's layout
differs from its track's.

Cutting, deleting and pasting are each one undo step, and one that changes nothing adds no step.
Copying is not an edit. An excerpt is held by the caller and never saved ([ADR-0073](../adrs/0073-range-edits-split-clips-without-new-fades-never.md)).

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
its tracks. Each audio track is a source with one output of the track's layout. Every project has one
stereo output, which cannot be removed. Other nodes are effects, instruments, mixers, buses and feedback delays joined
by connections ([ADR-0043](../adrs/0043-routing-is-undoable-project-state-tracks-are-bare.md)).

A MIDI track's output is one event port; it is not routed when the track is created, and it
connects only to an event input ([ADR-0144](../adrs/0144-midi-clips-are-windows-onto-shared-note-sequences.md)).

An instrument node is saved as a stable kind id with its non-default parameter values keyed by
name, as an effect is; an unknown kind id is kept and plays as silence. The model knows its ports:
event input 0, which only a MIDI track's output may reach, and stereo output 0. A side of an event
port is refused ([ADR-0147](../adrs/0147-instruments-are-their-own-node-kind.md)).

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
A connection may carry a wire shape: a bezier handle at each end of its wires, an offset from the
pin at that end (without one, that end's curve is automatic), and up to 256 points between, each a
board position with a handle each way. A change to the shape is one undo step per drag, add or delete
([ADR-0134](../adrs/0134-wire-handles-are-saved-as-optional-offsets-from-a.md)).

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

A project is a directory named `<name>.debussy`, containing:

- `project.json` — the current state, with an integer `format_version` field;
- `versions/` — one JSON file per named version, in the same format as `project.json`, named by
  the version's ISO 8601 basic UTC stamp with milliseconds (`20261107T153012.345Z.json`) and
  holding its name;
- `audio/` — the project's audio files;
- `cache/` — derived data such as waveform summaries, which may be deleted at any time and
  regenerated.
- `lock` — empty; a process holds an exclusive advisory lock on it while it has the project open
  ([ADR-0133](../adrs/0133-one-process-at-a-time-opens-a-project-enforced-by.md)).
- `recording.json` — present only while a pass is recorded, or after a crash cut one off; it
  names the pass's takes (see `docs/architecture/recording.md` "Take recovery").

A reader must refuse a `format_version` newer than it supports and must migrate older ones ([ADR-0013](../adrs/0013-a-project-is-a-bundle-directory-with-versioned.md)).
macOS presents the bundle as a single document of type `io.github.cparo.debussy.project`
([App bundle](platform.md#app-bundle), [ADR-0131](../adrs/0131-debussy-ships-as-a-debussy-app-bundle-that.md)).

## Projects folder

New projects are created in `~/Music/Debussy` by default, the folder created when missing. Open
lists the projects there: each directory named `*.debussy` directly inside it that holds a
`project.json` and is not open in any process (no process holds its `lock`), newest `project.json`
modification time first, at most 1000 ([ADR-0133](../adrs/0133-one-process-at-a-time-opens-a-project-enforced-by.md)). A project elsewhere is
opened through a folder dialog. The app keeps no list of recent projects. ([ADR-0098](../adrs/0098-projects-live-in-music-debussy-and-open-lists-them.md))

## Project archives

Share/Archive… writes a project as one zip file, `<name>.debussy.zip`, holding a single top-level
directory `<name>.debussy/` with `project.json`, `versions/*.json` and `audio/*.wav`; `cache/`,
`lock` and `recording.json` are left out. JSON entries are deflated and audio entries stored. The
project is saved first, and the file is written beside its destination and renamed into place.
Only `project_storage` depends on the `zip` crate ([ADR-0140](../adrs/0140-project-archives-are-read-and-written-with-the-zip.md)).

Opening an archive unpacks it into the projects folder under the first free name and then opens
it as any project. An archive is refused, leaving nothing behind, unless:

- every entry is that directory, or one of the files or subdirectories above inside it, named
  in UTF-8 with no absolute part, `..`, backslash or repeat;
- no entry is a symbolic link or encrypted, and each is stored or deflated;
- it has at most 100,000 entries, each JSON entry is at most 64 MB, and the bytes unpacked, counted
  as they are written and never taken from the headers, stay within the free space on the
  projects folder's volume less 1 GB.

Unpacking goes into a hidden directory in the projects folder, renamed into place once complete
and deleted on any failure; the result then passes the checks in [Loading](#loading)
([ADR-0141](../adrs/0141-a-project-archive-is-a-plain-zip-of-the-bundle.md)).

## Saved format

`project.json` and version files are JSON of the format types in `project_storage`, not of the
model's own types. Every field is required except a version's `name` and `saved_at`, which only
version files carry; unknown fields are refused. The file holds `format_version`, `sample_rate`,
`tracks` (top first, each with its clips bottom layer first), `nodes` and `connections`. Since
format version 2 a connection may carry `from_side` and `to_side` (`left` or `right`); version 1
files migrate unchanged ([ADR-0058](../adrs/0058-stereo-ports-are-connected-by-side-and-new-tracks.md)). Since version 3 a node may carry a
`position` (`x`, `y`) and the project an `output_y`, both patch-board coordinates within
±1,000,000; version 2 files migrate unchanged and use the automatic layout ([ADR-0061](../adrs/0061-node-positions-and-the-outputs-height-are-saved-as.md)). Since
version 4 the project carries a required `tempo` (`bpm`, `beats_per_bar`, `beat_unit`; see
[Musical time](#musical-time)); version 3 files migrate to 120 BPM in 4/4 ([ADR-0082](../adrs/0082-the-project-has-one-tempo-and-meter-saved-in.md)). Version 5
adds automation curves and curve ports (see `docs/architecture/automation.md` "Curves in the
project"). Since version 6 a connection may carry an `out_handle` and an `in_handle` (`dx`, `dy`) and
`points` (each `x`, `y`, `in_handle`, `out_handle`), at most 256, every value within ±1,000,000;
version 5 files migrate unchanged ([ADR-0134](../adrs/0134-wire-handles-are-saved-as-optional-offsets-from-a.md)). Since version 7 a track carries a `kind`
(`{"kind":"audio","layout":…}` or `{"kind":"midi"}`) in place of `layout`, the project carries
`sequences`, each `frames`, `notes` as `[start, length, channel, pitch, velocity]` and `controls` as
`[frame, channel, controller, value]`, and a clip source may be `{"kind":"midi","sequence":n}`, an
index into `sequences`; a sequence shared by several clips is saved once. Version 6 files migrate
to audio tracks and no sequences ([ADR-0144](../adrs/0144-midi-clips-are-windows-onto-shared-note-sequences.md)). Since version 8 a node kind may
be `{"kind":"instrument","instrument":…,"parameters":{…}}`; version 7 files migrate unchanged
([ADR-0147](../adrs/0147-instruments-are-their-own-node-kind.md)). Times are
frames at the project rate, gains dB, and kinds are tagged with a snake-case `kind` field ([ADR-0049](../adrs/0049-saved-projects-use-their-own-format-types-crash.md)).

## Saving

Every file is written to `<file>.tmp` in the same directory, synced, renamed over the old file,
and the directory synced. A `.tmp` file found on open is deleted. A save writes `project.json`,
writes only new or changed versions, and deletes the files of removed ones ([ADR-0049](../adrs/0049-saved-projects-use-their-own-format-types-crash.md)).

The interface saves the project after every change to the state or the named versions, including
undo and redo; it has no explicit save ([ADR-0067](../adrs/0067-the-project-is-saved-after-every-edit.md)).

## Loading

A project file is untrusted input. Load refuses a file over 64 MB, nesting deeper than 64 levels,
more than 10,000 tracks, 1,000,000 clips, 10,000 nodes, 100,000 connections or 10,000 versions,
a sample rate outside the four project rates, names over 1024 bytes, and audio file names that are
not plain names inside `audio/`. Every clip is checked against its track, ids must be unique, and
every connection must join existing endpoints, never from the output or into a track, with a pan
from −1 to 1. The tempo and meter must be within the limits in [Musical time](#musical-time). Curves have
their own limits (see `docs/architecture/automation.md` "Curves in the project"). Load refuses more
than 1,000,000 MIDI sequences or 2,000,000 notes and control changes over all sequences, counted
before any is built, a sequence that breaks the rules in [Editing model](#editing-model), a MIDI
source index out of range, and a sequence no clip refers to ([ADR-0144](../adrs/0144-midi-clips-are-windows-onto-shared-note-sequences.md)). Any failure refuses the whole bundle ([ADR-0049](../adrs/0049-saved-projects-use-their-own-format-types-crash.md)).

Opening or creating a bundle first takes its lock, creating a missing `lock` file. A bundle locked
by another process, or whose lock cannot be taken, is refused
([ADR-0133](../adrs/0133-one-process-at-a-time-opens-a-project-enforced-by.md)).

A recording marker, `recording.json`, is limited to 1 MB, 10,000 tracks, one or two plain `.wav`
file names inside `audio/` per track or, from version 2, one plain `.midilog` journal name, no file
named twice, and format version 1 or 2; version 2 adds `midi_latency` and journals. A marker
outside these limits is discarded, never refusing the bundle
([ADR-0135](../adrs/0135-takes-interrupted-by-a-crash-are-recovered-from-a.md),
[ADR-0151](../adrs/0151-midi-takes-are-journaled-to-a-fixed-record-log-so.md)).

## Cleanup

Audio files in `audio/` that neither the current state nor any version uses are deleted when a
project is opened and when it is closed, never while it is open, so undo can always reach its
audio. Only plain `.wav` file names are deleted ([ADR-0049](../adrs/0049-saved-projects-use-their-own-format-types-crash.md)),
and every plain `.midilog` MIDI take journal.
Takes recovered from a recording marker are kept until the interface places them
([ADR-0135](../adrs/0135-takes-interrupted-by-a-crash-are-recovered-from-a.md)).

## Application settings

The application keeps per-machine settings, outside any project, in
`~/Library/Application Support/Debussy/settings.json`: a `format_version` (2) and the chosen audio
device's id. The file is written crash-safely and read as untrusted input; a missing, newer or
invalid file gives the defaults ([ADR-0059](../adrs/0059-the-window-imports-exports-and-picks-one-audio.md)).
Since version 2 it also holds the click's `click_sound` (`block`, `click` or `stick`) and
`click_level_db` (−24 to +6); a version 1 file keeps its device and takes the Click sound at
0 dB, and a level out of range reads as 0 dB ([ADR-0083](../adrs/0083-the-click-is-mixed-at-the-output-stage-from.md)).
Since version 3 (the version written) it also holds the last export's settings, `export`:
`format` (`wav`, `flac`, `aac` or `mp3`), `wav_depth` and `flac_depth` (`int16`, `int24` or
`float32`), `aac_kbps`, `mp3_kbps` and `mono`. A version 1 or 2 file takes the default export,
24-bit stereo WAV; a FLAC depth of `float32`, or a bit rate of 0 or above 1000, reads as its
default ([ADR-0130](../adrs/0130-export-asks-for-the-format-and-its-settings-before.md)).

## Stored audio

Each channel of recorded or imported audio is stored as its own mono WAV file of 32-bit float
samples at the project sample rate, using RF64 when a file exceeds 4 GB ([ADR-0014](../adrs/0014-project-audio-is-stored-as-mono-32-bit-float-wav.md)).
