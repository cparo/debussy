# ADR-0167 — High-pass and low-pass are stages of the Equalizer, not effects of their own

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-23
- **Issue:** —
- **Architecture:** [Equalizer](../architecture/effects.md#equalizer)

## Context

v0.1.0 shipped three filter effects: `high-pass`, `low-pass` and `parametric-eq`. Since
[ADR-0094](0094-the-parametric-eq-has-four-bands-framed-by.md), the equaliser has optional
high-pass and low-pass stages with the same Butterworth design, cutoff range and order as the
standalone filters, so they duplicate it. Removing a kind id changes the saved format: a v0.1.0
project may hold nodes of either kind.

## Decision

The effects menu has one filter effect, shown as Equalizer, with kind id `parametric-eq` (the id
is unchanged, so saved equalisers still load). `high-pass` and `low-pass` are no longer built-in
kinds. A saved node of either is an unknown kind: it plays as silence and keeps its saved values,
like any other unknown kind.

## Alternatives considered

- **Convert on load** — turn a `high-pass` or `low-pass` node into a `parametric-eq` node with
  that stage on and its cutoff and order copied, so it sounds the same. Rejected by the developer
  in favour of the simpler rule; the project has few users and one release.
- **Keep the kinds loadable but hidden** — keep both in the engine so old projects still play,
  and drop them from menus and docs. Rejected: it keeps code that nothing new uses.
- **Keep all three** — rejected: two of the three menu entries are a subset of the third.

## Consequences

- A v0.1.0 project with a standalone high-pass or low-pass loses that path's audio until the user
  replaces the node with an Equalizer and turns on the matching stage.
- The standalone `Butterworth` node is gone; the Butterworth cascade remains, behind the
  equaliser's stages and the amplifier's DC filter.
- A kind id is now retired. Reusing `high-pass` or `low-pass` for a different effect later would
  give old projects that effect with stale values, so neither id may be reused.
