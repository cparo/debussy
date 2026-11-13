# ADR-0104 — Effect displays share a plot toolkit; the curve stays inside the plot and crowded labels are thinned

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-12
- **Issue:** —
- **Architecture:** [Display plots](../architecture/effect-displays.md#display-plots)

## Context

An architecture review found that the three effect displays, `response_display.rs` (equalisers,
ADR-0092), `transfer_display.rs` (dynamics, ADR-0101) and `amplifier_display.rs` (amplifier,
ADR-0103), were near-copies. Each had its own `Plot`, `render`, grid, label placement and curve
drawing, and their tests were copied too. The equaliser display had become the shared frame by
accident: the other two imported its gutters, label sizes and dB helpers. The copies had also
drifted. The equaliser curve was kept inside the plot only at the top, and the others on all four
sides. Only the dynamics display thinned crowded labels. More displays are coming: the tone stack
(#137), delay and reverb (#61), chorus and flanger (#62), and the gain-reduction meter (#134).

## Decision

- A `plot` module in `debussy_ui` owns the frame: the gutters, label size and gap, line widths,
  linear and log axes, the dB axis and its labels, grid lines, label placement, the sample columns
  and the clipped polyline. A `Canvas` wraps the raster at the context's size and scale.
- It is a toolkit, not a declarative spec. Each display keeps a short `render` that calls it in
  order and chooses its own axes, marks, line colours and where undefined values go.
- One behaviour for every display. The curve is pulled half its width inside all four edges of
  the plot. When any two neighbouring marks on an axis are crowded, every other one is labelled,
  starting from the axis's low end.
- The frame is tested once, in `plot.rs`. Each display's tests cover only what is specific to its
  effect.
- How each display looks up its parameters by name or position is unchanged. That belongs to a
  separate change that gives parameter values types.

## Alternatives considered

- **A declarative y = f(x) spec** that each display fills in with axes, marks and a function. It
  was rejected because the delay, modulation and meter displays to come are not y = f(x) curves,
  and the spec would grow escape hatches for each of them.
- **Keep the copies.** This was rejected because each new display would copy the frame again,
  the drift between copies would grow, and frame fixes would have to land in every file.

## Consequences

- Behaviour changes slightly. The equaliser curve may move by under 1 pixel at the right and
  bottom edges. The amplifier's input labels thin at narrow widths. Grid lines are drawn columns
  first, so where a red line crosses a grey one, the row drawn later covers the intersection
  pixel.
- The toolkit's constants and its thinning rule apply to every display. A display that needs a
  different frame has to extend the toolkit, not fork it.
- The coming displays (#61, #62, #134, #137) can use the toolkit for their frames.
