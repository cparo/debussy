# ADR-0041 — Replacing a node keeps its id, compatible connections and parameter values by name

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-06
- **Issue:** #37
- **Architecture:** [Processing graph](../architecture/signal-graph.md#processing-graph)


## Context

[ADR-0040](0040-sources-and-sinks-are-graph-nodes-declarations-are.md) fixes a node's ports,
latency and parameters when it is added, and says a node whose declarations change is replaced. The
only way to replace one was to remove it and add another. That loses its connections, its
`NodeId`, and the parameter values the user had set, which live in the old node's parameters.
Parameter *values* change live through atomics and never need a replacement; only a change to the
declared set does.

## Decision

- `Graph::replace_node(id, node)` swaps in a new processor under the same id. Sources, sinks and
  feedback delays cannot be replaced.
- Each connection to or from the node is kept if its port index still exists with the same signal
  type; a layout change is allowed, since pan and fold-down adapt it. The others are removed, and
  their ids are returned.
- Each of the new node's parameters takes the value of the old parameter with the same name,
  clamped to the new range, before the node is prepared. Unmatched parameters keep their defaults.
- The new node starts with its own internal state. The old node is dropped off the real-time
  thread when its plan comes back.

## Alternatives considered

- **Remove and add** — what existed: the caller must reconnect and re-set every value, and anything
  holding the old id loses track of the node.
- **Copy parameter values by index** — simpler, but wrong when parameters are reordered or one is
  inserted.
- **Never copy values** — the user's settings would be lost whenever a node's shape changes.
- **Refuse the replacement if any connection no longer fits** — safer, but a node losing a port
  could then never be replaced without first disconnecting it by hand.

## Consequences

- Parameter names are an identity: renaming one in a node loses its value on replacement.
- Port indices are an identity: reordering ports reconnects signals to different ports.
- Internal state (delay buffers, filter history) is not transferred.
