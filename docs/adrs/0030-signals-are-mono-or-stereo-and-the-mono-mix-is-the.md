# ADR-0030 — Signals are mono or stereo, and the mono mix is the channel sum scaled by -3 dB

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-06
- **Issue:** #15
- **Architecture:** [Channel layouts](../architecture/signal-graph.md#channel-layouts)

## Context

[ADR-0017](0017-signals-are-mono-or-stereo-only.md) made every signal mono or stereo and derived
the mono mix by summing the stereo channels. Panning then adopted a -3 dB constant-power law
([ADR-0018](0018-panning-uses-a-3-db-constant-power-law.md)), which puts a centred mono source at
about 0.707 in each channel, so a plain sum raises centred sources by 3 dB in mono. This record
restates ADR-0017 with the fold-down changed, and supersedes it.

## Decision

- Every connection carries a mono or a stereo signal. The project output is stereo.
- The mono mix is `(L + R) × 0.7071` (√½, -3 dB): a centred source folds to unity, and a
  hard-panned one to -3 dB.

## Alternatives considered

- **Plain sum (ADR-0017)** — hard-panned sources stay at unity, but centred sources, most of a
  typical mix, gain 3 dB, so mono exports run hotter and clip more often.
- **Sum × 0.5** — can never exceed the stereo peak, but mono mixes come out 3–6 dB quieter than
  stereo.
- **Any channel count, or named surround layouts** (from ADR-0017) — still rejected: every effect
  and meter would have to handle N channels, and surround is beyond the stated audience.

## Consequences

- Mono and stereo mixes have about the same loudness for centred material.
- Wide, in-phase content near full scale can exceed 0 dBFS in mono. That is harmless in float,
  but an integer mono export can clip, so export metering should show the mono peak.
- ADR-0017's other consequences stand: effects handle at most two channel cases, and a multi-input
  interface's channels become mono tracks or stereo pairs.
