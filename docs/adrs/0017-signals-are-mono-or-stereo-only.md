# ADR-0017 — Signals are mono or stereo only

- **Status:** Superseded by ADR-0030
- **Decided by:** Chris Paro
- **Date:** 2026-11-06
- **Issue:** —
- **Architecture:** [Channel layouts](../architecture/signal-graph.md#channel-layouts)

## Context

The audience records bands and solo sessions for stereo release. The brief asks for mono and stereo
tracks and a mono or stereo mix.

## Decision

Every connection carries a mono or stereo signal. The project output is stereo; a mono mix is the
sum of its channels.

## Alternatives considered

- **Any channel count** — leaves room for surround, but every effect and meter must handle N
  channels.
- **Named surround layouts** — beyond the stated audience.

## Consequences

- Effects implement at most two channel cases, and panning and metering stay simple.
- Surround is foreclosed without a redesign of connections and effects.
- Multi-input interfaces are handled at the input: each recorded channel becomes a mono track, or a
  pair becomes a stereo track.
