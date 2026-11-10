# ADR-0093 — Effect controls show slider values as hover tooltips and two-valued parameters as unlabelled two-button strips

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
- A two-valued parameter, either a toggle or a choice of exactly two options, is a strip of two
  mutually exclusive buttons. Each button is named by the value it selects ("On" then "Off" for a
  toggle; a choice's options in their order), and the strip has no name label.
- A choice of three or more options stays a labelled pick list.
- These are rules of the generic control builder (`effect_panel.rs`), so they apply to every
  built-in effect, not only the equaliser.

## Alternatives considered

- **A single on/off button with a name label** (the previous form): rejected. It costs a label
  row, and one button does not show the other state.
- **Values in the label row ("Gain −3.0 dB")**: rejected. It saves a row, but every strip still
  shows text that changes with every drag.
- **A checkbox for toggles**: rejected. It needs a name label to mean anything, while a pair of
  named buttons explains itself and also covers two-option choices.

## Consequences

- An exact value can be read only by hovering, so it cannot be read at a glance, and not at all on
  input with no hover. The equaliser display shows the overall effect.
- A two-valued parameter's meaning must be clear from its value names alone. A future two-option
  choice whose options need context ("A"/"B") would need better option names.
- A parameter frozen while playing still shows "Stop to change" under its name, whatever its
  scale.
