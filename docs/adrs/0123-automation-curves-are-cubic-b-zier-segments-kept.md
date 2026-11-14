# ADR-0123 — Automation curves are cubic Bézier segments kept functions of time, evaluated by bisection

- **Status:** Accepted
- **Decided by:** agent, ratified by Chris Paro
- **Date:** 2026-11-14
- **Issue:** #63
- **Architecture:** [Curves](../architecture/automation.md#curves)

## Context

Issue #63 asks for expression curves driving effect and mixer parameters. The developer chose
cubic Béziers edited with points and handles, a normalised value mapped per parameter, and no
automation of stopped-only or stepped parameters. A Bézier segment can turn back in time when its
handles cross, which would give two values at one frame, so a rule was needed for what a curve may
be and how the engine reads one in real time.

Patent research ([automation-curve-patents.md](../research/automation-curve-patents.md)) found
three candidates with a printed term end in the future. Chris Paro checked their status on
2026-11-13: two in force, one lapsed. Stage 2 read the independent claims:

- US7496416B2 (Luxology, now Foundry), input/output curve editor: does not read. Its only
  independent claim needs a control point's value broken so the curve becomes a step at that
  point; here each anchor has one value and the curve is continuous. Its collinear-handle and
  single-valued-curve claims depend on that claim.
- US9952739B2 (Avid), modular audio control surface: does not read. Its independent claims need
  a control surface sending recorded, time-stamped adjustments to applications on external
  platforms; Debussy has no control surface or network link.
- US9991861B2 (Magix): lapsed, not read.

The closest art, US6154221A (1997, parametric curves with a monotonic-x check after a handle
drag), and the Bézier and de Casteljau work of about 1960, are expired.

## Decision

A curve is anchor points in strictly increasing frame order, each with a value from 0 to 1 and an
in- and out-handle stored as offsets; consecutive anchors form cubic Bézier segments. Every
segment must be a function of time: with x control differences `a`, `b`, `c`, `a ≥ 0`, `c ≥ 0`,
and `b ≥ 0` or `b² ≤ ac`. A `Curve` can only be constructed valid, and an edit that breaks the
rule is refused, not repaired. A value is read by 52 steps of `f64` bisection on `x(t)` and then
`y(t)`, clamped; a cursor makes sequential reads constant-time. The curve maths lives in the
`modulation` crate and is built in-house on the evidence above. `map_to_parameter` maps the
normalised value linearly, or across the logarithm for logarithmic parameters;
`ParameterInfo::is_automatable` excludes stopped-only and stepped parameters.

## Alternatives considered

- **Repair an invalid edit** (clamp the handle back, as US6154221A did). Rejected: the developer
  chose refusal, and refusal keeps the editor's behaviour predictable; the point stays where it
  last was valid.
- **Newton's method** for `x(t) = frame`. Faster on average but needs a fallback where `x′`
  reaches 0 (the vertical tangent the rule allows), and its step count varies. Fixed bisection
  costs the same every time and always converges on a non-decreasing `x`.
- **Solving the cubic in closed form.** Exact in principle, but cancellation near repeated roots
  makes it less robust than bisection, and choosing the root in `[0, 1]` needs branching.
- **Constrain handles to the segment** (`0 ≤ d_frame ≤ gap`). Simpler to check, but it forbids
  valid shapes the exact rule allows, such as crossed handles with a small `b`.
- **Allow equal anchor frames for a step.** Rejected: it would edge towards US7496416B2's
  broken-value step, in force until about 2027-04-30.

## Consequences

- Steps need two anchors at least one frame apart. A split-value command needs a new patent check
  first.
- Evaluating costs 52 cubic evaluations, a few hundred floating-point operations. The engine reads
  at control rate (a later ADR for #63), not every frame.
- Adding networked hardware control surfaces later would need US9952739B2 read again
  (term to about 2034-07-10).
- The evidence rests on web searches and Google Patents pages, not a patent database query. The
  quoted claims have not been checked against the USPTO grant PDFs. The note lists its gaps.
