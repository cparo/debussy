# ADR-0054 — A new engine_bridge crate keeps the running graph in step by diffing checked states

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-07
- **Issue:** #54
- **Architecture:** [Routing](../architecture/project.md#routing)
  ([Crate map](../architecture/crates.md#crate-map))

## Context

ADR-0043 leaves effect ports and cycle rules to `signal_graph`, so a routing edit is recorded
only if the running graph accepts it, and undo, redo, version restore and load must reach the
graph too. ADR-0047 also left the conversion of project clips into the engine's `ChannelClip`s for
later. `audio_engine` cannot see `project_model` (`docs/architecture/crates.md` "Dependency
rules"). Where the bridge lives, how changes reach the graph, and its scope were put to the
developer for #54.

## Decision

- **Home:** a new core crate, `engine_bridge`, depending on `project_model`, `signal_graph`,
  `audio_effects`, `audio_engine`, `dsp` and `audio_buffers`.
- **Sync by diff:** one `sync(state)` compares the model's routing with a mirror of the graph and
  makes the graph calls between them: add, remove or replace nodes (`replace_node` for a changed
  processor, keeping its id, ADR-0041), connect and disconnect, set pans, parameters and delay
  times, then commit. Undo, redo, restore and load just sync.
- **Check first:** before touching the running graph, the target state is built into a scratch
  graph, which applies every `signal_graph` rule. If it fails, the running graph is untouched and
  the change is reverted with `Session::revert_last`, which leaves no redo step.
- **Scope:** the bridge also turns tracks into per-channel `ChannelClip`s and `TrackRenderer`s
  with a table of the files they read, for playback through disk streams and for export, and
  builds a separate graph for offline render.
- **Parts it needed:** `audio_effects` gains a
  `Bus` node, a `Silence` stand-in for unknown effect kind ids (stereo ports, enough for the
  connections saved with it) and an `effect(kind)` registry, empty until #58–#62.
  `signal_graph` gains `node_ids` and `connections` listings. Parameter values are pushed to the
  graph only when the model's value changes, so a sync does not undo a control being dragged.

## Alternatives considered

- **Inside `project_model`, or the `debussy` binary:** the model would take on running concerns,
  or the bridge would be hard to test in isolation.
- **Translating each `Edit` to graph calls:** cheaper per edit, but every variant, `Batch` and
  `Restore` needs its own mapping, and inverses must match exactly.
- **Rolling the running graph back after a refusal:** needs an inverse for every graph call; the
  scratch build avoids it.
- **Routing only:** would leave the project unable to play end to end until another ticket.

## Consequences

- Each routing change builds a scratch graph, which costs time proportional to the routing,
  not the clips.
- A refused change clears the redo history, as any new edit would.
- The crate map grows to 18 crates.
