# ADR-0134 — Wire handles and points are saved on a connection, in project format 6

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-14
- **Issue:** #82
- **Architecture:** [Saved format](../architecture/project.md#saved-format)

## Context

Wires on the patch board are cubic beziers whose control points ADR-0061 left automatic, level
with each end. On a crowded board wires cross nodes and each other, and people need to route
them by hand: bend a wire at its ends, and lead it through points around what is in the way.
ADR-0061 left wire geometry out of the format until it was designed. A routing that is lost on
reopening would have to be redone every session, so it is a saved-format change, which the
developer approved (issue #82).

## Decision

- A connection carries a wire shape: an optional handle at each end, `out_handle` and
  `in_handle`, each the control point's offset (`dx`, `dy`, logical pixels) from the pin at that
  end, `None` being the automatic control point; and up to 256 points between, from the output
  end, each a board position with a handle each way as an offset from it. The shape is model
  state: one undo step per drag, add or delete, kept in named versions, and restored with the
  connection when its removal is undone.
- Format version 6 saves them as optional `out_handle`, `in_handle` and `points` on a
  connection. Version 5 files migrate unchanged. Load refuses an offset or position that is not
  finite or is beyond ±1,000,000, as for node positions, and more than 256 points.
- A whole-port stereo connection's two wires share its shape. The second passes through the
  points moved by half the sum of how far its pins are from the first's, so the pair runs
  parallel.
- On the board, as on a curve lane: a selected wire shows both end handles on stems from their
  pins (at the automatic place when unset) and its points as squares; a selected point shows its
  handles. A press on a grip selects it and drags it; dragging a point's handle keeps the other
  pointing the opposite way at its own length, and ⌥ breaks that link. A press on the selected
  wire adds a point there, splitting the curve so it keeps its course, and selects it. Delete or
  Backspace removes the selected point, or returns the selected end handle to automatic, and
  leaves the wire selected.

## Alternatives considered

- **End handles as absolute board points.** Moving a node would leave its wires' handles behind
  and distort them.
- **One optional pair for both end handles.** Removing one would also reset the other.
- **A "Straighten" control, or undo only, to reset a wire.** The developer chose selectable,
  deletable handles, matching curve lanes.
- **Points without handles, smoothed automatically.** Fewer grips, but less control; the
  developer chose curve-lane handles.
- **A stereo pair converging at each point.** Simpler, but the two wires overlap there.
- **Points in a later format 7.** Format 6 was not yet merged, so one bump carries both.
- **Not saving the shape.** A routing lost on reopening would have to be redone every session.

## Consequences

- A project saved by this build cannot be opened by an earlier one (format version 6).
- Points are board positions, so moving a node reshapes only the segments next to its pins.
- A grip is kept on the board while dragged, and the board grows downwards to hold the shape, so
  it can always be caught again. A shape loaded from a file may still reach off the board's
  left or top edge.
- A press on a selected wire now adds a point rather than reselecting it.
