# ADR-0037 — Overlapping clips: the later covers with a short crossfade; clips have gain and fades; undo is per gesture; time is in frames

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-06
- **Issue:** #38
- **Architecture:** [Editing model](../architecture/project.md#editing-model)

## Context

The `project_model` crate (#38) holds tracks, clips and undoable edits in memory
([ADR-0011](0011-editing-is-non-destructive-over-immutable-audio.md),
[ADR-0012](0012-undo-is-unlimited-per-session-and-named-versions.md)). Overlaps, clip properties,
undo granularity and time units were open product questions, put to the developer.

## Decision

- **Overlaps:** when clips overlap on a track, the clip on top (the one most recently placed or
  moved) plays, and the covered part of the clip beneath is silent. A short automatic crossfade at
  each edge of an overlap prevents clicks.
- **Clip properties:** besides its source, offset, length and timeline position, a clip has a gain
  in dB and fade-in and fade-out lengths. There is no per-clip mute.
- **Undo:** one completed gesture (a whole drag, a whole fade adjustment) is one command.
  Selection, zoom and scrolling are not edits.
- **Time:** positions, offsets, lengths and fade lengths are integer frames (`u64`) at the
  project rate. Musical time, when it comes (click track, MIDI per
  [ADR-0020](0020-ports-and-track-contents-are-typed-so-midi-can-be.md)), is a conversion for
  display and snapping, not the stored unit.

## Alternatives considered

- **Overlaps as a hard cut, a mix, or refused** — a hard cut clicks; mixing surprises when comping
  takes and can clip; refusing makes comping fiddly.
- **A per-clip mute, or no clip properties** — mute is left to tracks; without gain and fades,
  takes cannot be level-matched and clip edges can click.
- **Undo for every intermediate step, or for selection too** — undoing a drag would take many
  steps, or selection would clutter the history.
- **Musical time as the stored unit** — audio positions would round, and tempo changes would move
  clips.

## Consequences

- The crossfade length at overlaps is a constant to choose when the playback path is built.
- The engine must render overlaps, crossfades, clip gain and clip fades when it plays clips.
