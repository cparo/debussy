# ADR-0113 — The reverb is a fixed-length feedback delay network or Dattorro plate behind a pre-delay, with no size control and a stopped-only algorithm switch

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-13
- **Issue:** #61
- **Architecture:** [Reverb](../architecture/delay-and-reverb.md#reverb)

## Context

Issue #61's second part asks for a reverb node with two algorithms chosen by a parameter: a
feedback delay network ("room") and Dattorro's plate ("plate"). The plan also had a `size`
control scaling every delay length and a crossfade between the engines on a change of algorithm.

Patent evidence is in [`docs/research/reverb-patents.md`](../research/reverb-patents.md). Eleven
candidates were found; Claude checked their status from Google Patents on 2026-11-13 under Chris
Paro's rule that a page with no expired or revoked note is In force: eight active, two pending
applications, one lapsed for fees. Reading the claims of the ten found one that reads and four
unclear:

- US10559295B1 (Abel, to 2038) claim 10 **reads** on `size`, in either algorithm and whether or
  not it may change while playing.
- US8345887B1 (Sony, to 2030) is **unclear** for the room: it turns on whether the outputs are
  taken from the Hadamard matrix's output vector.
- US8351614B2 (ST, to 2030) is **unclear** for the plate: whether a tank half is a feed-forward
  loop in cascade. Dattorro's 1997 paper predates its 2006 priority.
- US10311845B2 (Casio, to 2038) and the pending EP4468748A1 (Harman) are **unclear** for the
  crossfade between engines.

The other five do not read. Chris Paro decided all four questions through `flag-risk` (tag
`patent`) on 2026-11-13. The search used web search and Google Patents pages, not
patent-database queries, and the claim quotes are from Google's text. Dattorro's paper could not
be read as text in the search; the plate's constants are those widely reproduced from it and were
not checked against the paper.

## Decision

- **No `size`.** Every delay length is fixed per algorithm, scaled only to the sample rate.
- **Room taps the lines, not the matrix.** The outputs are the delay lines' ends before the
  Hadamard matrix, summed with two orthogonal sign patterns.
- **No engine crossfade.** `algorithm` is stopped-only; a change switches engines at once,
  clearing both, and the two never run together. Both engines are allocated at prepare, so the
  switch needs no node replacement.
- **The plate is kept**, accepting the construction risk on US8351614B2 with Dattorro 1997 as
  prior art.
- The room: four allpasses per channel, eight prime-length lines, a fast Walsh–Hadamard transform
  scaled by 1/√8, a per-line gain from `decay` (Jot and Chaigne's RT60 rule) and a one-pole
  damping low-pass per line. The plate: Dattorro's topology and constants, its two tank allpasses
  modulated ±16 frames (at 29 761 Hz) by a 1 Hz quadrature LFO and read by linear interpolation,
  the tank gain set so a trip round the tank falls by `decay`'s rate.
- The pre-delay reuses the delay's crossfading tap. The kind id is `reverb`; its parameter names
  are new saved names under `SavedNodeKind::Effect`, and `format_version` is unchanged.

## Alternatives considered

- **Keep `size` on the expired prior art (US7561699, US6978027B1).** Whether that art makes claim
  10 invalid is a question for counsel; the claim reads as written.
- **Room outputs from the matrix's output vector.** That is what the unclear element turns on;
  tapping the lines sounds the same.
- **A crossfade between engines over the ramp time.** It is the subject of two unclear claims, and
  runs both engines during the fade.
- **Replacing the node on a change of algorithm** (as a latency-setting parameter does). It needs
  the parameter marked latency-setting though latency does not change; allocating both engines,
  about 1.2 MB at 192 kHz, is simpler.
- **Drop the plate, or wait for counsel.** The plate's tank recirculates and its taps sit inside
  its delays, so the claim most likely does not read.

## Consequences

- Room size is set only by the algorithm's fixed lengths; `decay`, `damping` and `pre-delay` shape
  the space. Revisit a size control when US10559295B1 lapses (2038-12-07).
- Measured RT60 at 48 kHz, damping at 20 kHz: within 10 % of `decay` for 0.5, 1 and 3 s in both
  algorithms.
- A change of algorithm drops the tail, which only happens while stopped.
- The algorithms are listed in `docs/architecture/patents.md` "Patent policy". The plate's
  constants should be checked against the paper when a readable copy is to hand.
