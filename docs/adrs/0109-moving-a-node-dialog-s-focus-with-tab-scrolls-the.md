# ADR-0109 — Moving a node dialog's focus with Tab scrolls the focused control into view

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-13
- **Issue:** —
- **Architecture:** [Layout](../architecture/user-interface.md#layout)

## Context

Trying the dialogs' Tab cycle (ADR-0096, ADR-0108), the developer found that with the track pane
narrow enough for a parametric equaliser's band strips to scroll horizontally, Tab moved the
focus ring onto controls out of sight. They asked that the focused control be scrolled into view.

## Decision

When Tab or Shift-Tab moves a node dialog's keyboard focus, every scrollable holding the newly
focused control (the dialog body vertically, a mixer's or effect's strips horizontally) scrolls
by the least that shows the control whole; one already in view does not move. A press that
focuses a control does not scroll, since the control was under the pointer.

The focused control's focus ring carries a fixed widget id. The shell, after a Tab, runs two
`iced` widget operations: one finds that ring's bounds, the next scrolls each scrollable whose
content holds them.

## Alternatives considered

- **Compute the offset from the known layout** (fixed control and strip widths) and scroll a
  named scrollable to it. It needs the viewport's width, which `App` does not know, and breaks
  whenever the layout changes; reading the laid-out bounds does not.
- **One operation that scrolls as it finds the control.** `iced` hands an operation a
  scrollable's state before its children, so the target is not yet known when the state is in
  reach; hence two passes.
- **Snap the focused control to the start of the view.** Simpler, but the strips would jump on
  every Tab, even between controls already in view.

## Consequences

- The scrolling cannot be unit-tested through `App`, which has no layout; the offset rule and the
  operation's choice of scrollables are tested directly, and the result is checked by hand.
- Any focus ring, including a new kind of dialog control's, is revealed without further work.
