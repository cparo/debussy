# ADR-0018 — Panning uses a -3 dB constant-power law

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-06
- **Issue:** —
- **Architecture:** [Panning](../architecture/signal-graph.md#panning)

## Context

Placing a mono signal in the stereo field needs a pan law. The choice changes mix balance, so it
must be fixed before mixes exist.

## Decision

Panning uses a -3 dB constant-power law (sine/cosine gains).

## Alternatives considered

- **User-selectable per project** — flexible, but adds a setting to the persisted format and a
  choice most of the audience never needs to make.
- **-6 dB linear** — sums to unity in mono, but perceived loudness dips in the centre.

## Consequences

- A centred mono source sums to +3 dB when the stereo mix is folded to mono. The mono mix derivation
  should take this into account.
- Changing the pan law later would alter existing mixes, so it would need a per-project setting
  that defaults to -3 dB.
