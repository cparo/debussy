# ADR-0103 — The dynamics envelope plot has one linear time axis

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-12
- **Issue:** —
- **Architecture:** [Dynamics display](../architecture/effect-displays.md#dynamics-display)

## Context

ADR-0101 gave the release side and the attack side of the envelope plot half its width each, so a
short attack would not vanish beside a long release. Chris Paro found that this hides what the
plot is for. A short attack and a long release should look like a short, steep attack and a long,
gradual release.

## Decision

- **One linear axis.** The envelope plot has a single linear time axis. It runs from the oldest
  sample the window weighs (or the current sample, if that is later) at the left, to the lookahead
  at the right.
- **Proportional sides.** Each side's width is in proportion to its length.
- **Peak sample.** The peak is drawn as a sample of its own, so its corner shows however short the
  attack.
- **Zero span.** A window that spans no time is a single vertical line at the current sample.

## Alternatives considered

- **Half the width per side (ADR-0101).** Rejected because it shows the shapes but not their
  relative durations.
- **A logarithmic time axis.** Rejected because it cannot show the current sample at zero, and it
  distorts the very proportions the plot is meant to show.

## Consequences

- **Thin attacks.** A 1 ms attack beside a 5 s release is a sliver at the right edge. The shape of
  that side can only be read when the two lengths are comparable.
- **Still in force.** The rest of ADR-0101 (the layout, the transfer curve plot and the shared
  drawing code) is unchanged.
