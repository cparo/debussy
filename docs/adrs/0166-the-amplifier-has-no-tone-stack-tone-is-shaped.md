# ADR-0166 — The amplifier has no tone stack; tone is shaped with EQ nodes on the patch board

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-22
- **Issue:** #137
- **Architecture:** [Amplifier](../architecture/effects.md#amplifier)

## Context

The `amplifier` effect ([ADR-0102](0102-the-amplifier-is-a-memoryless-waveshaper-with-pre.md))
shipped without a tone stack, and #137 proposed adding one (bass, mid and treble controls before
the shaper, after it, or both). The patch board already lets a user place `parametric-eq` nodes
([ADR-0094](0094-the-parametric-eq-has-four-bands-framed-by.md)) before or after an `amplifier`,
and chain EQs with more than one amplifier stage.

## Decision

The `amplifier` has no tone stack and no tone controls. Tone before or after the shaper is shaped
by `parametric-eq` nodes wired around it on the patch board; multi-stage voicings chain EQ and
amplifier nodes.

## Alternatives considered

- **A circuit-style tone stack inside the amplifier** — duplicates what EQ nodes on the board
  already do, fixes the EQ to one place in the chain, needs a patent search for tone-stack
  modelling that `docs/research/amplifier-patents.md` did not cover, and adds parameters to the
  saved form of `amplifier` nodes.
- **Built-in equaliser bands from `dsp`'s filters** — the same sound as a `parametric-eq` node
  beside the amplifier, with fewer bands and less placement freedom, and the same saved-format
  change.

## Consequences

The amplifier's parameters and saved form stay as ADR-0102 set them. A classic amp voicing takes
more than one node on the board, and there is no single amp node with one-knob tone controls;
presets or saved node groups could supply that later if users ask. #137 is closed as won't do.
