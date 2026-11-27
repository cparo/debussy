# ADR-0175 — A node's group strips are laid out in rows, aligned in columns, with an optional caption strip starting a row

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-26
- **Issue:** #210
- **Architecture:** [Node dialogs](../architecture/node-dialogs.md#node-dialogs)

## Context

ADR-0174 gave the synth a noise chain whose stages mirror the oscillator chain's: noise beside the
oscillators, a noise filter beside the filter, and so on. In one row of strips the synth's eleven
groups run off far to the right, and nothing shows which noise stage answers which tone stage.
Issue #210 asks for the noise stages to sit under their counterparts. Chris Paro chose a general
mechanism over a synth-only layout when planning the work.

## Decision

- `ParameterInfo` gains `row` (default 0, set with `in_row`) and `row_caption` (set with
  `with_row_caption`), display only and not saved. A group takes both from its first parameter.
- Tab order, which is also drawn order, lists the ungrouped parameters, then the groups sorted
  stably by row, so row-major, groups of one row in the order they first appear.
- A row whose first group's first parameter has a caption starts with a strip that shows only the
  caption and takes no focus.
- The strips form one grid inside the single horizontal scroll: column *k* is as wide as the
  widest *k*-th strip of any row, each row as tall as its tallest strip, each strip filling its
  cell with its content at the top left. With one row this is the previous layout.
- The synth's noise groups are in row 1, the first captioned "^ DCO Chain / Noise Chain >", so
  each noise stage sits under its oscillator-chain counterpart.

## Alternatives considered

- **A synth-specific layout in the UI.** Quicker for one node, but the UI would know a node's
  structure, which every other dialog draws from metadata alone.
- **A scroll per row.** Simpler layout, but the rows would scroll apart and lose their alignment.
- **Rows with their own column widths.** Simpler, but the noise stages would drift out from
  under their counterparts.
- **A row number on the group instead of each parameter.** Groups are only names today; giving
  them their own metadata would be a larger change for the same effect.

## Consequences

- `equal_heights` is replaced by `strip_grid`; every node without rows lays out as before.
- A node author keeps a group's row and caption on its first parameter; a later parameter's
  are ignored.
- The caption is plain text; its arrows are characters, not drawn shapes.
