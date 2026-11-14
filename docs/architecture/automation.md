# Automation

## Curves

An automation curve (`modulation::Curve`) is a normalised value from 0 to 1 over timeline frames.
It is real-time safe to read: evaluation allocates nothing, takes no locks and does no I/O (see
`docs/architecture/engine.md` "Real-time thread").

- **Points.** A curve is a list of anchor points (`modulation::Point`), each a timeline frame
  (`i64`), a value from 0 to 1, and an in-handle and out-handle (`modulation::Handle`), each an
  offset `(d_frame: f64, d_value: f32)` from the anchor. Anchor frames strictly increase. The
  first point's in-handle and the last point's out-handle are unused but must be finite.
- **Segments.** Points `i` and `i + 1` are joined by a cubic Bézier with control points
  anchor `i`, anchor `i` + its out-handle, anchor `i + 1` + its in-handle, and anchor `i + 1`.
- **Function of time.** Every segment's x must never decrease. With the segment's x control
  values `x0` to `x3` and `a = x1 − x0`, `b = x2 − x1`, `c = x3 − x2`, it is valid exactly when
  `a ≥ 0`, `c ≥ 0`, and `b ≥ 0` or `b² ≤ ac`. `Curve::validate` checks this, finite handles,
  values from 0 to 1 and increasing frames, and names the first point or segment that fails.
  A `Curve` can only be made valid; an edit that would make it invalid is refused.
- **Evaluation** (`Curve::value_at`, or `modulation::Cursor` for sequential reads in constant
  time):
  - Before the first point the value is the first point's; from the last point on it is the last
    point's. A curve with no points is 0.5.
  - Inside a segment, 52 steps of `f64` bisection on `x(t)` find the curve parameter `t` for the
    frame, and the value is `y(t)`, clamped to 0 to 1.
- **Parameter value.** `signal_graph::map_to_parameter` maps a curve value onto a parameter's
  range: 0 to the minimum and 1 to the maximum, evenly in the value for a linear scale and in its
  logarithm for a logarithmic one.
- **Eligibility.** A curve may drive a parameter only when `ParameterInfo::is_automatable` holds:
  the parameter is not stopped-only and its scale is not stepped (integer, choice or toggle).
- **Patent constraint.** No command splits an anchor's value into a step (two values at one
  frame) without a new patent check (see `docs/architecture/patents.md` "Patent policy").

The curve maths is built in-house ([ADR-0123](../adrs/0123-automation-curves-are-cubic-b-zier-segments-kept.md)).

## Curves in the project

- **Lanes.** `State.curves` holds automation curve lanes (`project_model::CurveLane`: id, name,
  curve), shown below the tracks in list order. A curve id comes from the session's shared id
  counter.
- **Curve ports.** A node's `curve_ports` maps a parameter name to a curve port number, from 1,
  unique within the node. A new assignment takes the lowest free number. Only a mixer's or a
  known effect's parameter for which `ParameterInfo::is_automatable` holds may be assigned. A
  saved port on an effect this build does not know is kept unchecked. Fewer mixer channels are
  refused while a removed channel's parameter has a port.
- **Control wires.** A connection from `Endpoint::Curve` (output 0) to `Endpoint::CurvePort`
  (input 0) drives the port's parameter. A curve connects only to a curve port, and a curve port
  only from a curve. A curve port takes one connection; a curve may feed many. Control wires
  carry no side.
- **Edits.** Each returns its exact inverse:
  - `InsertCurve`, `RemoveCurve` and `RenameCurve`. `RemoveCurve` is refused while the curve
    has connections; `Session::remove_curve` removes them in the same step.
  - `SetCurve` replaces the curve. Points that fail `Curve::validate` are refused when the curve
    is made (`Session::set_curve`), so a drag gesture records one valid curve.
  - `AssignCurvePort` names its port number; `UnassignCurvePort` is refused while the port is
    connected. `Session::unassign_curve_port` removes the connection in the same step.
  - Removing a node also removes its curve-port connections.
- **Saved format.** Since format version 5 the project carries a required `curves` list (`id`,
  `name`, `points`; each point `frame`, `value`, `in_handle` and `out_handle`, each handle
  `d_frame` and `d_value`), each node a required `curve_ports` object, and an endpoint may be
  `curve` (`id`) or `curve_port` (`node`, `port`). Version 4 files migrate to no curves and no
  ports.
- **Loading.** Load refuses more than 10,000 curves or 1,000,000 points over all curves, a curve
  that fails `Curve::validate`, curve names over 1024 bytes, more curve ports on a node than its
  parameter limit, and curve ports or control wires the model refuses.

The project's curve state and format 5 are recorded in
[ADR-0124](../adrs/0124-project-format-5-saves-automation-curve-lanes.md).

## Evaluation

The engine reads curves from the plan; no message carries a curve value (see
`docs/architecture/signal-graph.md` "Port types").

- **Graph.** `Graph::add_curve` adds a curve source holding an immutable `Arc<modulation::Curve>`;
  `Graph::set_curve` replaces it, keeping its id. `Graph::connect_curve` binds a curve to a
  processor's parameter by name: only an automatable parameter, one curve per parameter, any
  number of parameters per curve. The bridge connects a curve port's wire to the parameter the
  port is assigned to; a port on an unknown effect's stand-in is not connected.
- **Bindings.** Compiling a plan turns each curve-to-parameter connection into a binding: the
  curve, the parameter, and the node's arrival time (see "Delay compensation" in
  `docs/architecture/signal-graph.md`).
- **Transport.** `Runner::set_transport(position, playing)` gives the timeline frame of the
  block's first source frame and whether it advances; `audio_engine` passes the playhead in
  playback and the range position in offline render.
- **Control grid.** While playing, a plan with bindings runs in pieces split at timeline frames
  that are multiples of `signal_graph::CONTROL_PERIOD` (32). Before each piece, each binding
  reads its curve at the next multiple minus its node's arrival time, maps it with
  `map_to_parameter`, and sets the parameter to reach that value at that multiple. Cycles keep
  their own sub-blocks inside each piece. Output does not depend on the block size.
- **Stopped.** While stopped, each binding reads its curve once per block at the position and
  reaches it over the block.
- **Override.** A bound parameter is automated: `Parameter::get` gives the curve's value, and
  `Parameter::set` changes only `Parameter::base`, the value it returns to. A reader reaches an
  automated value by the next control point (`Smoothed::update`, `Ramp::follow`). When a new plan
  no longer binds a parameter, the parameter is released and returns to its base value over the
  normal 10 ms ramp.
- **Readers.** `Smoothed` follows automation for every parameter it reads. A node that ramps a
  value derived from a parameter calls `Ramp::follow` with that parameter; the mixer does so for
  channel gain. A derived ramp that calls `Ramp::set_target` ramps over 10 ms from each control
  point.

The evaluation rules are recorded in
[ADR-0125](../adrs/0125-curves-drive-parameters-on-a-32-frame-timeline.md).
