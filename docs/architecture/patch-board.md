# Patch Board

## Patch board

The patch board draws the routing; it pans horizontally with its own scrollbar when wider than
its pane, is never narrower than its pane, and does not zoom.

- The gutter is drawn in the board's colour and holds one port per lane, centred vertically on
  the lane with its left edge 4 pixels from the divider. A port's hover tip names its track, with
  " L" or " R" for one side of a stereo port
  ([ADR-0089](../adrs/0089-the-mixer-dialog-shows-narrow-numbered-strips-with.md)). Each curve
  lane has a lavender port at its centre, its hover tip the curve's name.
- Every wire is mono, in the audio colour, or lavender from a curve ([Layout](user-interface.md#layout)); a selected
  wire's tint is mixed from its own colour. A stereo port is drawn as two circles, left above right; a
  connection that names a side joins that circle, and a whole-port connection is drawn as a wire
  per side.
- The board is drawn at two-thirds of its base sizes: node ports are about 32 pixels apart and a
  node about 107 pixels wide. Outline strokes are never thinner than one pixel, and press
  tolerances keep their base sizes
  ([ADR-0159](../adrs/0159-the-patch-board-is-drawn-at-two-thirds-size-node.md)).
- Node inputs are on the left and outputs on the right; a node's first and last ports sit 8
  pixels in from its top and bottom edges, its height follows from its taller side, and its title
  is centred level with its first port. A mixer has a mono input per channel and a stereo output; a bus or
  feedback delay one port of its layout each way; an effect is drawn with one stereo port each way. A node's curve
  ports, one per assigned parameter, are spread along its bottom edge in port order, lavender
  with the port's number inside, and not counted in its height
  ([ADR-0126](../adrs/0126-curve-lanes-are-edited-by-refusing-invalid-shapes.md),
  [ADR-0159](../adrs/0159-the-patch-board-is-drawn-at-two-thirds-size-node.md)). A wire into a
  curve port, and the preview of one drawn from a curve, arrives from straight below unless its
  end handle is set.
- A node the user has moved stays where they put it. The rest are placed automatically in
  columns by graph depth (one past the deepest node feeding them), each column ordered by the
  height of what feeds it.
- The project output's L and R sit just inside the board's right edge, to the right of every node,
  at the height the user dragged them to, or else level with what feeds them.

Editing ([ADR-0061](../adrs/0061-node-positions-and-the-outputs-height-are-saved-as.md)):

- Drag from an output to an input to connect; at a stereo port the connection names its side. An
  input takes one wire; a drop on an occupied input, an output, empty board, or an input of the
  other signal kind (a curve on an audio input, audio on a curve port) does nothing.
- A press on an input that takes a wire picks up that wire's far end. Dropped on an input the
  connection may join, the connection moves there as one undo step, keeping its source and pan,
  its wires back on the automatic curve; a connection drawn as a pair of wires moves as a whole
  port, a single wire to the side it is dropped on. A drop anywhere else changes nothing
  ([ADR-0160](../adrs/0160-a-wire-s-input-end-is-dragged-to-another-input-to.md)).
- Drag a node, or the L and R outputs (vertically), to move them; the move is one undo step.
- A selected wire shows its two end handles as circles on stems from their pins, at the
  automatic place when unset, and its points as squares about 5 pixels across; a selected point shows its two
  handles. A whole-port stereo connection's second wire passes its points moved as far as its
  pins are from the first wire's. A press on a grip selects and drags it, kept on the board, as
  one undo step; dragging a point's handle keeps the other pointing the opposite way at its own
  length, and ⌥ breaks that link. A press on the selected wire adds a point there without
  changing its course, as one step, and selects it. Delete or Backspace removes the selected
  point, or returns the selected end handle to the automatic curve
  ([ADR-0134](../adrs/0134-wire-handles-are-saved-as-optional-offsets-from-a.md)).
- Click a node or wire to select it; a node's dialog opens. Delete or Backspace removes a
  selected wire, except while text is being typed; a node is deleted only through its dialog
  ([ADR-0108](../adrs/0108-a-node-dialog-s-delete-button-is-in-its-tab-cycle.md)). A click on empty board clears the selection and
  closes the dialog.
- The "+" at the board's top right adds a two-channel mixer, a stereo bus, a stereo feedback
  delay, or, below a separator, each built-in effect at its defaults, by the name shown in
  `docs/architecture/effects.md` "Effect kinds". A node's title is that name.

([ADR-0060](../adrs/0060-the-window-is-laid-out-as-top-bar-track-pane.md))
