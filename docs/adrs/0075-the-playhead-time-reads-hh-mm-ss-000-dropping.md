# ADR-0075 — The playhead time reads HH:MM:SS.000, dropping leading units a short project does not need

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-09
- **Issue:** —
- **Architecture:** [Transport and seeking](../architecture/user-interface.md#transport-and-seeking)


## Context

The bottom bar showed the playhead as seconds with two decimals, left of the position bar. Long
projects were hard to read in seconds, and the developer asked for a clock format and for the time
to sit after the bar.

## Decision

- The time sits right of the position bar, at a fixed width.
- It reads `HH:MM:SS.000`. While the later of the project's end and the playhead is under an
  hour it reads `MM:SS.000`, and under a minute `SS.000`. Milliseconds are truncated, not
  rounded, so the time never shows a frame not yet reached.

## Alternatives considered

- **Always `HH:MM:SS.000`** — leading zeros take space and attention on the short projects that
  are the common case.
- **Choosing the format by the project's end alone** — playing past the end of a short project
  would show 75 seconds as `75.000`; including the playhead keeps every field in range.
- **Seconds with decimals, as before** — unreadable past a few minutes.

## Consequences

- The format can change while playing, when the playhead passes a minute or an hour beyond a
  shorter project's end.
- Over 99 hours the hours field widens rather than wraps.
