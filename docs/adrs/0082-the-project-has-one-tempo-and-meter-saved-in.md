# ADR-0082 — The project has one tempo and meter, saved in project format 4

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-09
- **Issue:** #57
- **Architecture:** [Musical time](../architecture/project.md#musical-time)

## Context

Issue #57 asks for a click while recording, and with it the first musical time in Debussy. Until
now every time was a frame at the project rate (ADR-0037), and the project file had no notion of
tempo. A click, a count-in and a bars/beats grid all need one agreed beat grid, saved with the
project so it is the same on reopening, and the developer chose to keep it to one tempo and meter
for the whole project for now.

## Decision

- `project_model::State` holds one `Tempo { bpm, beats_per_bar, beat_unit }`, default 120 BPM in
  4/4. The limits are 20–300 BPM to 0.01, 1–16 beats per bar, and a beat unit of 2, 4, 8 or 16.
  The BPM counts the clicked beat, the `beat_unit` note, so 6/8 at 120 clicks eighth notes.
- Beat `n` falls on frame `round(n × frames_per_beat)`, `frames_per_beat = 60 × rate / bpm`,
  computed from `n` each time; bar 1 beat 1 is frame 0, and negative beats extend the grid before
  it. The engine's click uses the same expression, so the two agree to the frame.
- Frames stay the stored unit. Changing the tempo is `Edit::SetTempo`: undoable, refused while
  playing like every edit, and carried by named versions. It moves no clip.
- The saved format goes to version 4 with a required `tempo` object; a version 3 file migrates to
  the default. Load refuses a tempo outside the limits. The developer approved the format change
  through `flag-risk` (`product`).

## Alternatives considered

- **A tempo map with changes over time** — what a full DAW offers, but every conversion then
  needs a search and the edit model needs tempo events; nothing in #57 needs it yet.
- **An optional field without a version bump** — an older build would load the file and drop the
  tempo on its next save without warning.
- **Session-only tempo** — the grid would change under recorded takes on reopening.
- **Storing positions in beats** — would make every clip edit depend on the tempo, against
  ADR-0037's frame-exact model.
- **Accumulating beat lengths** — rounding errors would drift: about a frame per few thousand
  beats at odd tempos.

## Consequences

- Older builds refuse version 4 projects (ADR-0013).
- A tempo change does not move clips; material recorded to the old tempo no longer lines up with
  the grid.
- Tempo changes over time, and snapping to the grid (a follow-up issue), remain open.
