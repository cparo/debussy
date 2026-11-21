# ADR-0164 — The board's + menu no longer adds a bus

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-20
- **Issue:** —
- **Architecture:** [Patch Board](../architecture/patch-board.md#patch-board)

## Context

[ADR-0061](0061-node-positions-and-the-outputs-height-are-saved-as.md) gave the board's "+" a
stereo bus and kept one wire per input, saying "a bus does the summing". The board applies that
one-wire rule to a bus's inputs too (`can_connect` and `occupied` in
`crates/debussy_ui/src/patch_board.rs`), so a stereo bus takes at most two mono wires, one per side,
and a mono bus takes one. On the board a bus cannot sum, and a mixer already covers what else it
offers.

## Decision

The board's "+" menu no longer offers a bus. `NodeKind::Bus`, its saved form, the engine's bus
node and the bus dialog stay, so a project that already holds a bus opens, plays and can be edited
as before. The saved format does not change. This amends ADR-0061's "+" item; the rest of that
record stands.

## Alternatives considered

- **Let a bus input take several wires and sum them** — it would make the bus useful, but it breaks
  the one-wire-per-input rule the board's dragging, re-wiring and side handling are built on, for a
  job a mixer already does.
- **Remove the bus from the model, the engine and the saved format** — it would change a persisted
  format and leave older projects that contain a bus unable to open, for no gain over hiding it.

## Consequences

- New projects get mixers, not buses; a bus appears only in a project that already had one.
- The bus code paths stay and keep their tests, though only loaded projects reach them.
- If buses are wanted again, the summing question has to be settled first.
