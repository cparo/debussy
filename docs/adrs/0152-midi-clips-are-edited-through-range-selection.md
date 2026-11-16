# ADR-0152 — MIDI clips are edited through range selection: quantize by grid and strength, a note miniature, and a per-track instrument menu

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-16
- **Issue:** #177
- **Architecture:** [Track Pane](../architecture/track-pane.md#track-pane)

## Context

After #175, MIDI tracks recorded clips that played back but could not be seen, could not be
tidied in time, and stayed silent until the user wired an instrument by hand on the patch board.
Issue #177 asked for clip operations, quantize, a choice of instrument, and a read-only view of
the notes. The model already applied every clip operation and range edit to MIDI clips
(ADR-0144), but the interface had no clip move or trim for either kind of clip, and the
Synthesizer was the only instrument.

## Decision

- **Range edits only.** MIDI clips are edited through the range selection, as audio clips are:
  cut, copy, paste, delete, and now quantize. Clip select, move and trim, for both kinds, are left
  to #190.
- **Quantize acts on the range selection.** A dialog picks a grid (bar, beat, 1/8, 1/16) and a
  strength from 0 to 100 %. Each note that starts in the range moves that fraction of the way to
  the nearest grid line, keeping its length and staying inside its clip. Overlaps on a key are
  resolved by ending the earlier note where the later starts. Each changed clip gets its own new
  sequence, at its own position and layer, and the whole change is one undo step.
- **A note miniature.** A MIDI lane draws its clips as faint bands and the notes the engine plays
  as thin bars, scaled to the pitch range in view (at least an octave). It reuses the engine's
  flattening of a track's clips, so what is drawn is what plays.
- **A per-track instrument menu.** A MIDI track's ≡ menu lists the built-in instruments. Choosing
  one adds that instrument, wires the track into it and its output into the project mixer, and
  removes the instrument the track fed before unless another source feeds it, as one undo step.

## Alternatives considered

- **Clip move and trim in this change** — a press in a lane already means seek and a drag means
  range select, so clip gestures need a design of their own, and they apply to audio clips
  equally. Doing them for MIDI alone would split the two kinds' editing models; they went to #190.
- **Quantize whole clips** (select a clip, quantize all its notes) — needs clip selection, which
  does not exist yet (#190). The range form also quantizes part of a clip, and a range over a
  whole clip quantizes all of it.
- **Quantize in place on the shared sequence** — pieces of a split clip share one sequence, so
  quantizing one piece would move notes in its siblings. Each clip gets a new sequence instead.
- **Wire an instrument automatically when a MIDI track is created** — picks an instrument the user
  did not ask for, and adds nodes and mixer channels that must then be found and deleted for a
  track meant for an external port (#176) or a sampler (#174). The menu makes it one deliberate
  step that also works for changing instrument later.
- **Draw notes from the clip's sequence directly** — would show notes hidden under a higher clip
  or past a window's end. Reusing the engine's flattening keeps the two in step.

## Consequences

- A new MIDI track is silent until an instrument is chosen or wired; its header says "No
  instrument" until then.
- Quantize moves note starts only. Note-end quantize, swing and groove templates are not offered.
- The instrument menu lists only the built-in registry; when MIDI out (#176) and the sampler
  (#174) arrive, they join the same menu.
- Quantizing a split piece makes its sequence a separate copy, so the saved project holds one
  more sequence per quantized piece.
- Clip select, move and trim remain open in #190.
