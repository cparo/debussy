# ADR-0100 — A latency-setting parameter replaces its node when it changes

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-12
- **Issue:** #59
- **Architecture:** [Effect nodes](../architecture/signal-graph.md#effect-nodes)

## Context

The `dynamics` lookahead (ADR-0099) changes the node's latency. The graph reads `Node::latency`
once, when a node is added or replaced, and delay-compensates parallel paths from it.
`engine_bridge` replaced a node only when its shape changed, so a new lookahead would leave
parallel paths misaligned.

## Decision

- `ParameterInfo::sets_latency()` marks a parameter whose value the node's latency depends on. It
  implies stopped-only.
- When a recorded change touches such a parameter, `engine_bridge` sets it on the running node and
  then calls `Graph::replace_node`. The new node takes the values by name before `prepare`, so its
  latency is read at the new value. Its graph id and connections are kept (ADR-0041), and the next
  plan compensates the new latency.
- The comparison is with the model's previous value, not the live handle's, because a control sets
  the handle while it is dragged, before the edit is recorded (ADR-0043).
- A node fixes its latency at `prepare` and ignores later changes to the value until it is
  prepared again.

## Alternatives considered

- **Re-reading `Node::latency` at every commit.** The node would have to resize its delay lines
  while in a plan, and that needs allocation on the real-time thread or a handoff the graph does
  not have.
- **Allocating the maximum lookahead and reporting the maximum latency always.** Simple, but every
  dynamics node would add 10 ms to the graph's latency even with lookahead off.

## Consequences

- A latency-setting change resets the node's state (levels, delay contents). This is acceptable
  only because the change is stopped-only.
- Any future latency-setting parameter on another effect gets the same handling with no bridge
  changes.
