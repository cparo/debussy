# ADR-0125 — Curves drive parameters on a 32-frame timeline grid, read behind by each node's arrival time, overriding the set value

- **Status:** Accepted
- **Decided by:** agent, ratified by Chris Paro
- **Date:** 2026-11-14
- **Issue:** #63
- **Architecture:** [Evaluation](../architecture/automation.md#evaluation)

## Context

Issue #63's third slice makes automation curves (ADR-0123, ADR-0124) drive parameters in the
running graph. ADR-0038 already places control curves in the engine, evaluated from the plan
rather than sent as messages, and ADR-0020 typed ports so that new types could be added. The
developer chose three things: evaluation every 32 timeline frames with linear movement between,
output independent of the buffer size, and a connected curve that overrides its parameter.
Still open were how a node learns the curve's value, how it ramps to it, what time the curve is
read at, and what happens when the curve is disconnected.

## Decision

- `PortType::Control` joins `Audio`. Curve sources hold an immutable `Arc<Curve>`, replaced
  whole on a change. Curve-to-parameter connections are kept apart from audio connections, in
  the same id space, and name the parameter. No `Node` impl declares curve ports.
- A plan compiles each connection into a binding (curve, `Parameter` handle, node arrival
  time). While playing it splits blocks at timeline multiples of 32. Before each piece it sets
  each parameter to the curve's value at the next multiple, read at that frame minus the node's
  arrival time, to be reached at that multiple. While stopped it reads at the playhead once per
  block.
- The override lives in `Parameter`: an `automated` flag, the curve's value and the frames to
  the next control point, all atomics. `get` returns the curve's value and `set` changes only
  the base. A new plan releases the parameters the old plan bound that it does not, during
  carry-over on the real-time thread, and they ramp back to the base over 10 ms.
- `Smoothed` follows automation itself. `Ramp::follow` does the same for a value derived from
  one parameter. The mixer's channel gain uses `follow`; other effects' derived ramps keep the
  10 ms ramp for now.
- `Runner::set_transport(position, playing)` carries the timeline position, kept separate from
  `begin_block` so existing callers are unchanged. `audio_engine::render` gains its start
  frame.

## Alternatives considered

- **Sending curve values as parameter messages from the interface.** Rejected by ADR-0038: the
  timing depends on the interface thread, and offline render would not match playback.
- **Evaluating every frame.** Exact, but a bisection per frame per binding costs far more than
  one per 32 frames, and the plan's 10 ms parameter ramps already smooth finer detail.
- **Evaluating at block starts.** Rejected: the output would depend on the block size, which the
  developer ruled out.
- **Ramping to the current grid point's value** (reaching it 32 frames late). Rejected for
  targeting the next point: the latter lands on the curve at each control point, and a piece cut
  short by a block boundary sets the same target again, which leaves the ramp untouched.
- **Declaring control inputs on each `Node`.** Rejected: assignment is per project (ADR-0124),
  and every effect would change.
- **Converting every effect's derived ramps to `follow` now.** Deferred to keep the slice
  reviewable. Without it, an automated parameter on those effects lags its curve by up to 10 ms,
  which stays deterministic and independent of the block size.

## Consequences

- An automated parameter on an effect other than the mixer reaches the curve through a 10 ms
  ramp restarted at each control point. That is smooth and repeatable, but lags the curve.
  Converting those effects to `Ramp::follow` is follow-up work.
- A mute or solo change on an automated mixer channel ramps over the remaining control period
  (at most 32 frames) rather than 10 ms.
- A new binding moves its parameter from its base value to the curve within one control
  period.
- Within each control period, the value a sample uses is the interpolation towards the next
  point, one sample ahead of a centred interpolation.
- Control wires to an unknown effect's stand-in are kept in the model but not run.
