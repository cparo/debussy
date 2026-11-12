# ADR-0100 — Weighted-RMS dynamics may be built in-house; no gain is derived from a block's peak

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-12
- **Issue:** —
- **Architecture:** [Patent policy](../architecture/patents.md#patent-policy)

## Context

The patent policy (`docs/architecture/patents.md` "Patent policy") requires evidence before an
algorithm is built in-house. The `patent-research` skill was run on the dynamics effect's planned
design; the full record is `docs/research/dynamics-patents.md`.

- **Stage 1** found 10 candidate patents.
- **Status check:** Chris Paro checked each one on 2026-11-12. Seven were in force: US11394356B1
  (Amazon), US11496099B2 (Mimi), US9979369B2 (Dolby), US9225310B1 (iZotope/Native Instruments),
  US8983092B2 (Conexant/Synaptics), US8315398B2 and US9264836B2 (DTS). US9661438B1 and
  US9667214B2 had lapsed, and EP3588776B1 had been revoked.
- **Stage 2** read the independent claims of the seven in-force patents. Six do not read on the
  design. US11394356B1 is *Unclear* for a weighted-peak detector mode. That mode would compute a
  block maximum, derive one gain per block from it and apply that gain to the block's samples,
  which matches the claim. The question is whether the claim's "threshold" covers the `tanh`
  curve, whose gain tends to ceiling divided by peak at high levels.

## Decision

- The dynamics effect, as described in `docs/architecture/effects.md` "Dynamics", may be built
  in-house. That covers the weighted-RMS window, the block-rate gain with per-sample
  interpolation, the `tanh` transfer curve, asymmetric shaping with a DC blocker, integer-factor
  oversampling and the display.
- No mode derives a gain from a block's peak or maximum. Chris Paro decided this in answer to the
  `patent` flag. The planned weighted-peak detector mode, and with it the `detector` parameter,
  are dropped.
- Two verdicts hold only under conditions, which a future change must respect or re-check:
  - US11496099B2 does not read only while the window is evaluated directly. Reverse-then-forward
    recursive gain passes would need the claims read again.
  - US9979369B2 does not read only while no option substitutes a gain that brings a block's peak
    exactly to the ceiling (a "true peak" mode, for example).

## Alternatives considered

- **Keep weighted-peak mode and rely on prior art.** The Orban (1996), Waves (1997) and
  Plantronics (2001) look-ahead patents, all expired, could support a prior-art position.
  Rejected because it is a legal judgement the project would take on for a mode RMS mostly covers.
- **Keep peak detection away from the gain path** (for metering only, for example). Rejected
  because it adds little for users and would need the claim read again.
- **Defer peak mode until counsel has looked at it.** Rejected as unnecessary for now. Peak mode
  can come back later with a new record.

## Consequences

- Without a peak detector, the hard clip is the only ceiling guarantee, and RMS detection lets
  transients through until they reach it.
- The evidence has gaps, listed in the research note: general web search only, families not
  walked, and some claims relayed in fragments. If the design changes in the areas above, the note
  must be revisited.
