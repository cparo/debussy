# ADR-0124 — Project format 5 saves automation curve lanes, curve ports and control wires

- **Status:** Accepted
- **Decided by:** agent, ratified by Chris Paro
- **Date:** 2026-11-14
- **Issue:** #63
- **Architecture:** [Curves in the project](../architecture/automation.md#curves-in-the-project)

## Context

Issue #63 puts automation curves in the project: lanes in the track pane that drive effect and
mixer parameters through numbered curve ports, wired on the patch board (curve maths in
ADR-0123). That is new model state, new edits and a change to the persisted format, which
CLAUDE.md puts to a human (`product`). Chris Paro approved format 5 as described here when he
approved the plan for #63.

## Decision

- Curves are lanes in `State.curves`, with ids from the session's shared counter.
- A node holds its curve ports as `parameter name → port number` (from 1, lowest free on
  assignment), in the model, not in the effect. No effect code changes.
- Control wires reuse `Connection` with two new endpoints, `Curve(CurveId)` and
  `CurvePort(NodeId, u32)`, each with port 0. The model type-checks them: control to control
  only, one wire per curve port, fan-out allowed.
- Only an automatable parameter (ADR-0123) of a mixer or known effect may be assigned. The model
  asks `audio_effects` for the parameter. A saved port on an unknown effect is kept, as an
  unknown effect itself is.
- Edits: `InsertCurve`, `RemoveCurve`, `RenameCurve`, `SetCurve`, `AssignCurvePort` and
  `UnassignCurvePort`. Removals are refused while wired; the session methods remove wires in the
  same step. `SetCurve` takes a `Curve`, which is valid by construction.
- Format 5: required `curves` and per-node `curve_ports`, endpoints `curve` and `curve_port`.
  Migration from 4 adds empty ones. Load limits: 10,000 curves and 1,000,000 points, each curve
  validated.

## Alternatives considered

- **Curve ports declared by each effect's `Node` impl.** Rejected: every effect would change, and
  assignment is a per-project choice, not a property of the effect.
- **A separate `ControlConnection` list.** Rejected: wires share selection, undo, saving and the
  patch board with audio connections. Two lists would duplicate all of it for a type check.
- **Optional fields defaulting to empty**, with no version bump. Rejected: the format requires
  every field, and an older build silently dropping unknown curves would lose work. Version 5
  makes an older build refuse the file.
- **Checking automatability in the interface only.** Rejected: a file from elsewhere could assign
  a stepped parameter. The model is the one place every edit and load passes through.

## Consequences

- `project_model` now depends on `audio_effects` and `modulation`, both internal crates in an
  earlier tier. Checking an assignment builds the effect to read its parameters, which
  allocates. That is acceptable off the audio thread, but it is not free on large loads.
- A file from this build cannot be opened by a format-4 build.
- The running graph ignores control wires until the engine slice of #63 lands.
