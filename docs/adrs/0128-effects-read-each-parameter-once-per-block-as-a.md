# ADR-0128 — Effects read each parameter once per block as a Reading, and their derived ramps approach it by the next control point

- **Status:** Accepted
- **Decided by:** agent, ratified by Chris Paro
- **Date:** 2026-11-14
- **Issue:** #156
- **Architecture:** [Evaluation](../architecture/automation.md#evaluation)

## Context

ADR-0125 left every effect but the mixer ramping its derived values over 10 ms from each control
point, so an automated parameter lagged its curve by up to 10 ms (#156). `Ramp::follow` already
reaches a target by the next control point, but takes the `Parameter` itself, while each
effect's `set` takes plain values through a closure, which its unit tests fake. Reading the value
and the control timing in two calls could also pair a curve's value with an unautomated ramp if
the binding changed between them.

## Decision

- `Parameter::read` gives a `Reading`: the value and, while automated, the frames to the next
  control point, loaded together. `Reading::map` derives a value that keeps the timing, and
  `Ramp::approach` sets a target from a reading. `Ramp::follow` is `approach` over the
  parameter's control ramp.
- Each effect's per-block `set` takes readings instead of values. Every ramp of a continuous
  value derived from one parameter approaches its reading: dynamics, amplifier, delay, reverb,
  chorus, flanger, the modulated delays' spread and mix, the filters' cutoff and the parametric
  equaliser's band frequency, gain and Q.
- Ramps derived from toggles, choices or integers, and internal fades (shape, morph, voice
  gains), keep `Ramp::set_target`: no curve can drive their parameters.

## Alternatives considered

- **Passing `&Parameter` to each `set`.** Rejected: unit tests that call `set` with made-up values
  would need real parameters, and only `signal_graph` can automate one.
- **A second closure for the control timing.** Rejected: two reads per parameter, which can
  disagree if the binding changes between them.
- **Converting the effects to `Smoothed`.** Rejected: the derived values (log frequencies,
  slopes, frames) are not the parameters themselves.

## Consequences

- An automated parameter on any built-in effect lands on its curve at each control point, as
  the mixer's gain does; ADR-0125's 10 ms lag is gone.
- A new effect must read through `Parameter::read` and approach its ramps for automation to be
  exact; one that calls `set_target` on an automatable value lags by 10 ms.
