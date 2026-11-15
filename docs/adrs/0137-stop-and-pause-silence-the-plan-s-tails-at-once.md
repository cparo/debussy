# ADR-0137 — Stop and pause silence the plan's tails at once

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-14
- **Issue:** —
- **Architecture:** [Transport](../architecture/engine.md#transport)

## Context

The plan runs every block, whether or not the transport is playing, so that armed tracks can be
monitored while stopped ([ADR-0078](0078-recording-captures-armed-inputs-on-the-real-time.md)).
Stop and pause only ended clip playback: whatever effects and compensation delays held kept
sounding, so a reverb or delay rang out, and a lookahead or compensation delay played its last
few milliseconds, after the transport had stopped. With the reverb, delay and lookahead nodes now
in use this was clearly audible. Stop and pause are expected to silence the output at once.

## Decision

- A node can be reset: it clears what it holds of past input (delay lines, filter memory, envelope
  levels) and keeps its parameters. Reset runs on the real-time thread, under the same rules as
  processing; a node that holds no past input need not implement it.
- Resetting a plan resets every node and clears every feedback delay and compensation delay.
- The engine resets the plan when it takes a stop or pause, so the output from the next block on
  carries only what the sources give then: silence, or monitored input.

## Alternatives considered

- **Mute the output after stop until the tails decay** — would also mute monitored input while
  stopped, and the tails would come back on the next play.
- **Stop running the plan while stopped** — breaks monitoring while stopped, and leaves the state
  in the nodes to resume on play.
- **Fade the output over a few milliseconds before the reset** — softer, but the user asked for an
  immediate stop, and a fade needs the plan to run on with its tails during the fade. Can be added
  later if the hard cut clicks audibly.
- **Rebuild the plan from the control side on stop** — allocates, and the new plan reaches the
  real-time side a block or more after the stop.

## Consequences

- Stop and pause cut the output at a block boundary. A loud signal may click as it is cut.
- Playing again starts every effect from silence, as after loading.
- Every node that holds past input must implement reset; one that does not keeps ringing after a
  stop. `audio_effects` tests every built-in effect, at its defaults and at each preset.
- Monitored input still passes after a stop, through effects starting from silence.
