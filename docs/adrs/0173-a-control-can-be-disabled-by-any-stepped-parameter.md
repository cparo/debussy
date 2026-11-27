# ADR-0173 — A control can be disabled by any stepped parameter

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-26
- **Issue:** —
- **Architecture:** [Node dialogs](../architecture/node-dialogs.md#node-dialogs)

## Context

`osc N pulse width` only shapes the pulse wave, yet its control stays editable for saw and
triangle. ADR-0169 lets a control be disabled by another parameter's value, but only by a
stopped-only stepped one, and `osc N wave` is not stopped-only: it can change while playing.
Stepped parameters are never automated (`docs/architecture/automation.md`), so a stepped decider
changes only when the user edits it. Chris Paro decided on 2026-11-26 to relax the rule to any
stepped decider.

## Decision

- `ParameterInfo::enabled_while` names any stepped parameter (a choice, toggle or whole number)
  of the same node other than itself; it need not be stopped-only.
- `signal_graph::check_conditions` refuses a decider that is missing, itself or not stepped, or
  values outside its range.
- The synth's `osc N pulse width` is enabled only while `osc N wave` is pulse.
- Everything else in ADR-0169 stands.

## Alternatives considered

- **Make `osc N wave` stopped-only.** Keeps ADR-0169's rule, but loses switching waves while
  playing.
- **Leave pulse width enabled for every wave, documented.** No change, but a control that
  silently does nothing for two of three waves.
- **Allow any parameter, linear ones included.** A control could flicker as automation moves its
  decider during a take.

## Consequences

- A control may grey out or return during playback, but only when the user edits its decider.
- The UI already evaluates enablement from current values, so no UI change was needed.
