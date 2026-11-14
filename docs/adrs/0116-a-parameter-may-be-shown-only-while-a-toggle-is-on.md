# ADR-0116 — A parameter may be shown only while a toggle is on or off

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-13
- **Issue:** —
- **Architecture:** [Effect nodes](../architecture/signal-graph.md#effect-nodes)


## Context

The delay's `sync` toggle chooses between a free `time` and a synced `note` × `feel`; the
others have no effect while hidden by the choice, yet the dialog showed all four, which reads as
if all of them applied at once.

## Decision

A parameter may declare `shown_while: (toggle, on)`: its control is drawn only while the named
toggle parameter is on (`true`) or off (`false`). A hidden control is left out of the drawn
order, so Tab skips it, and arrow keys and presses do not change it. Its value is kept and saved
as before. The field is display metadata, like `state_labels`, `max_label` and `step`, and is not
saved. The delay's `time` is shown while `sync` is off, and `note` and `feel` while it is on.

## Alternatives considered

- **Grey the inactive controls out** — keeps the layout still, but still shows controls that do
  nothing; the developer asked for them hidden.
- **One "time" control whose kind switches with `sync`** — needs a control that is a slider or a
  pick list by state, and either merges two saved parameters or changes the persisted format.
- **Hard-code the rule in the dialog for `delay`** — dispatches on kind id in the UI, which the
  metadata-driven dialog avoids (ADR-0087).

## Consequences

- The dialog's layout changes as the toggle flips: a strip grows or shrinks, and the group
  strips stay equal height.
- Only a toggle can govern visibility; a condition on a choice or a range would need a wider
  field.
- A focused control that becomes hidden keeps the focus until Tab moves it, but keys do nothing
  to it.
