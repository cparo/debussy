# ADR-0169 — A parameter's control can be disabled by the value of a stopped-only parameter

- **Status:** Superseded by ADR-0173
- **Decided by:** Chris Paro
- **Date:** 2026-11-26
- **Issue:** #206
- **Architecture:** [Node dialogs](../architecture/node-dialogs.md#node-dialogs)

## Context

The synth's 6 dB low-pass order (ADR-0168) has no resonance, so the `resonance` control does
nothing there. Dialogs could only hide a control by a toggle's state (`shown_while`, ADR-0116);
hiding would shift a strip's layout, and it could not depend on a choice. Chris Paro decided on
2026-11-26 that a control may be disabled by another parameter's value when that parameter cannot
change during playback or recording, as a stopped-only one cannot.

## Decision

- `ParameterInfo::enabled_while` names a stopped-only stepped parameter of the same node and the
  values (option indices, or 0 and 1 for a toggle) for which this parameter's control is enabled.
  Display only: not saved.
- A disabled control is drawn greyed out (a slider in the driven style, a pick list or toggle as
  greyed text of its value); drags, presses and arrow keys leave it unchanged. It stays in Tab
  order, keeps its value, and still takes effect if the node reads it; automation of it is
  unaffected.
- `signal_graph::check_conditions` refuses a condition naming a parameter that is missing, itself,
  not stepped or not stopped-only, or values outside its range, and refuses a `shown_while` that
  does not name a toggle; every built-in effect and instrument is checked by a test.

## Alternatives considered

- **Extend `shown_while` to choices and hide `resonance` at 6 dB.** Reuses one field, but the strip's
  layout would shift with the order.
- **Leave `resonance` enabled with no effect, documented.** No new mechanism, but a control that
  silently does nothing.
- **Allow any parameter as the decider.** Controls could grey out and return during a take, as
  automation or a slider moves; restricting it to stopped-only parameters keeps the state fixed
  while playing.

## Consequences

- Nodes can now disable controls by mode choices that are stopped-only; a mode choice that must
  change while playing still has only `shown_while` on toggles.
- `iced` pick lists and buttons have no disabled state, so disabled ones are drawn as text.
