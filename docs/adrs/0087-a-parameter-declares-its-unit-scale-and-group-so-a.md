# ADR-0087 — A parameter declares its unit, scale and group so a generic control can be drawn for it

- **Status:** Accepted
- **Decided by:** agent, ratified by Chris Paro
- **Date:** 2026-11-09
- **Issue:** #58
- **Architecture:** [Effect nodes](../architecture/signal-graph.md#effect-nodes)

## Context

The first effects (#58) have up to 40 parameters each, and more effects follow (#59–#62). Until
now a parameter declared only a name, a range and a default, so every node needed its own
hand-written dialog: the mixer's strips know that `channel N mode` is a three-way choice only
because the UI code says so. A parameter must also stay something a third-party plugin wrapper
could supply (ADR-0019).

## Decision

`ParameterInfo` carries, besides name, range and default:

- `unit`: text shown after the value, such as `dB` or `Hz`; empty for none.
- `scale`: `Linear`, `Logarithmic` (minimum above 0), `Integer`, `Choice(options)` (range 0 to
  options − 1, the value is the option's index) or `Toggle` (0 or 1). A stepped scale (integer,
  choice, toggle) rounds every value set to a whole number.
- `group`: the strip a control is drawn in, such as `band 1`; empty for none.
- `stopped_only`: whether the value may change only while playback is stopped or paused. The UI
  shows such a control without offering a change while playing, refuses the change, and no
  automation may drive it; the node may apply a change at once (first use: a filter's order,
  ADR-0088).

The node dialog draws a built-in effect's controls from these alone: a slider (over the logarithm
for a logarithmic scale, in whole steps for an integer), a pick list for a choice, an on/off
button for a toggle. The metadata is not saved: projects store values by parameter name only.

## Alternatives considered

- **A hand-written dialog per effect.** Simplest for one effect, but repeats layout code for every
  effect and for each band of an equaliser, and could not serve a hosted plugin.
- **Display hints kept in the UI crate, keyed by kind id.** Keeps `signal_graph` smaller, but
  splits one parameter's description over two crates that must agree, and a plugin's parameters
  would have no home.
- **CLAP's full parameter model (flags, value-to-text callbacks, module paths as `/` paths).**
  More than the built-in effects need; `unit`, `scale` and `group` map onto it if a plugin wrapper
  is added later.

## Consequences

- Every parameter constructor states its metadata; the mixer's mode is now a `Choice`, and its
  channels are grouped `channel N`.
- Value text is formatted generically (hertz from 1000 up as kilohertz); a parameter needing a
  custom display, such as note names, would need another scale or a formatter.
- A choice's options are labels only; their order is part of the saved format, since the index is
  what is stored.
