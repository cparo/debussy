# ADR-0022 — The engine compensates recording and effect latency automatically

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-06
- **Issue:** —
- **Architecture:** [Recording alignment](../architecture/recording.md#recording-alignment)

## Context

`cpal` delivers input and output as separate streams ([ADR-0009](0009-audio-device-i-o-uses-cpal-behind-our-own.md)). Without compensation,
overdubs land late by the device round trip. Effects that add delay (lookahead limiters, for
example) would knock parallel paths out of phase in the graph ([ADR-0015](0015-routing-is-one-general-processing-graph-per.md)).

## Decision

Recorded audio is shifted by the reported input and output latencies. Every node reports its own
latency, and the engine delays parallel paths so they meet aligned.

## Alternatives considered

- **Recording alignment only** — parallel paths through delaying effects would comb-filter.
- **Manual offset** — puts a measurement task on amateur users and still ignores effect delay.

## Consequences

- Correct alignment depends on drivers reporting latency honestly. A loopback calibration tool may
  be needed for interfaces that do not.
- Compensation delays need preallocated buffers sized from the worst-case reported latency, and
  are re-planned off the real-time thread when the graph changes.
- Monitored inputs ([ADR-0021](0021-inputs-can-be-monitored-through-the-software-per.md)) are not delayed for compensation, so live monitoring stays as
  short as possible.
