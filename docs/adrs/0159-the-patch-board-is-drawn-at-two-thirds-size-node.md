# ADR-0159 — The patch board is drawn at two-thirds size, node titles level with the first port and curve ports along the bottom edge

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-17
- **Issue:** —
- **Architecture:** [Patch board](../architecture/patch-board.md#patch-board)

## Context

The patch board fills up fast: with ports 48 pixels apart, a node 160 pixels wide and a curve
port adding a row to its node's height, a modest project runs off the pane. Chris Paro tried the
board at half size and then at other scales, settling by eye on two-thirds. At that size a
title above the first port wastes a row, and curve ports below the audio inputs make a node with
several assigned parameters much taller than its audio ports need.

## Decision

- **Two-thirds size.** One scale, 0.67, multiplies every drawn size on the board: port spacing,
  node width, margins and gaps, pin and output radii, wire width, handles, wire points and text.
  Stroke widths used for outlines and the selected wire's extra width never fall below one
  pixel. Hit tolerances keep their full size.
- **Titles level with the first port.** A node's first and last ports sit 8 pixels in from its
  top and bottom edges (to the circle's edge), and its title is centred level with the first
  port, between the input and output columns.
- **Curve ports along the bottom edge.** A node's curve ports are spread evenly along its bottom
  edge in port order, and do not count towards its height. A wire into one, unless its end
  handle has been set, arrives from straight below; so does the preview of a wire drawn from a
  curve.

## Alternatives considered

- **Half size.** Tried first; Chris Paro then tried other scales by eye and chose two-thirds.
- **Curve ports below the audio inputs.** Kept them in one column with the inputs but made nodes
  tall, and mixed control inputs in among audio ones.

## Consequences

- Saved node and output positions are absolute pixels, so a project laid out at the old size
  looks spread out until its nodes are moved; the saved format does not change.
- A node with many curve ports crowds its bottom edge; more than about three overlap the stems
  of wires arriving from below. Widening such nodes is left open.
- Stacked nodes are closer together, so wires arriving from below can run near the node above.
