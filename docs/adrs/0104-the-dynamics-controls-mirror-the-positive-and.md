# ADR-0104 — The dynamics controls mirror the positive and negative curves

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-12
- **Issue:** #132
- **Architecture:** [Dynamics](../architecture/effects.md#dynamics)

## Context

Reviewing the dynamics effect's dialog (PR #132), Chris Paro found its controls lopsided. The
negative side had its own curve and ceiling but shared the positive side's makeup gain, so the
two halves could not be levelled independently; the asymmetric toggle sat inside the `negative`
group, where it was easy to miss that it governs the whole group; the curve exponent was linear
from 0 to 16, which crowded the useful range around 1 into the slider's left edge; attack and
release were linear, so short times (the ones that matter for a limiter) took a few pixels; and
the transfer plot gave no strong cue for where 0 dBFS lies in and out. The effect was not yet
merged, so no saved project used its parameter names.

## Decision

- **Mirrored sides.** The curve controls are two groups, `positive` and `negative`, each with
  `steepness`, `exponent`, `output gain` and `ceiling`, named with the group as prefix
  (`positive output gain`, `negative exponent`). `makeup` becomes `positive output gain`;
  `negative output gain` is new, −24 to +24 dB, default 0.
- **Output gain before the DC blocker.** Each side's output gain multiplies the sample with its
  curve gain, before the DC blocker, so the blocker removes the offset an asymmetric output gain
  adds; the clip follows as before.
- **Asymmetric is its own group**, `gain function`, ahead of the two sides. The negative
  controls are enabled by it: greyed and unresponsive while it is off.
- **Exponents** on both sides are logarithmic from 1/16 to 16, default 1.
- **Attack and release** are logarithmic from 0.25 ms to their maxima, with 0 at the slider's
  far left (`Scale::LogarithmicOrZero`, ADR-0105). 0 for both still makes the effect a
  waveshaper. The attack and release exponents are unchanged (already logarithmic, 0.1 to 16).
- **0 dB lines.** The transfer plot draws 0 dB in and out (on each half when asymmetric) as
  1-pixel red lines over the grid.
- The parameter names change with no migration: none had been saved.

This partly changes ADR-0099 (the parameter list) and ADR-0102 (the exponent's range and scale);
both stay Accepted for the rest of their decisions.

## Alternatives considered

- **Keep one shared makeup gain** — the halves of an asymmetric curve could then be matched only
  through their ceilings or curve shapes, not levelled.
- **Apply the output gain after the DC blocker** — an asymmetric output gain would then put a
  DC offset into the output.
- **Hide the negative group while symmetric** — the dialog would change shape as the toggle
  flips; greying keeps the layout still and shows what the toggle unlocks.
- **Keep the exponent linear from 0** — exponent 0 (everything to full scale) is not a useful
  setting, and a linear slider gives 1/16 to 1 a sixteenth of its travel.
- **A separate "instantaneous" toggle instead of attack/release 0** — two controls for one state;
  the far-left 0 keeps the waveshaper reachable from the sliders themselves.

## Consequences

- Saved names differ from the ones on the branch before this record; no project was saved with
  them.
- The negative output gain adds one ramped value per side in the real-time path.
- Greyed, toggle-enabled controls are generic (ADR-0105), so other effects can use them.
