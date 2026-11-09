# ADR-0073 — Range edits split clips without new fades, never ripple, and paste as an overlay

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-09
- **Issue:** #56
- **Architecture:** [Range edits](../architecture/project.md#range-edits)


## Context

The product brief asks for cut, copy and paste of selected ranges of one or more tracks (#56).
Clip moves and trims exist ([ADR-0037](0037-overlapping-clips-the-later-covers-with-a-short.md),
[ADR-0042](0042-tracks-have-a-fixed-layout-with-paired-mono-files.md)); a range operation also
has to split clips, which leaves three questions, put to the developer. What fades does a new
edge made by a split get? Does a cut close the gap it leaves? And how does a paste fit a
multi-track excerpt onto tracks that may hold clips already or differ in layout?

## Decision

- **Splits:** a range edit splits each clip at the range's edges. The pieces keep the clip's
  layer; the left piece keeps its id, and the right piece of a clip straddling the whole range
  gets a new id one layer above. A new edge made by a split has no fade, as a trim does. A piece
  keeps the clip's fade-in or fade-out only at an edge that was the clip's own, shortened to fit
  the piece.
- **No ripple:** a cut or delete leaves a silent gap; no clip moves.
- **Paste as an overlay:** the excerpt's lanes go in order onto the target track and the tracks
  below it. Pasted clips go on the top layer and get new ids. The whole paste is refused if a
  lane's layout differs from its track's, or if the lanes run past the last track.
- The excerpt is a plain value the caller holds; nothing persisted changes. Wiring into the
  interface is a follow-up.

## Alternatives considered

- **A fixed declick fade at each split edge** — changes the sound of audio that was not touched,
  and adds a constant the user did not choose. The automatic crossfade at overlapping edges
  ([ADR-0037](0037-overlapping-clips-the-later-covers-with-a-short.md)) already covers clicks
  where clips meet.
- **Keeping the exact fade shape across a split** — a fade cut in two would need a start and end
  gain on each piece, a change to the clip and the saved format for a rare case.
- **Ripple, always or as an option** — moves clips on other tracks out of sync with the ones
  edited, and an option doubles the cases to test before the interface needs it.
- **Skipping lanes whose layout does not match** — pastes part of what the user copied without
  saying so.
- **Insert-paste, moving later clips right** — a ripple by another name; rejected with it.

## Consequences

- A split leaves a hard edge, smoothed only where another clip overlaps it.
- A fade cut in two keeps only the part on its own piece, so a cut and paste back does not
  restore a fade that crossed a range edge.
- Without ripple, closing a gap is a separate move of the later clips.
- A paste of more lanes than there are tracks below the target is refused rather than adding
  tracks.
- Selecting ranges and the shortcuts for these operations are a follow-up interface issue.
