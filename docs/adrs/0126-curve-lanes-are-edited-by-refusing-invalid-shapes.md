# ADR-0126 — Curve lanes are edited by refusing invalid shapes, with screen-linked handles, and a wired curve drives its parameter's control

- **Status:** Accepted
- **Decided by:** agent, ratified by Chris Paro
- **Date:** 2026-11-14
- **Issue:** #63
- **Architecture:** [Layout](../architecture/user-interface.md#layout)

## Context

Issue #63's fourth slice gives automation curves (ADR-0123, ADR-0124, ADR-0125) an interface:
curve lanes in the track pane, curve pins, ports and wires on the patch board, and a port button
beside each automatable control. A curve is valid only while every segment is a function of time,
so a gesture can easily ask for a shape the model refuses. Still open were how a lane is drawn,
what a gesture does with an invalid shape, how new handles are placed, how a point's two handles
stay smooth when the lane's axes are scaled differently, what a driven control shows, and where
port buttons sit in the dialog's keyboard focus.

## Decision

- A curve lane is a vector canvas, drawn every frame with 64 samples per Bézier segment, under the
  audio lanes' playhead and grid overlay; it is not rasterised.
- Point edits are pure functions over the points (`curve_edit`) that return `None` for a shape
  `Curve::validate` refuses. A drag computes each move from the shape at the press; a refused move
  is ignored, so the point stays at its last valid place. A drag is recorded as one step on
  release.
- A new point's handles are horizontal and reach a third of the gap to each neighbour. If the
  neighbours' facing handles would then overlap it, they are clamped to a third of their new gaps;
  removing a point clamps the joined pair the same way.
- Dragging a handle turns the opposite one to point the opposite way, keeping its length in screen
  pixels, so the pair looks collinear on screen whatever the zoom. Holding ⌥ breaks the link for
  that drag.
- A parameter's value as shown is, in order: its wired curve's value at the playhead, mapped into
  its range; the value being dragged; the model's; its default. A driven slider is greyed out and
  takes no drags or arrow keys.
- In a dialog's Tab order a control's port button comes just before the control: in an effect,
  before each automatable shown parameter; in a mixer strip, before gain and before pan.

## Alternatives considered

- **Rasterising curve lanes like audio lanes.** Rejected: a curve is a few strokes, cheap to draw
  as vectors, and a raster would need recomputing on every drag move.
- **Correcting an invalid shape** (pushing neighbours or shortening handles as the pointer moves).
  Rejected: the correction moves things the user did not touch; stopping at the last valid shape
  is predictable.
- **Linking handles in data space** (equal lengths in frames and value). Rejected: frames and
  value are scaled independently on screen, so equal data lengths look unequal and the pair kinks
  when zoomed.
- **Linking always, or never.** Rejected: smooth points are the common case, but a corner needs a
  way to break the link; ⌥ is the usual modifier for it.
- **Showing the stored value on a driven slider.** Rejected: the slider would contradict what the
  user hears.
- **Port buttons after all controls in Tab order.** Rejected: drawn order and Tab order would
  disagree.

## Consequences

- A gesture that would break the curve gives no message; the point simply stops.
- Each display frame samples every visible segment; very dense curves cost more to draw.
- A driven effect parameter still reaches its curve through a 10 ms ramp in the engine, so the
  sound lags the slider slightly; converting effects' derived ramps to `Ramp::follow` is #156.
