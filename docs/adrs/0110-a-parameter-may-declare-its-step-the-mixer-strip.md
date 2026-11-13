# ADR-0110 — A parameter may declare its step; the mixer strip reads its controls from ParameterInfo

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-13
- **Issue:** —
- **Architecture:** [Layout](../architecture/user-interface.md#layout)

## Context

The architecture review of 2026-11-12 (item 4) found that the mixer's channels already declare
their range, unit, scale and group in `ParameterInfo` (ADR-0087), but the mixer dialog ignored
that metadata. `mixer_strip.rs` hard-coded the gain and pan ranges, the gain tip's format, the
end labels and the slider steps, and the dialog's arrow-key handling repeated its own stepping
arithmetic instead of calling the parameter view's stepping (ADR-0106). Items 2 and 3 of the
review had already given effects one parameter view and one stepping rule.

The mixer could not use that rule as it stood: ADR-0096 steps a linear slider by 1 % of its
range, which is 1.08 dB for gain, while the mixer (ADR-0089) moves gain in whole dB and pan by
0.01. Something had to declare the mixer's grid.

## Decision

`ParameterInfo` gains an optional `step`: the grid a control snaps to and an arrow press moves by.
It is display only, not saved, and does not change `constrain` or processing. A parameter with a
step is stepped by snapping its value to the grid and moving whole steps, then clamping; one
without keeps the per-scale rule of ADR-0096. A linear slider drags in the declared step, and its
value text takes its decimal places from the step.

The mixer's gain declares a step of 1 dB and its pan 0.01. The mixer strip takes each slider's
range, step, tic position (its default) and gain value text from the parameter's metadata, and
the arrow keys step gain and pan through the parameter view. Pan's "L" top, its reversed drawing,
its "L 35" text and the mute and solo buttons stay the mixer's own presentation (ADR-0089).
`MixerFocus` stays the mixer's layout of focus targets; it is not merged into the effect dialogs'
per-parameter focus.

## Alternatives considered

- **Keep the mixer's constants and only share the range.** Leaves two stepping rules and two
  formats to keep in step, which is what the review asked to remove.
- **Apply the 1 % rule to the mixer.** Would change gain to 1.08 dB steps and make values that
  are not whole dB, a visible behaviour change nobody asked for.
- **Make the step part of the stored value (round in `constrain`).** Would change what the
  engine and automation see and what is saved; the step is about controls, not the value.
- **Merge `MixerFocus` into `DialogFocus::Parameter`.** The mixer's Tab order (Channels field,
  then mute, solo, gain and pan per strip) does not follow its parameters' order, and mute and
  solo are two buttons over one choice parameter; an index into the parameters cannot name them.

## Consequences

- No user-visible change: the mixer still steps whole dB and 0.01 pan, labels +12dB and -96dB, and
  shows whole-dB tips.
- A future mixer range or default change (or a new node with its own grid) needs no dialog
  change; an effect may declare a step to get grid stepping.
- The step is a display hint only; nothing stops a dragged value from landing off-grid if a
  toolkit rounds differently, and saved values are whatever was set.
