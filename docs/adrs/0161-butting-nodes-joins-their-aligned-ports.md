# ADR-0161 — Butting nodes joins their aligned ports

- **Status:** Accepted
- **Decided by:** Chris Paro (snap, pull-apart, ⌥ chain drag, inert joints); agent, ratified by Chris Paro (eligibility, pinned neighbour, shape reset)
- **Date:** 2026-11-17
- **Issue:** —
- **Architecture:** [Patch Board](../architecture/patch-board.md#patch-board)

## Context

A linear chain of stereo effects took a pair of wires between each pair of nodes, which is
untidy for the most common routing. Chris Paro asked that two nodes pushed edge to edge, their
output and input pins lined up in the same order, count as wired at those pins. Inputs and
outputs sit on a node's left and right edges at the same pitch, so a node butted against
another's right edge, a whole number of slots up or down, puts each output pin on an input pin in
order.

## Decision

- A join is an ordinary connection between two pins that coincide on the board. There is no new
  model field and no format change; the board finds joints by laying out and comparing pins.
- A dragged node snaps flush to the nearest butted place within 8 pixels across and up or down,
  with its pin rows in step. It snaps only if at least one output and input pin meet there and
  every pair that meets is eligible: the two carry the same kind of signal, and each is wired to
  nothing but the other (a whole-port connection counting for both its sides). The preview shows
  the snapped place before release.
- Dropping there makes, as one undo step, the moves, a connection for each pair not yet wired
  (one whole-port connection when both sides of a stereo output meet both sides of a stereo
  input in order, otherwise one per pin), and resets the shape of joined connections already
  wired. An automatically placed neighbour it joins is pinned where it is drawn. If the signal
  graph refuses the result (a cycle without a feedback delay), nothing changes and the refusal is
  reported.
- Pulling butted nodes apart leaves their connections, drawn as ordinary wires. A plain drag
  moves one node; ⌥-drag moves every node reachable through joins.
- A joint is drawn as one pin, its output's circle with a seam across it, with no wire. It is not
  a hit target: a press on it catches the node under the pointer. Pins that meet but are not
  connected are ordinary pins.

## Alternatives considered

- **A separate "adjacency" relation in the model** — a second way to express a connection would
  need its own format, validation and graph translation, for something an ordinary connection
  already says.
- **Joining any pins that meet, rewiring what was there** — a drag would silently remove
  connections the user made elsewhere; refusing to snap keeps every snap additive.
- **Removing the connections when nodes are pulled apart** — a stray drag would break routing;
  keeping them means pulling apart is never destructive, and Delete still removes a wire.
- **Dragging a node always moves its chain** — then a node could not be lifted out of a chain
  without a modifier; the common single-node move stays the plain gesture.
- **Joint pins as hit targets for starting or picking up wires** — the pin under the pointer
  would be ambiguous between the two nodes' pins, and a joint's wire has no length to grab;
  falling through to the node makes the joint a place to grip the node.
- **Leaving an automatically placed neighbour unpinned** — automatic reflow would move it once
  its inputs changed and split the joint the user just made.
- **Keeping a joined connection's shape** — stale points would reappear as a detour once the
  nodes were pulled apart.

## Consequences

- Snapping looks at node rectangles and pin positions on the laid-out board, so its rule follows
  any later change to node geometry; a node whose pins are not on the slot pitch would never snap.
- A joint is only a drawing: moving one node of it without ⌥ turns the joint back into wires.
- Fan-out from a joined output is not allowed while joined; such an output must be wired to
  something else after pulling apart.
- The model still allows two connections into one input; one wire per input stays a board rule,
  so `place_nodes` relies on the board's eligibility check.
