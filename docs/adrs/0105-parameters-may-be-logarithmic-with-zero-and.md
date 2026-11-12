# ADR-0105 — Parameters may be logarithmic with zero and enabled by a toggle

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-12
- **Issue:** —
- **Architecture:** [Effect nodes](../architecture/signal-graph.md#effect-nodes)

## Context

The dynamics controls rework (ADR-0104) needed two things the generic parameter metadata could
not express: a logarithmic slider whose far left is exactly 0 (attack and release, where 0 means
"no window" and small non-zero times need fine control), and controls that only mean something
while a toggle is on (the negative curve, while asymmetric). The dialog builds every control from
`ParameterInfo`, so both belong there rather than in effect-specific UI code.

## Decision

- **`Scale::LogarithmicOrZero`**: 0, or `min` (above 0) to `max` spaced by logarithm. A value
  below `min / 2` is held as 0, one from `min / 2` to `min` as `min`. Its slider runs over the
  logarithm from `min` to `max` plus a pad of 4% of that span below `min`: the lower half of the
  pad is 0, the upper half `min`. An arrow key steps like a logarithmic slider above `min`, and
  between 0 and `min` below it. The value shows as "0 ms" (its unit) at 0.
- **`ParameterInfo::enabled_by`** names a toggle of the same node. While that toggle is off the
  control's label and slider are greyed, it ignores the pointer, the arrow keys and Space, and it
  can still take the keyboard focus. It is display metadata, like `state_labels`: not saved, and
  it does not change the parameter's value or what the node does with it.

## Alternatives considered

- **A linear range from 0 for attack and release** — short times would get a few pixels of
  travel.
- **A logarithmic range from a tiny minimum such as 0.001 ms** — 0 would not be reachable, and
  the far left would spend travel on times shorter than a sample.
- **A separate toggle for 0** — two controls for one state.
- **Effect-specific greying in the dialog** — the dialog builds controls only from metadata;
  a dynamics special case would be the first.
- **Making disabled controls refuse the keyboard focus** — Tab order would change as the toggle
  flips.

## Consequences

- Any parameter can use the new scale or be enabled by a toggle; the node is unaffected by either
  and still reads the value as set (the dynamics node itself chooses to ignore its negative values
  while symmetric).
- A value saved between `min / 2` and `min` loads as `min`, one below `min / 2` as 0.
