# ADR-0144 — MIDI clips are windows onto shared note sequences; format 7

- **Status:** Accepted
- **Decided by:** agent, ratified by Chris Paro
- **Date:** 2026-11-15
- **Issue:** #170
- **Architecture:** [Editing model](../architecture/project.md#editing-model)

## Context

ADR-0139 put MIDI performance capture in scope, voiced by a built-in synth, a sampler or hardware,
and ADR-0143 added event ports to the graph. Playback (#172), recording (#175) and the clip UI
(#177) all need somewhere to keep notes: a MIDI track kind holding clips of notes and controller
changes, positioned in frames, that support the same move, trim, split, cut, copy, paste and
per-gesture undo as audio clips. The project format had no way to store them, and a project file is
untrusted input (`docs/architecture/project.md` "Loading"), so the new data needs its own limits and
checks.

## Decision

- A track has a kind: audio (with its channel layout) or MIDI. A MIDI track's output is one event
  port.
- A MIDI clip is the same `Clip` as an audio clip, with a new source, `Source::Midi`, holding an
  `Arc`-shared `Sequence`: notes (start, length, channel, pitch, velocity) and control changes
  (frame, channel, controller, value), timed in frames from the sequence's start. The clip's
  offset and length window the sequence, so every existing clip operation applies unchanged and
  splitting a clip shares one sequence.
- A sequence is valid by construction: values in MIDI 1.0 range, velocity at least 1, notes at
  least one frame long and inside the sequence, notes sorted by (start, channel, pitch) and
  controls by frame, and no two notes on the same channel and pitch overlapping.
- A note plays if it starts inside the clip's window and is cut off at the window's end.
- A MIDI clip has no gain and no fades; a clip's source must match its track's kind.
- The saved format moves to version 7: a track's `layout` becomes a tagged `kind`, the project
  gains a deduplicated `sequences` table whose notes and controls are fixed-size arrays, and a MIDI
  source refers to a sequence by index. Version 6 files migrate with an empty table.
- Loading caps sequences at 1,000,000 and notes plus controls over all sequences at 2,000,000,
  counted before any is built, and refuses an index out of range or a sequence no clip refers to.

## Alternatives considered

- **A separate `MidiClip` type** — would make the kinds distinct in the type system, but every
  clip operation, range edit and undo step would need a second copy.
- **Storing notes inline in each saved clip** — simpler to read, but a split clip would save its
  notes twice and reload as two unshared sequences, breaking the sharing the model relies on.
- **Optional MIDI fields with no version bump** — old builds would silently drop MIDI tracks
  instead of refusing a file they cannot represent.
- **Unbounded or per-sequence limits only** — a hostile file could still ask for an unbounded total
  allocation; one global count bounds memory before anything is built.

## Consequences

- Version 7 files do not open in 0.1.0, which refuses newer formats.
- `project_model` depends on the in-workspace `midi` crate; no third-party dependency changes.
- A MIDI track cannot connect to anything until instruments with event inputs arrive (#173), and
  it plays nothing until #172.
- Per-clip velocity scaling or MIDI fades would need a new decision.
