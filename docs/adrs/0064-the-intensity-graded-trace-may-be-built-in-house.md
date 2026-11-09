# ADR-0064 — The intensity-graded trace may be built in-house: no patent in force was found to read on it

- **Status:** Accepted
- **Decided by:** agent, ratified by Chris Paro
- **Date:** 2026-11-08
- **Issue:** —
- **Architecture:** [Patent policy](../architecture/patents.md#patent-policy)

## Context

Lanes at zooms finer than the waveform summaries are to be drawn as an intensity-graded trace
([ADR-0065](0065-zooms-finer-than-the-waveform-summaries-are-drawn.md)), the kind of display
digital-phosphor oscilloscopes made familiar and several oscilloscope makers have patented.
`docs/architecture/patents.md` "Patent policy" requires recorded evidence before building it
in-house. The design was researched against Google Patents
([research note](../research/trace-density-patents.md)): six candidates were found whose terms
could still run, and Chris Paro checked all six as in force on 2026-11-08.

## Decision

The trace is built in-house. The evidence:

- **Linear segments and a per-column histogram weighted by each segment's span (the core):**
  Tektronix US 5,440,676 (filed 1988) and US 5,550,963 (filed 1994), both expired, describe them.
- **In-force candidates, each read literally on its independent claims; none reads:**
  - LeCroy US 7,589,728 (est. end 2027-02-02) needs detecting and filling a gap between adjacent
    columns' histograms, and, in claims 6 and 12, an oscilloscope acquisition board. Chris Paro
    read its description and found the fill applies only where samples are too sparse to join
    adjacent columns, which cannot arise at the zooms the trace draws (2 to 63 frames per
    column, every column closed by the next column's first sample).
  - LeCroy US 8,330,758 needs a pixel state updated against a random threshold, and fading.
  - Rohde & Schwarz US 9,672,794 needs a display memory of compressed counts summed over
    several acquisitions.
  - Tektronix US 10,983,146 needs digital bus values, hit lines and persistence.
  - Tektronix US 7,834,780 needs parallel compression schemes, a peak-detect background under a
    filtered foreground, or averaging over acquisitions.
  - Home Box Office US 10,565,973 needs the audio reshaped by a polynomial of degree three or
    more; the trace's square root acts on the histogram, not the audio.

## Alternatives considered

- **Draw from the min/max bars only until the candidates expire** — the last runs to 2038, and
  none was found to read on the design.
- **A professional freedom-to-operate opinion first** — out of proportion for a display mode
  that can be switched back to the bars if a claim is ever read differently.

## Consequences

- This is a literal claim reading from a web search, not a freedom-to-operate opinion; the
  research note lists its gaps: claim text read from Google Patents pages and not checked
  against the PDFs, and patent families and forward citations not searched.
- The min/max/RMS bars at coarser zooms were not part of this check; the note marks Tektronix
  US 7,834,780 claim 32 as relevant to them.
- If a claim is later read as covering the trace, the fine zooms can fall back to the bars
  ([ADR-0062](0062-zooms-finer-than-the-waveform-summaries-are-drawn.md)).
