# ADR-0032 — Monitored input is not delayed for delay compensation

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-06
- **Issue:** #17
- **Architecture:** [Input monitoring](../architecture/recording.md#input-monitoring)

## Context

[ADR-0022](0022-the-engine-compensates-recording-and-effect.md) delays parallel paths
so they meet aligned, and its consequences said monitored live input
([ADR-0021](0021-inputs-can-be-monitored-through-the-software-per.md)) would not be delayed. That
was the agent's own assumption, not part of the developer's answer, so it was put to the developer
to confirm.

## Decision

Monitored live input is not delayed for delay compensation. Its latency is the device round trip
alone. Recording alignment and playback are compensated as before.

## Alternatives considered

- **Compensate monitored paths too** — keeps them phase-aligned with parallel paths, but adds the
  largest effect latency in the graph to what the player hears (5 ms or more with a lookahead
  limiter), which players notice while performing.

## Consequences

- While monitoring through an effect that adds latency, the monitored signal and compensated
  parallel paths can be out of phase. Only monitoring is affected; recorded takes and playback are
  not.
