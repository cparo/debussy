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

The engine supplies signals through source nodes and takes them from sink nodes, each of a fixed
layout. A node's ports, latency and parameters are read when it is added and are fixed while it is
in the graph; a node whose ports or latency change is replaced ([ADR-0040](../adrs/0040-sources-and-sinks-are-graph-nodes-declarations-are.md)).

Replacing a node keeps its id and every connection whose port index still exists with the same
signal type; the other connections are removed. Each new parameter takes the value of the old
parameter of the same name, clamped to its range, before the node is prepared. The new node starts
with fresh internal state ([ADR-0041](../adrs/0041-replacing-a-node-keeps-its-id-compatible.md)).

## Feedback loops

A connection that forms a cycle is allowed only if the cycle passes through at least one
feedback-delay node, whose delay length is set by the user. Connections forming a cycle without
one are refused.

Rendered output must not depend on the buffer size. The engine processes the nodes of a cycle in
sub-blocks no longer than the shortest feedback delay in that cycle ([ADR-0016](../adrs/0016-feedback-loops-must-pass-through-a-fixed-delay.md)).

A feedback delay is at least 1 ms, stored as a time and rounded up to whole samples at the project
rate. The UI does not offer shorter values, and a shorter stored or automated value is raised to
1 ms ([ADR-0031](../adrs/0031-feedback-delays-are-at-least-1-ms.md)).

A feedback delay is at most 10 s. A new length takes effect with the next plan, keeping the delay's
contents only if its length in frames is unchanged ([ADR-0040](../adrs/0040-sources-and-sinks-are-graph-nodes-declarations-are.md)).

## Channel layouts

Every connection carries either a mono or a stereo signal. The project output is a stereo mix; the
mono mix is derived from it as `(L + R) × 0.7071` ([ADR-0030](../adrs/0030-signals-are-mono-or-stereo-and-the-mono-mix-is-the.md)).

Every bus and node input has a fixed layout. A mono signal entering a stereo input is panned
(centre by default); a stereo signal entering a mono input is folded down ([ADR-0038](../adrs/0038-processing-plans-are-swapped-over-queues.md)). The pan
is set per connection ([ADR-0040](../adrs/0040-sources-and-sinks-are-graph-nodes-declarations-are.md)).

## Panning

Mono signals are panned into stereo with a -3 dB constant-power pan law ([ADR-0018](../adrs/0018-panning-uses-a-3-db-constant-power-law.md)).

## Mixer

A mixer is an ordinary node; a project may have any number of them. A mixer has one or more
channels, each with one mono input, and one stereo output. Channel N's input is port N − 1, named
`channel N`; the output is named `main`. A mixer of more channels replaces it with the same id
(see "Processing graph"); channels are only ever added at the end.

Each channel has three parameters, by name:

| Name | Range | Default | Meaning |
|---|---|---|---|
| `channel N gain` | −96 to +12 | 0 | Gain in dB; −96 is silence |
| `channel N pan` | −1 to 1 | 0 | Pan position, under the pan law in "Panning" |
| `channel N mode` | 0 to 2 | 0 | 0 plays, 1 is muted, 2 is soloed; read rounded to the nearest whole number |

Mute and solo are one parameter, so a channel is never both. Solo acts within its own mixer: while
any channel of a mixer is soloed, only its soloed channels play. Gain, pan, mute and solo changes
ramp like any parameter. A mono mix of a mixer is made by connecting its output to a mono input
(see "Channel layouts") ([ADR-0044](../adrs/0044-mixing-is-a-mixer-node-with-per-channel-gain-pan.md)).

## Effect nodes

Effects are built-in nodes. The effect node interface must not assume an effect is built in: it
reports its latency, declares its ports, and exposes its parameters through the same interface a
third-party plugin wrapper would implement. Third-party plugins are not hosted ([ADR-0019](../adrs/0019-only-built-in-effects-for-now-behind-a-plugin.md)).

Each parameter declares, besides its name, range and default, a unit (possibly empty), a scale
(linear, logarithmic, logarithmic with 0, integer, choice of named options by index, or toggle) and
a group (possibly empty). Logarithmic with 0 holds 0 or `min` (above 0) to `max`; a value below
`min / 2` is held as 0, one from there up to `min` as `min`
([ADR-0105](../adrs/0105-parameters-may-be-logarithmic-with-zero-and.md)). A stepped scale (integer, choice, toggle) holds only whole numbers. A parameter may be marked
stopped-only: it changes only while playback is stopped or paused, and no automation drives it. A
toggle may name its on and off states, and a parameter may name a toggle of its node that enables
it, both for display only ([ADR-0105](../adrs/0105-parameters-may-be-logarithmic-with-zero-and.md)). The
node dialog draws an
effect's controls from this metadata alone, one strip per group. The metadata is not saved
([ADR-0087](../adrs/0087-a-parameter-declares-its-unit-scale-and-group-so-a.md)). A change to a stopped-only value replaces
the node, prepared with the new value, so a value that sets its latency takes effect
([ADR-0099](../adrs/0099-the-dynamics-effect-is-a-weighted-rms-window.md)).

## Port types

Every port and connection has a type. Audio is the only type; connections between ports of
different types are refused. Track contents are typed in the same way: audio clips are the only
kind ([ADR-0020](../adrs/0020-ports-and-track-contents-are-typed-so-midi-can-be.md)).

## Delay compensation

Each node reports the latency it adds. The engine delays parallel paths so that signals meeting at
a node are time-aligned, and reports the total output latency ([ADR-0022](../adrs/0022-the-engine-compensates-recording-and-effect.md)).

Latency is compensated on every connection except one that closes a feedback loop: a connection
into a feedback delay from inside the same cycle. Every sink is delayed to the latest arrival at any
sink, which is the plan's reported latency. Compensation delays start silent in each new plan ([ADR-0040](../adrs/0040-sources-and-sinks-are-graph-nodes-declarations-are.md)).
