# ADR-0134 — Wire handles are saved as optional offsets from a connection's ends, in project format 6

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-14
- **Issue:** #82
- **Architecture:** [Saved format](../architecture/project.md#saved-format)

## Context

Wires on the patch board are cubic beziers whose control points ADR-0061 left automatic, level
with each end. On a crowded board wires cross nodes and each other, and people need to route
them by hand. ADR-0061 left wire geometry out of the format until it was designed. A routing
that is lost on reopening would have to be redone every session, so it is a saved-format change,
which the developer approved (issue #82).

## Decision

- A connection carries an optional handle at each end, `out_handle` and `in_handle`: the
  control point's offset (`dx`, `dy`, logical pixels) from the pin at that end. `None` is the
  automatic control point. Both are model state: one undo step per drag, kept in named versions,
  and restored with the connection when its removal is undone.
- Format version 6 saves them as optional `out_handle` and `in_handle` objects on a connection.
  Version 5 files migrate unchanged. Load refuses an offset that is not finite or is beyond
  ±1,000,000, as for positions.
- A whole-port stereo connection's two wires share its handles, so they stay parallel.
- On the board: a selected wire shows both handles on stems from their pins, at the automatic
  place when unset. A press on one selects it and drags it. Delete or Backspace removes the
  selected handle, returning that end to the automatic curve, and leaves the wire selected, as
  Delete removes a selected point on a curve lane.

## Alternatives considered

- **Handles as absolute board points.** Moving a node would leave its wires' handles behind and
  distort them, and a stereo pair sharing the points would cross.
- **One optional pair for both handles.** Removing one handle would then also reset the other.
  Per-end handles let either end go back to automatic on its own.
- **A "Straighten" control, or undo only, to reset a wire.** The developer chose selectable,
  deletable handles, matching curve lanes.
- **Not saving handles.** A routing lost on reopening would have to be redone every session.

## Consequences

- A project saved by this build cannot be opened by an earlier one (format version 6).
- Offsets are in logical pixels, so a later change to node size or spacing moves the pins and
  carries the handles with them, rather than losing them.
- A handle is kept on the board while dragged, and the board grows downwards to hold it, so it
  can always be caught again. A handle loaded from a file may still lie off the board; deleting
  it brings it back.
