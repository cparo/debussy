# ADR-0038 — Processing plans are swapped over queues, parameters are smoothed atomics, and only cycles run in sub-blocks

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-06
- **Issue:** #37
- **Architecture:** [Processing graph](../architecture/signal-graph.md#processing-graph)

## Context

The `signal_graph` crate (#37) turns the project's routing graph
([ADR-0015](0015-routing-is-one-general-processing-graph-per.md)) into something the real-time
thread can run. How a plan reaches that thread, how parameters change, how cycles are scheduled
([ADR-0016](0016-feedback-loops-must-pass-through-a-fixed-delay.md),
[ADR-0031](0031-feedback-delays-are-at-least-1-ms.md)), and how mono and stereo meet
([ADR-0018](0018-panning-uses-a-3-db-constant-power-law.md),
[ADR-0030](0030-signals-are-mono-or-stereo-and-the-mono-mix-is-the.md)) were put to the
developer.

## Decision

- **Plans:** a processing plan (the node order, preallocated buffers and compensation delays) is
  compiled off the real-time thread and sent to it over an `audio_buffers` queue. The real-time
  thread swaps plans at a block boundary and sends the old plan back over a second queue, to be
  dropped off the real-time thread. Node state carries over by moving nodes into the new plan.
- **Parameters:** each parameter is an atomic `f32` that the UI writes. A node reads it once per
  block and ramps linearly to the new value over about 10 ms. Control curves, when built, are
  evaluated in the engine from the plan rather than sent as messages.
- **Cycles:** the plan finds strongly connected components. Acyclic parts run once per block.
  Each cycle runs its nodes in topological order, with the feedback-delay outputs as sources, in
  sub-blocks no longer than that cycle's shortest delay.
- **Mono meets stereo:** each bus and node input has a fixed layout. A mono signal entering a
  stereo input is panned (centre by default, −3 dB per channel); a stereo signal entering a mono
  input is folded as `(L + R) × 0.7071`.

## Alternatives considered

- **Swapping plans through an atomic pointer** — slightly faster to take effect, but needs
  `unsafe` outside `audio_buffers`' queue module, and safe reclamation of the old plan.
- **Timestamped parameter messages** — sample-accurate and reusable for automation, but more
  machinery than the UI needs now.
- **Sub-blocking the whole graph when any cycle exists** — a simpler scheduler, but one short loop
  would slow every node.
- **Copying mono to both channels at unity** — mono sources would be 3 dB louder than when panned
  centre, so routing changes would alter levels.

## Consequences

- Plan compilation may allocate freely; executing and swapping plans must not, and is tested
  under the real-time guard.
- Parameter changes take effect at block granularity, smoothed. Sample-accurate automation needs
  control curves evaluated in the engine.
