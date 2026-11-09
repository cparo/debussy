# ADR-0065 — Zooms finer than the waveform summaries are drawn as a time-in-level trace, normalised per column through a square root

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-08
- **Issue:** —
- **Architecture:** [Waveform summaries](../architecture/analysis.md#waveform-summaries)

## Context

At zooms finer than the summaries' 64-frame bucket, lanes drew a min/max/RMS bar per column from
the stored audio, each column taking in the next column's first sample
([ADR-0062](0062-zooms-finer-than-the-waveform-summaries-are-drawn.md)). At 2 to 8 frames per
column those bars are thick blocks rather than a line, and at 32 to 63 they hide where in its
range the signal spends its time: a sine and a noise burst of the same peak look alike.

## Decision

Below 64 frames per column, lanes are drawn as a time-in-level trace. The audio is taken as
straight lines between samples; each column's segments (from its first sample to the next
column's first) share the column's time equally, each spread over the pixel rows its value
range crosses in proportion to its overlap. Each pixel's share, divided by the column's largest
and passed through a square root, is the opacity of the audio colour. Runs of frames where no
clip plays are left out, and gaps stay blank. The patent evidence is in
[ADR-0064](0064-the-intensity-graded-trace-may-be-built-in-house.md).

## Alternatives considered

- **Linear opacity (phosphor-like)** — fast swings across many rows leave each row a small share
  and fade nearly out next to a flat stretch in the same column.
- **A log scale with a floor** — brings up the faint rows but needs a tuned floor, and flattens
  the difference between dense and sparse levels.
- **A dot per sample** — at 2 frames per column the dots leave gaps across fast swings, and do
  not show density at 32 to 63.
- **Keep the min/max bars** — they cannot show where in its range the signal sits.
- **Normalise across the whole lane** — a column with one flat stretch would set the scale and
  dim every column with swings.

## Consequences

Coarser zooms keep the summaries' bars, so the view changes character between 63 and 64 frames
per column. Brightness is relative within a column, so it does not compare level density between
columns. Each redraw at the fine zooms computes up to 64 histograms of the lane's height per
column, from the samples the lanes already hold.
