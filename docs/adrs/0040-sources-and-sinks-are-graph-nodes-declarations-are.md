# ADR-0040 — Sources and sinks are graph nodes, declarations are fixed when a node is added, and loop-closing connections are not compensated

- **Status:** Accepted
- **Decided by:** agent, ratified by Chris Paro
- **Date:** 2026-11-06
- **Issue:** #37
- **Architecture:** [Delay compensation](../architecture/signal-graph.md#delay-compensation)


## Context

Building `signal_graph` (#37) on ADR-0038 left questions that decision did not settle: how the
engine puts signals into the graph and takes them out, how latency is computed when the graph has
cycles ([ADR-0016](0016-feedback-loops-must-pass-through-a-fixed-delay.md),
[ADR-0022](0022-the-engine-compensates-recording-and-effect.md)), how the graph learns a node's
ports and latency once the node itself lives in a plan on the real-time thread, and what bounds a
feedback delay's length.

## Decision

- **Sources and sinks:** the engine supplies signals (track playback, inputs) through *source*
  nodes and takes them (the project output) from *sink* nodes, each of a fixed layout.
- **Declarations:** a node's ports, latency and parameters are read once, when it is added, and
  are fixed while it is in the graph. A node whose ports or latency change is replaced.
- **Compensation:** latency is worked out over every connection except those that close a
  feedback loop (a connection into a feedback delay from inside the same cycle). Those are not
  compensated. Every sink is delayed to the latest arrival at any sink, which is the plan's
  latency.
- **Pan:** where a mono signal enters a stereo input, the pan is a property of the connection,
  centre by default, changed by committing a new plan.
- **Feedback delays:** at most 10 s. A new length takes effect with the next plan, keeping the
  delay's contents only if its length in frames is unchanged. Compensation delays start silent
  in each new plan.

## Alternatives considered

- **Engine writes into node buffers by index** — no source nodes, but tracks would not be nodes
  of the graph, against [ADR-0015](0015-routing-is-one-general-processing-graph-per.md).
- **Querying nodes for latency at each commit** — would let latency change in place, but the node
  is on the real-time thread by then; asking it would need a message round trip per commit.
- **Compensating loop-closing connections too** — there is no consistent arrival time around a
  loop; any delay added there only lengthens the loop the user set.
- **Aligning each sink only to its own inputs** — outputs (main, headphones) could drift apart by
  different latencies, and the engine would have to track several.
- **Unbounded feedback delays** — a mistyped length could ask for gigabytes of delay line.

## Consequences

- A lookahead effect whose latency depends on a parameter must be replaced to change it, which
  resets its state; none exists yet.
- Changing a feedback delay's length or a connection's pan rebuilds the plan, so they are not
  smooth while dragged; continuous panning belongs in a pan node.
- A latency inside a loop lengthens that loop, as the node's own delay would.
