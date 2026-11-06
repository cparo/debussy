# ADR-0005 — The engine processes audio as 32-bit float throughout

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-06
- **Issue:** —
- **Architecture:** [Sample format](../architecture/engine.md#sample-format)

## Context

Every buffer, effect and mixer in the engine needs one sample representation. Core Audio, Audio
Units and the codecs reached through AudioToolbox all work natively in 32-bit float.

## Decision

The engine processes audio as `f32` throughout, including the mix bus.

## Alternatives considered

- **`f32` with an `f64` mix bus** — less rounding when summing many tracks, but at 32+ tracks of
  24-bit material `f32` summing error stays far below the noise floor, and the bus boundary would
  add conversions.
- **`f64` throughout** — twice the memory and bandwidth on the real-time path and in disk caches,
  plus conversions at every device and codec boundary, for precision that is inaudible.

## Consequences

- Sample data passes to and from Core Audio and AudioToolbox without conversion.
- Effects whose internal state needs more precision (for example, low-frequency IIR filters) may
  use `f64` internally; that is an implementation detail of the effect, not the engine format.
