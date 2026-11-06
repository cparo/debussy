# Signal Graph

## Processing graph

All audio routing is one processing graph per project. Tracks, effects, buses, aux sends and
outputs are nodes; connections carry audio between node ports. The mixer and the patch board are
views of this graph, not separate models ([ADR-0015](../adrs/0015-routing-is-one-general-processing-graph-per.md)).

## Feedback loops

A connection that forms a cycle is allowed only if the cycle passes through at least one
feedback-delay node, whose delay length is set by the user. Connections forming a cycle without
one are refused.

Rendered output must not depend on the buffer size. The engine processes the nodes of a cycle in
sub-blocks no longer than the shortest feedback delay in that cycle ([ADR-0016](../adrs/0016-feedback-loops-must-pass-through-a-fixed-delay.md)).

## Channel layouts

Every connection carries either a mono or a stereo signal. The project output is a stereo mix; a
mono mix is derived from it by summing its channels ([ADR-0017](../adrs/0017-signals-are-mono-or-stereo-only.md)).

## Panning

Mono signals are panned into stereo with a -3 dB constant-power pan law ([ADR-0018](../adrs/0018-panning-uses-a-3-db-constant-power-law.md)).
