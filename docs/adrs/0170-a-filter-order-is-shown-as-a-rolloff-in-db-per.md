# ADR-0170 — A filter order is shown as a rolloff in dB per octave

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-26
- **Issue:** #206
- **Architecture:** [Node dialogs](../architecture/node-dialogs.md#node-dialogs)

## Context

The Equalizer's high-pass and low-pass orders were whole-number sliders labelled **Order**, 1 to 8
poles, and the synth's new low-pass order (ADR-0168) a pick list of `6 dB` to `24 dB`. Chris Paro
asked on 2026-11-26 for both to be shown alike: as **Rolloff**, a slider, with the value in
dB/octave. The Equalizer's parameter names and values are in saved projects; renaming them, or
saving dB instead of poles, would change the saved format.

## Decision

- `ParameterInfo` gains two display-only fields, not saved: `label`, shown in place of the label
  made from the name, and `display_factor`, which the shown value (with its unit) is multiplied
  by. The value held, saved and read by the node is unchanged.
- Every filter order (the Equalizer's `high-pass order` and `low-pass order`, the synth's
  `filter order`) is a whole number of poles, labelled **Rolloff**, with unit `dB/octave` and
  display factor `dsp::DB_PER_POLE` (6): a slider whose tooltip shows 6 dB/octave per pole.
- A UI test keeps every filter order's label, form and shown value the same.

## Alternatives considered

- **Rename the parameters to `… rolloff` and save dB.** The label would follow from the name, but
  the Equalizer's saved projects would need a migration; rejected for a display change.
- **Keep the synth's pick list of named slopes.** Differs from the Equalizer, which was the
  complaint.
- **Share the label strings from `audio_effects`.** `instruments` does not depend on it, and adding
  a dependency for two strings is not worth a dependency decision; the UI test holds them together.

## Consequences

- The synth's `filter order` is saved as poles (1 to 4), not an option index; it had not shipped.
- Other parameters may now declare a label or a display factor; status messages still name the
  parameter by its saved name.
