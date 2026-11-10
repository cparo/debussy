# ADR-0096 — Effect dialogs share the mixer's keyboard focus, and their group strips are as tall as the tallest

- **Status:** Accepted
- **Decided by:** agent, ratified by Chris Paro
- **Date:** 2026-11-10
- **Issue:** —
- **Architecture:** [Layout](../architecture/user-interface.md#layout)

## Context

An effect's dialog draws one strip per parameter group, each as tall as its own controls, so the
strips' bottoms were ragged: a parametric EQ band has five controls, its high- and low-pass
stages three. iced's `row` cannot stretch its children to the tallest: in a `Shrink`-height row,
a `Fill`-height child is laid out against a height of 0. And only mixer dialogs had a keyboard
focus (ADR-0089), so an effect's controls could not be reached from the keyboard at all.

## Decision

- An effect's group strips are laid out by a small in-house widget that lays each strip out at
  its own height, then again at the tallest's, so every strip's background reaches the same
  bottom; content stays at the top.
- Effect dialogs use the mixer dialog's keyboard focus model, generalised to one focus per open
  dialog (`DialogFocus`: a mixer control, or an effect parameter by index):
  - A press on a control focuses it; a press on the dialog's background clears the focus;
    selecting another node clears it.
  - Tab and Shift-Tab visit controls in the order they are drawn (ungrouped parameters, then each
    group's in first-appearance order), wrapping. The panel is drawn from that same order.
  - The up and down arrows step the focused control, each press one undo step, up increasing:
    a linear slider by 1 % of its range, a logarithmic one by 1 % of its logarithm's range, an
    integer by one, a choice to the next or previous option; all clamped to the range. Arrows
    leave a toggle alone. A stopped-only parameter refuses the step while playing, as a drag.
  - Space or Enter flips a focused toggle.
  - Sliders and toggles withhold keyboard and wheel events so the arrows reach the focused
    control, not the hovered one; pick lists keep theirs.

## Alternatives considered

- **`row` with `Fill`-height strips** — laid out against zero height in a shrinking row, so the
  strips collapse.
- **A fixed strip height** — wrong for every effect but the one it was measured for, and it
  breaks when fonts or controls change size.
- **A separate focus state for effect dialogs beside the mixer's** — two fields that must be
  kept mutually exclusive, and every focus message handled twice; one enum keeps one focus.
- **Snapping arrow steps to a 1 % grid, as the mixer's gain does** — an effect's ranges rarely
  divide into round numbers, so the grid would not land on values people expect; a plain step from
  the current value keeps a dragged value's precision.

## Consequences

- `Message::SetMixerFocus` became `Message::SetFocus(Option<DialogFocus>)`.
- The equal-height widget forwards events, drawing and overlays to each child as `row` does; its
  layout is checked visually, since testing it needs a renderer.
- A custom effect dialog added later gets keyboard focus for free if its controls come from
  `ParameterInfo`; anything else needs its own `DialogFocus` variant.
