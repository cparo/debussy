# ADR-0163 — Menus list effect and instrument kinds alphabetically by name

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-19
- **Issue:** —
- **Architecture:** [Effect kinds](../architecture/effects.md#effect-kinds)

## Context

The patch board's **+** menu and a MIDI track's ≡ menu listed effects and instruments in the order
their registry rows happened to be written (filters, then EQ, dynamics, amplifier, time effects).
That order follows no rule a user can learn, and each new kind would land wherever it was added.
The user guide had the same problem with its per-component headings.

## Decision

`audio_effects::EFFECTS` and `instruments::INSTRUMENTS` are kept sorted by name shown, ignoring
case, and every menu lists them in that order. A unit test in each registry enforces it. The user
guide orders sibling headings for like items the same way (the `user-docs` skill).

## Alternatives considered

- **Group by function** (filters, dynamics, distortion, time, modulation) — a learnable order in a
  big list, but with nine effects the categories are thinner than the list, and every new kind
  needs a category decided.
- **Sort at the menu, leave the registries unordered** — every menu (and the effect display's
  iteration) would need to sort, and one that forgot would disagree with the rest.
- **Keep the written order** — rejected as arbitrary.

## Consequences

A new kind goes in its alphabetical place; the test fails otherwise. Registry order carries no
meaning beyond the menus, and is not part of the saved format, so reordering changes no project.
If the list grows long enough to want categories, that is a new decision.
