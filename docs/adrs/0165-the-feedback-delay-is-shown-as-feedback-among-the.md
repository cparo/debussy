# ADR-0165 — The feedback delay is shown as Feedback, among the effects

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-21
- **Issue:** —
- **Architecture:** [Patch Board](../architecture/patch-board.md#patch-board)

## Context

The board's "+" listed the feedback delay as "Feedback delay" in its top group, beside Mixer and
MIDI Out ([ADR-0061](0061-node-positions-and-the-outputs-height-are-saved-as.md),
[ADR-0164](0164-the-board-s-menu-no-longer-adds-a-bus.md)), apart from the effects. The user
guide never explained it, so users did not know a loop needs one or where to find it. In use it
sits in the signal path like an effect: sound in, the same sound out later.

## Decision

The app shows the feedback-delay node as **Feedback**: its board title, its dialog header and its
"+" menu entry. The menu lists it in the effects group, sorted by name with the built-in effects
([ADR-0163](0163-menus-list-effect-and-instrument-kinds.md)); the top group holds Mixer and MIDI
Out. This amends the "+" menu part of ADR-0061 and ADR-0164. The node kind, its edit, its saved
form and the engine are unchanged, so the persisted format does not change.

## Alternatives considered

- **Rename the node kind and its saved name too** — a persisted-format change, with migration of
  saved projects, for no gain a user can see.
- **Keep it in the top group under the new name** — it is used like an effect, and users look for
  it among the effects.

## Consequences

Code and saved projects still say "feedback delay" while the app says "Feedback"; the glossary
records both. "Feedback" is also the name of a control on the delay and flanger effects, which the
context (a node title against a control label) keeps apart.
