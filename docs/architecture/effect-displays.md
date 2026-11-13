# Effect Displays

## Effect displays

An effect's dialog shows a display area between its header and its controls: the dialog's width
less its padding, 320 logical pixels tall, scrolling with the controls. The area shows the picture
of the `Curve` its parameter values (the dragged one while dragging) ask for
(`docs/architecture/effects.md` "Effect curves"). `draw` in
`crates/debussy_ui/src/effect_display.rs` matches on the curve's variant and calls that kind of
curve's `render`, given a `DisplayContext`: the tone and the area's size in physical pixels. It
returns non-premultiplied RGBA pixels, stretched to the area. A kind with no curve, or a picture
that cannot be drawn, leaves the area charcoal grey (`#2B2B2B`) in both tones
([ADR-0105](../adrs/0105-effect-curves-are-described-per-kind-in-audio.md)). It runs on the
interface thread only when the curve, the tone or the area's physical size changes, and depends
on nothing else ([ADR-0095](../adrs/0095-rasters-reach-the-gpu-in-strips-under-the-renderer.md)).

## Display plots

Displays that plot values on axes draw with the toolkit in `crates/debussy_ui/src/plot.rs`; each
display's `render` calls it in order and chooses its own axes, marks, line colours and where
undefined values go.

- The plot runs to the area's top and right edges, with gutters of 46 logical pixels on the left
  and 20 at the bottom for labels.
- Grid lines are 1 logical pixel wide and span the plot; a line on the plot's edge is drawn just
  inside it.
- Labels are 9 logical pixels to the capital height, in light grey, 5 logical pixels from the plot
  or their line. Labels below the plot are centred on their marks; labels in the left gutter are
  right-aligned, centred on theirs. Each label is moved inward as far as needed to stay inside the
  area.
- When any two neighbouring marks on an axis lie closer than 1.4 times the widest label's extent
  along it, only every other mark is labelled, starting from the low end of the axis.
- The curve is a 2-logical-pixel line in the tone's audio colour, sampled twice per physical
  column. Values beyond the axes are drawn on the plot's edge, the line kept wholly inside the
  plot on all four sides.
- The background is transparent, so the area's charcoal shows through.
- Labels are drawn through the `Typeface` trait (`crates/debussy_ui/src/typeface.rs`);
  `StrokeFont`, an in-house face of straight strokes, implements it
  ([ADR-0092](../adrs/0092-equaliser-displays-plot-the-exact-digital.md),
  [ADR-0104](../adrs/0104-effect-displays-share-a-plot-toolkit-the-curve.md)).

## Equaliser displays

A `Response` curve, which `high-pass`, `low-pass` and `parametric-eq` give, is drawn by
`crates/debussy_ui/src/response_display.rs`. It plots the exact magnitude response their parameter
values ask for at the project's sample rate (`docs/architecture/effects.md` "Frequency
response"):

- Axes are fixed: 20 Hz to 20 kHz on a log scale across, +12 dB at the top to −96 dB at the
  bottom, drawn as in "Display plots".
- Grid lines, the same grey in both tones, mark 20, 50, 100, 200, 500 Hz, 1, 2, 5, 10 and 20 kHz,
  and every 12 dB; the 0 dB line is brighter.
- Each label carries its unit with no space, such as `20Hz`, `1kHz`, `+12dB` and `−96dB`:
  frequencies below the plot and gains to its left.
- The curve is also sampled at each notch's centre, which is drawn on the plot's bottom edge;
  nothing is drawn from the Nyquist frequency up
  ([ADR-0092](../adrs/0092-equaliser-displays-plot-the-exact-digital.md)).

## Dynamics display

A `Transfer` curve, which `dynamics` gives, is drawn by `crates/debussy_ui/src/transfer_display.rs`.
It plots the steady-state output level over the detected input level that the parameter values ask
for: the curve, makeup and the ceiling if it is on, by the same mapping processing uses. Ballistics, lookahead and the detector mode are not shown.

- Axes are fixed: input from −96 dBFS at the left to +12 dBFS at the right, output from +12 dBFS at
  the top to −96 dBFS at the bottom, drawn as in "Display plots". Grey grid lines, the same in both
  tones, run every 12 dB on both axes, with gain labels on both.
- The 0 dBFS lines of both axes are 1 logical pixel wide and red (`#D03030`) in both tones, in
  place of the brighter 0 dB grey.
- An undefined output level is drawn at −96 dBFS; levels beyond the axes, such as a gate's
  floor, lie on the plot's edge ([ADR-0101](../adrs/0101-the-dynamics-display-plots-the-steady-state.md)).

## Amplifier display

A `Shaper` curve, which `amplifier` gives, is drawn by `crates/debussy_ui/src/amplifier_display.rs`.
It plots the static curve the parameter values ask for (see `docs/architecture/effects.md`
"Amplifier"): output over input sample, passing through the
origin. The DC filter's response and oversampling are not shown.

- Both axes are linear in sample value, from −10^(3/20) to +10^(3/20) (±3 dBFS),
  drawn as in "Display plots".
- Grey lines run through 0 on both axes. Lines at ±1 (0 dBFS) on both axes are 1 logical pixel wide
  and red (`#D03030`), as in "Dynamics display".
- Labels are shown only at ±1 (`0dB`) and at the axes' edges (`+3dB`), on both axes.
- An undefined output is drawn at 0
  ([ADR-0103](../adrs/0103-the-amplifier-display-plots-the-recentred-static.md)).
