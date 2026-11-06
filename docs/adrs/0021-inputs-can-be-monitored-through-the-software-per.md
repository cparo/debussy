# ADR-0021 — Inputs can be monitored through the software per track

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-06
- **Issue:** —
- **Architecture:** [Input monitoring](../architecture/recording.md#input-monitoring)

## Context

Tracking musicians often want to hear their input with effects (an amp simulation, for example).
Many audio interfaces also offer direct hardware monitoring at zero latency.

## Decision

Each armed track has a monitor toggle, off by default. When on, its live input runs through the
graph to the outputs.

## Alternatives considered

- **Hardware monitoring only** — simpler, but players cannot hear effects while recording.

## Consequences

- Monitored latency is the device round trip at the current buffer size; the 128-frame default
  ([ADR-0007](0007-the-engine-targets-a-128-frame-default-buffer.md)) keeps it roughly 6–10 ms on typical interfaces, including driver safety offsets.
- Off by default avoids doubled sound for users monitoring through hardware.
- Input with monitoring on must not feed back through the speakers; the UI should warn when the
  output device is the built-in speaker.
