# ADR-0043 — Routing is undoable project state: tracks are bare sources, one fixed output, effects by kind id with parameters by name

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-07
- **Issue:** #45
- **Architecture:** [Routing](../architecture/project.md#routing)


## Context

[ADR-0042](0042-tracks-have-a-fixed-layout-with-paired-mono-files.md) left mixer and routing state
out of `project_model`, for #45. The running graph (`signal_graph`) is rebuilt from the project and
holds no saved form of its own. Routing changes need a saved form that can be undone and captured
by named versions ([ADR-0012](0012-undo-is-unlimited-per-session-and-named-versions.md)). Its shape
was put to the developer.

## Decision

- **Home:** routing is part of `project_model`'s `State`, with its own `Edit` variants, in the same
  undo history and named versions as tracks and clips.
- **Tracks:** a track is a bare source with one output of its layout. There is no built-in channel
  strip: gain, pan and mute are ordinary nodes. A new track is connected to the project output,
  panned centre, in the same undo step. Removing a track also removes its connections.
- **Output:** every project has exactly one stereo output, which cannot be removed.
- **Nodes:** an effect is saved as a stable kind id (such as `"eq"`) with the parameter values that
  differ from its defaults, keyed by parameter name, matching
  [ADR-0041](0041-replacing-a-node-keeps-its-id-compatible.md). A kind id the application does not
  know is kept and plays as silence. Buses and feedback delays are their own node kinds.
- **Parameter undo:** one step per completed gesture
  ([ADR-0037](0037-overlapping-clips-the-later-covers-with-a-short.md)). While a control is
  dragged, values go only to the running graph's parameters; the edit is recorded on release.
- **Checks:** the model checks references: endpoints exist, the output is never a source, a track
  is never a target, ports exist where the model knows them (tracks, output, buses, delays), and
  no connection is duplicated. Effect ports and cycle rules are `signal_graph`'s, checked when the
  edit reaches the running graph.

## Alternatives considered

- **A separate routing model crate** — two histories would have to be merged into one undo order.
- **A fixed channel strip per track** — fewer nodes for simple mixes, but chosen against, so that a
  track has nothing the graph cannot express.
- **A closed enum of effects** — every new effect would change the model and the saved format.
- **Parameter changes outside undo** — a mix move could not be taken back.
- **Unconnected new tracks** — a new track would be silent until patched.
- **Cycle checks in the model** — would duplicate `signal_graph`'s rules and could drift from them.

## Consequences

- The application must apply each routing edit to the running graph and record it only if the graph
  accepts it. Undo and redo only re-create states the graph already accepted.
- The effect kind ids and parameter names become part of the saved format (#45, `project_storage`).
- A default mixer layout (channel-strip nodes for a new track) can be added later as a template,
  without changing the model.
