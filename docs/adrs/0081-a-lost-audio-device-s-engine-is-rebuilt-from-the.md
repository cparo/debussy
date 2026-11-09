# ADR-0081 — A lost audio device's engine is rebuilt from the project

- **Status:** Accepted
- **Decided by:** agent, unratified
- **Date:** 2026-11-09
- **Issue:** #67
- **Architecture:** [Files and devices](../architecture/user-interface.md#files-and-devices)

## Context

ADR-0059 switches devices by asking the running callback to hand the engine's real-time side
(the `Processor`) back over a lock-free queue. When a device is unplugged its stream stops and
never calls back, so the handback never comes: every later switch waited 2 s and failed, whether
to the built-in output or to the same interface plugged back in. The processor is owned by the
dead stream's callback, and getting it out without the callback running would need a lock or
`unsafe` shared ownership on the real-time path (CLAUDE.md "Real-Time Audio Thread").

## Decision

- The device layer records that a stream has stopped with an error, or has not called back for
  500 ms, reports it once, and refuses to switch from it instead of waiting for a handback. In
  testing, unplugging an interface during playback stopped its stream with no error reported in
  time; the engine then took no commands, and resuming from pause reported it busy.
- The interface then builds a new bridge and engine from the project's current state, keeps the
  playhead, mono listening and the armed tracks, and hands the new processor to the device
  layer, which drops the dead stream (and the old processor with it, off the real-time thread)
  and opens a device: the one the user picked, or after an unplug the remembered device if
  present, else the default output.
- A recording pass in progress when the device goes keeps its start frame and plan latency in the
  interface; dropping the old processor ends its takes, which are written and placed as usual.

## Alternatives considered

- **Take the processor back from the dead stream** — `cpal` owns the callback closure, so this
  needs the processor behind a lock or an `unsafe` atomic pointer shared with the real-time
  thread; both are against the real-time rules.
- **Keep the processor outside the callback and lend it each buffer** — the same shared-ownership
  problem on every buffer.
- **Ask the user to restart** — loses no data but is a poor answer to unplugging a cable.

## Consequences

- A stream the system pauses for longer than 500 ms, such as across sleep, counts as lost, and
  the engine is rebuilt on waking.

- A live slider drag at the moment of loss is lost; its value was not yet recorded.
- The rebuilt graph starts from silence: effect tails and delay lines restart.
- The device is not reselected automatically when it returns; the user picks it in the dropdown
  (ADR-0080), which re-reads the device list on opening.
