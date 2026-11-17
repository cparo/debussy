# ADR-0160 — A wire's input end is dragged to another input to move the connection

- **Status:** Accepted
- **Decided by:** Chris Paro (the gesture); agent, unratified (the drop and side rules)
- **Date:** 2026-11-17
- **Issue:** —
- **Architecture:** [Patch board](../architecture/patch-board.md#patch-board)

## Context

Moving a connection's far end meant deleting the wire and drawing a new one from its source,
which loses the connection's pan and is two undo steps. Chris Paro asked to be able to press the
pin where a wire meets an input and drag that end to another input. A first version kept a
connection's side only when it already named one, so a wire from one side of a stereo output,
moved to a mono mixer input and then back, returned as a whole-port connection that the other
side's wire blocked.

## Decision

- **Pick up an input end.** A press on an input that takes a wire picks up that wire's far end;
  the connection's wires hide while a preview runs from its source to the pointer.
- **Drop to move.** Dropped on an input the connection may join, the connection moves there as
  one undo step (`Session::reconnect`), keeping its id, source and pan, its wires returning to
  the automatic curve. Dropped anywhere else, or where it already ends, nothing changes.
- **Sides.** A connection drawn as a pair of wires moves as a whole port. A single wire takes
  the side of the pin it is dropped on.

## Alternatives considered

- **Delete and redraw.** Already possible; two steps, and the pan is lost.
- **Dropping on empty board disconnects.** Common in patching tools, but makes a slip
  destructive; Delete on a selected wire already removes it.
- **Keeping the connection's own side.** The first version; it cannot regain a side once the
  wire has visited a mono input, which caused the blocked move back.
- **Keeping the wire's shape.** Its points are absolute positions and suit the old route, not the
  new one.

## Consequences

- A refused drop is silent, as it is when drawing a new wire.
- Only the input end can be picked up; moving the source end is left open.
