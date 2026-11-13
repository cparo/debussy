# ADR-0111 — Nodes receive the project tempo before they are prepared; a tempo change replaces the nodes that use it

- **Status:** Accepted
- **Decided by:** agent, ratified by Chris Paro
- **Date:** 2026-11-13
- **Issue:** #61
- **Architecture:** [Effect nodes](../architecture/signal-graph.md#effect-nodes)

## Context

Issue #61's delay syncs its time to the project tempo, but nodes saw no tempo: it lived in
`project_model::Tempo` and only the click used it. The tempo changes only while stopped
(ADR-0082), as a latency-setting parameter does (ADR-0100), so the same replace-in-place path
can carry it.

## Decision

- `signal_graph::Node` gains two defaulted methods: `uses_tempo()` (false) and
  `set_tempo(frames_per_whole_note)` (ignored). Existing nodes do not change.
- The tempo crosses as the length of a whole note in frames at the graph's rate:
  `frames_per_beat × beat_unit`. A node turns note values into frames with no knowledge of
  meters or BPM.
- `Graph` holds the value (120 BPM in 4/4, two seconds, until set) and gives it to every
  processor before `prepare`. `Graph::set_tempo` stores a new value and returns the processors
  that declared `uses_tempo`.
- `engine_bridge`'s target carries the project tempo. `Mirror::build` sets it before adding
  nodes; `Mirror::sync` sets it first and replaces each returned node with a fresh one of its
  kind, keeping id, connections and values. The bridge's launch, every edit, undo, a restored
  version and an offline render all pass through the target, so no other caller sets it.

## Alternatives considered

- **Tempo in `signal_graph::Config`.** `Config` is fixed for the graph's lifetime and derives
  `Eq`; the tempo changes and is an `f64`. A graph field with a setter fits the lifetime.
- **A tempo parameter on the node.** It would be saved per node and could disagree with the
  project's tempo.
- **Pushing tempo to running nodes through an atomic.** It would let tempo change while playing,
  which ADR-0082 does not allow, and every tempo user would need its own glide; replacement
  reuses the ADR-0100 path.
- **BPM and meter instead of frames per whole note.** Every node would repeat the arithmetic,
  and the graph already deals in frames.

## Consequences

- A tempo change resets the state of every tempo user, such as a delay's buffer; tempo changes
  only while stopped, so nothing audible is cut off.
- A future tempo-following node only implements the two methods.
