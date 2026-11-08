# ADR-0061 — Node positions and the outputs' height are saved as optional fields, and the patch board edits the routing

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-07
- **Issue:** —
- **Architecture:** [Saved format](../architecture/project.md#saved-format)

## Context

ADR-0060 laid the window out with a patch board beside the tracks, drawn read-only from an
automatic layout. Editing the routing there means people arrange nodes, and an arrangement that
is lost on reopening would have to be redone every session, so where the user puts nodes, and the
project output's L and R, must be kept with the project. That changes the saved format, which the
developer approved. The node dialogs also needed a way to take a channel off a mixer, which
ADR-0044 had left grow-only.

## Decision

- A node carries an optional board position (its top-left corner) and the state an optional height
  for the outputs' L. `None` means the automatic layout (columns by graph depth). Both are model
  state: moving a node or the outputs is one undo step when the drag ends, and named versions
  keep them.
- Format version 3 saves them as an optional `position` (`x`, `y`) on a node and an optional
  `output_y` on the project. Version 2 files migrate unchanged. Load refuses a coordinate that is
  not finite or is beyond ±1,000,000.
- On the board: drag from an output to an input to connect; every connection names its sides at
  stereo ports. An input takes one wire, and a drop on an occupied input, an output, or nothing
  does nothing. Click a node or wire to select it (a node's dialog opens over the track pane);
  Delete or Backspace removes the selection; a click on empty board clears it. The "+" adds a
  mixer (two channels), a stereo bus, or a stereo feedback delay of 0.25 s.
- A mixer's dialog can remove its last channel when nothing is connected to it.

## Alternatives considered

- **Positions in a separate interface file, outside the model** — no undo, no named versions,
  and a second file to keep in step with node ids.
- **Not saving positions; always lay out automatically** — moving a node would be pointless, and a
  board that rearranges itself after every edit is hard to follow.
- **Saving every pin's or wire's geometry** — wire routing (bezier handles) is not designed yet;
  it is left for its own issue rather than fixed into the format now.
- **Letting an input take several wires (summing)** — the model allows it, but one wire per input
  keeps the board readable; a bus does the summing.

## Consequences

- A project saved by this build cannot be opened by an earlier one (format version 3).
- Positions are layout hints in logical pixels; a different lane height or node size later moves
  wires relative to the gutter but loses nothing.
- Mono-only ports and wires in the model, automation curves on the board, and bezier handles for
  wires are left to follow-up issues.
