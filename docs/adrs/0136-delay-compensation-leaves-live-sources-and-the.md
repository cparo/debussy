# ADR-0136 — Delay compensation leaves live sources and the nodes fed only by them undelayed

- **Status:** Accepted
- **Decided by:** agent, ratified by Chris Paro
- **Date:** 2026-11-14
- **Issue:** #109
- **Architecture:** [Delay compensation](../architecture/signal-graph.md#delay-compensation)

## Context

[ADR-0032](0032-monitored-input-is-not-delayed-for-delay.md) says monitored input is not delayed
for delay compensation, so that a performer hears themselves after the device round trip alone.
While no node reported latency that held by accident. Dynamics lookahead, oversampling and reverb
now do, and `signal_graph` compensated every connection the same way
([ADR-0022](0022-the-engine-compensates-recording-and-effect.md)). A monitored track's input,
which the engine writes into the track's graph source, was delayed exactly as its playback would
be, and the graph had no way to tell a live source from a playback one.

## Decision

- A graph source can be marked *live*. The mark takes effect at the next commit; only a source can
  carry it.
- A node is *live-only* when it is a live source, or when it has at least one input connection
  that does not close a loop and every such connection comes from a live-only node. Any other node
  with no audio inputs, such as a source that is not live, a curve or a generator, is never
  live-only.
- Arrival times are computed over connections from nodes that are not live-only. A connection from
  a live-only node is not compensated; every other connection is compensated as before. The plan
  latency is the latest such arrival at a sink that is not live-only.
- Once a live signal is summed with playback at a node, it shares that node's output and goes
  through the same compensation as the playback downstream.
- An automation binding on a live-only node uses the arrival of its own live signals.
- The engine bridge marks the sources of armed, monitored tracks live. Offline render marks none.

## Alternatives considered

- **Compensate everything, and subtract the live path's delay elsewhere** — the delay is already in
  the feed lines by the time the signal reaches a mix; nothing downstream can take it back out.
- **Keep the live signal undelayed even after it is summed with playback** — the sum is one buffer,
  so this would mean running the downstream nodes twice, once per alignment. It doubles the work
  of every shared bus for a case where the playback must stay aligned anyway.
- **A separate monitoring graph** — duplicates the track's effects and their state, and the
  monitored sound would no longer be what is recorded through the shared routing.
- **Mark live nodes in the model** — monitoring is session state
  ([ADR-0079](0079-arm-state-is-session-only-and-inputs-open-only.md)); putting it in the saved routing would change a persisted format for
  something that is not saved.

## Consequences

- A monitored track whose effects report latency is heard after that latency only, and no longer
  raises the plan latency or shifts where takes are placed.
- Where a monitored signal meets playback on a shared bus, it is delayed from there to keep
  playback aligned: monitoring through a latency-adding bus that also carries playback is not
  undelayed. Routing a monitored track straight to the output avoids it.
- Changing monitoring recompiles the plan. Monitoring changes only while the transport is stopped,
  so the new compensation lines starting silent is not heard.
- The real-time code is unchanged; only the feed delay lengths differ.
