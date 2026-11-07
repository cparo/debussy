# Signal Graph

## Processing graph

All audio routing is one processing graph per project. Tracks, effects, buses, aux sends and
outputs are nodes; connections carry audio between node ports. The mixer and the patch board are
views of this graph, not separate models ([ADR-0015](../adrs/0015-routing-is-one-general-processing-graph-per.md)).

A processing plan is compiled from the graph off the real-time thread and sent to it over a
lock-free queue. The real-time thread swaps plans at a block boundary and returns the old plan over
a second queue, so it is dropped elsewhere. Node parameters are atomic values, read once per block
and ramped over about 10 ms. Acyclic parts of a plan run once per block; each cycle runs in
sub-blocks no longer than its shortest feedback delay ([ADR-0038](../adrs/0038-processing-plans-are-swapped-over-queues.md)).

## Feedback loops

A connection that forms a cycle is allowed only if the cycle passes through at least one
feedback-delay node, whose delay length is set by the user. Connections forming a cycle without
one are refused.

Rendered output must not depend on the buffer size. The engine processes the nodes of a cycle in
sub-blocks no longer than the shortest feedback delay in that cycle ([ADR-0016](../adrs/0016-feedback-loops-must-pass-through-a-fixed-delay.md)).

A feedback delay is at least 1 ms, stored as a time and rounded up to whole samples at the project
rate. The UI does not offer shorter values, and a shorter stored or automated value is raised to
1 ms ([ADR-0031](../adrs/0031-feedback-delays-are-at-least-1-ms.md)).

## Channel layouts

Every connection carries either a mono or a stereo signal. The project output is a stereo mix; the
mono mix is derived from it as `(L + R) × 0.7071` ([ADR-0030](../adrs/0030-signals-are-mono-or-stereo-and-the-mono-mix-is-the.md)).

Every bus and node input has a fixed layout. A mono signal entering a stereo input is panned
(centre by default); a stereo signal entering a mono input is folded down ([ADR-0038](../adrs/0038-processing-plans-are-swapped-over-queues.md)).

## Panning

Mono signals are panned into stereo with a -3 dB constant-power pan law ([ADR-0018](../adrs/0018-panning-uses-a-3-db-constant-power-law.md)).

## Effect nodes

Effects are built-in nodes. The effect node interface must not assume an effect is built in: it
reports its latency, declares its ports, and exposes its parameters through the same interface a
third-party plugin wrapper would implement. Third-party plugins are not hosted ([ADR-0019](../adrs/0019-only-built-in-effects-for-now-behind-a-plugin.md)).

## Port types

Every port and connection has a type. Audio is the only type; connections between ports of
different types are refused. Track contents are typed in the same way: audio clips are the only
kind ([ADR-0020](../adrs/0020-ports-and-track-contents-are-typed-so-midi-can-be.md)).

## Delay compensation

Each node reports the latency it adds. The engine delays parallel paths so that signals meeting at
a node are time-aligned, and reports the total output latency ([ADR-0022](../adrs/0022-the-engine-compensates-recording-and-effect.md)).
