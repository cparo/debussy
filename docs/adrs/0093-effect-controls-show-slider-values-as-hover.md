# ADR-0093 — Effect controls show slider values as hover tooltips and toggles as one named button, lit while on

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-10
- **Issue:** —
- **Architecture:** [Layout](../architecture/user-interface.md#layout)

## Context

The parametric equaliser's eight band strips, each with five controls, crowded the dialog. Every
slider had a name label above it and a value label below it, and the band's on/off toggle had a
label as well as a button. Effects to come with many bands, channels or parameters would crowd
the same way, so this is the pattern for them, not a one-off fix.

## Decision

- A slider (linear, logarithmic or integer) keeps its name label. Its value and unit appear only
  as a tooltip while the slider is hovered, which includes while it is dragged.
- A toggle is a single button labelled with the parameter's name (such as "Enabled"). It is lit
  while on and unlit while off, and clicking it flips the value. It has no separate name label.
- A choice stays a labelled pick list, whatever its number of options.
- These are rules of the generic control builder (`effect_panel.rs`), so they apply to every
  built-in effect, not only the equaliser.

## Alternatives considered

- **A single on/off button with a name label** (the previous form): rejected. It costs a label
  row, and one button does not show the other state.
- **Values in the label row ("Gain −3.0 dB")**: rejected. It saves a row, but every strip still
  shows text that changes with every drag.
- **A strip of two mutually exclusive buttons named by their values ("On" | "Off")**: tried first
  and rejected on review. It takes two buttons' width, and the values alone do not say what is
  being switched. One named button says both what is switched and, by being lit, its state.
- **A checkbox for toggles**: rejected. It needs a name label beside it, while a named button that
  lights up matches the toolbar's toggles.

## Consequences

- An exact value can be read only by hovering, so it cannot be read at a glance, and not at all on
  input with no hover. The equaliser display shows the overall effect.
- A toggle's state is shown by its highlight alone, so the lit and unlit styles must stay easy to
  tell apart in both tones.
- A parameter frozen while playing still shows "Stop to change" under its name, whatever its
  scale.
