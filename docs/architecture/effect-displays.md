# Effect Displays

## Effect displays

An effect's dialog shows a display area between its header and its controls: the dialog's width
less its padding, 320 logical pixels tall, scrolling with the controls. An effect kind may register
an `EffectDisplay` (`crates/debussy_ui/src/effect_display.rs`), which is given each parameter's
name, current value (the dragged one while dragging) and unit, and a `DisplayContext`: the
effect's kind, the tone, the project's sample rate, and the area's size in physical pixels. It
returns non-premultiplied RGBA pixels, stretched to the area. A kind with no display, or a
display that returns nothing, leaves the area charcoal grey (`#2B2B2B`) in both tones
([ADR-0091](../adrs/0091-effect-displays-are-registered-per-effect-kind-and.md)). It runs on the
interface thread only when the kind, a parameter value, the tone, the sample rate or the area's
physical size changes, and depends on nothing else
([ADR-0095](../adrs/0095-rasters-reach-the-gpu-in-strips-under-the-renderer.md)).

## Equaliser displays

`high-pass`, `low-pass` and `parametric-eq` share one display, `ResponseDisplay`
(`crates/debussy_ui/src/response_display.rs`), which plots the exact magnitude response their
parameter values ask for at the project's sample rate, as `audio_effects::frequency_response`
gives it (`docs/architecture/effects.md` "Frequency response"):

- Axes are fixed: 20 Hz to 20 kHz on a log scale across, +12 dB at the top to −96 dB at the
  bottom. The plot runs to the area's top and right edges, with gutters of 46 logical pixels on
  the left and 20 at the bottom for labels.
- Grid lines, the same grey in both tones, mark 20, 50, 100, 200, 500 Hz, 1, 2, 5, 10 and 20 kHz,
  and every 12 dB; the 0 dB line is brighter. A line on the plot's edge is drawn just inside it.
- Each label carries its unit with no space, such as `20Hz`, `1kHz`, `+12dB` and `−96dB`, in light
  grey. Frequency labels sit centred under their lines and gain labels right-aligned to the left
  of theirs, each moved inward as far as needed to stay inside the area.
- The curve is a 2-logical-pixel line in the tone's audio colour, sampled twice per physical
  column and at each notch's centre. Gains beyond the axes are drawn on the plot's edge, the
  line kept wholly inside it; nothing is drawn from the Nyquist frequency up.
- The background is transparent, so the area's charcoal shows through.
- Labels are drawn into the pixels through the `Typeface` trait (`crates/debussy_ui/src/
  typeface.rs`); `StrokeFont`, an in-house face of straight strokes, implements it
  ([ADR-0092](../adrs/0092-equaliser-displays-plot-the-exact-digital.md)).

## Dynamics display

`dynamics` has `DynamicsDisplay` (`crates/debussy_ui/src/dynamics_display.rs`): two plots side by
side, each half the area's width less a 12-logical-pixel gap, each with gutters of 46 logical
pixels on the left and 20 at the bottom for labels, grid and labels drawn as in "Equaliser
displays", and the curves in the tone's audio colour:

- **Envelope**, on the left: the window weights of `docs/architecture/effects.md` "Dynamics"
  against time on one linear axis, from the oldest sample weighed (or the current sample, if
  later) at the left to the lookahead at the right, so each side's width is in proportion to its
  length; with both sides zero the window is a single vertical line
  ([ADR-0103](../adrs/0103-the-dynamics-envelope-plot-has-one-linear-time.md)). Weight runs from 0 at the bottom to 1 at the
  top, with grid lines at 0, 0.5 and 1. A brighter vertical line marks the current sample. The
  labels under the plot give the oldest sample's age at the left, the lookahead at the right and
  `0ms` at the current sample when there is room, such as `−110ms` and `+10ms`.
- **Transfer curve**, on the right: the steady-state output level for each input level, through
  the preamp, curve, output gain and ceiling (`audio_effects::static_output`), with input from −96
  to +12 dB across and output from −96 to +12 dB up, grid lines every 24 dB across and every 12 dB
  up, and 1-pixel red lines over the grid at 0 dB in and 0 dB out (on each half when asymmetric)
  ([ADR-0104](../adrs/0104-the-dynamics-controls-mirror-the-positive-and.md)). When `asymmetric` is on, the plot is split by a centre line at −96 dB: the
  positive samples' curve rises above it to +12 dB at the top and the negative samples' curve
  falls below it to +12 dB at the bottom, with grid lines every 36 dB in each half

([ADR-0101](../adrs/0101-the-dynamics-display-shows-the-envelope-window.md)).
