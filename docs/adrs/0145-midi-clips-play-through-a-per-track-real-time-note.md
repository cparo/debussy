# ADR-0145 — MIDI clips play through a per-track real-time note player that treats any jump as a seek

- **Status:** Accepted
- **Decided by:** agent, ratified by Chris Paro
- **Date:** 2026-11-15
- **Issue:** #172
- **Architecture:** [MIDI clip playback](../architecture/engine.md#midi-clip-playback)

## Context

ADR-0143 gave the graph event ports and ADR-0144 put MIDI clips on MIDI tracks, each track with an
event source in the graph, but nothing filled those sources. Playback has to put each note on its
exact frame whatever the device block size, offline render has to send the same events as
playback, and stop, pause and seek must never leave a note hanging. The real-time thread may not
allocate, so the work of reading clips cannot happen there. The transport has no loop yet, but will.

## Decision

- Each MIDI track's clips are flattened onto the timeline off the real-time thread
  (`engine_bridge::midi_tracks`) into notes and control changes in timeline frames, and handed to
  the engine as one `MidiTrack` per track, which builds every index it needs (notes by start and
  by end, a running maximum of note ends, controls by frame and by controller) when it is made.
- Overlap follows audio: the top clip plays, and a lower clip plays only where nothing above
  covers it. A note plays if it starts inside a stretch of its clip that shows and is cut off at
  that stretch's end, extending ADR-0144's window rule to covered stretches.
- The player remembers the frame the next block should start at. Any block that starts elsewhere
  is a seek: it sends note-offs for every sounding note, moves its cursors by binary search, and
  chases at the block's first frame, sending each controller's last value before that point and
  then note-ons for the notes sounding there. Play, locate, the first block of a render and,
  later, a loop's wrap all take this one path.
- While not playing, the player sends note-offs for every sounding note, so stop and pause
  (which reset the plan before the block runs) silence MIDI in the same block.
- A note-off the event buffer refuses leaves its note marked as sounding and is sent at the next
  block. A refused note-on is not marked as sounding. Every refused push is counted, and the
  engine exposes the total.
- MIDI tracks are replaced only while stopped or paused; a replacement takes over which notes
  the old player on the same event source left sounding.

## Alternatives considered

- **Read the model's clips on the real-time thread** — needs the model there and per-block
  searches across clips; flattening once off the thread keeps the real-time work to cursor steps.
- **Mix all overlapping clips' notes** — differs from how audio clips overlap, and makes a
  covered take audible under the one placed over it.
- **A separate code path for loop and for locate** — two paths that must agree; treating any
  discontinuity as a seek gives loop for free once the transport has it.
- **No chase: play only notes starting after the playhead** — starting in the middle of a held
  note or after a controller change would sound wrong and differ from rendering from the top.
- **Drop a refused note-off** — the note would hang until the instrument is reset.

## Consequences

- Every MIDI edit rebuilds the track's player off the real-time thread; cheap for the sequence
  sizes expected, and edits already happen only while stopped or paused.
- A chase after a seek can send up to one event per controller and per sounding note at one
  frame; with ADR-0143's capacity, more than 512 at once spill into counted overflows, except
  note-offs, which carry over.
- Chasing covers control changes only; when pitch bend, pressure or program changes reach the
  model they need the same chase.
- Instruments that make MIDI audible come in #173; #175 and #176 build on this player.
