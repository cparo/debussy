# ADR-0035 — The dither noise source is xorshift64* seeded by SplitMix64

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-06
- **Issue:** #29
- **Architecture:** [Patent policy](../architecture/patents.md#patent-policy)

## Context

The `dsp` crate's TPDF dither (ADR-0025) needs a fast, seeded, allocation-free noise source.
`docs/architecture/patents.md` "Patent policy" allows an in-house algorithm only with evidence,
recorded in an ADR, that no unexpired, non-royalty-free patent covers it. The generator chosen
was xorshift64* seeded by one SplitMix64 step. It was researched against primary sources
([research note](../research/prng-patents.md)) and the open question was put to the developer.

## Decision

The dither noise source is xorshift64* (shifts 12/25/27, multiplier `0x2545F4914F6CDD1D`), with
its state derived from the seed by one SplitMix64 step (add `0x9E3779B97F4A7C15`, then the
Mix13 finaliser). The evidence:

- **xorshift:** Marsaglia, "Xorshift RNGs", *Journal of Statistical Software* 8(14), published
  2003-07-04. No patent naming Marsaglia was found, and any patent would have had to be filed by
  2004, so it would have expired by about 2024.
- **xorshift64\*:** Vigna, ACM TOMS 42(4), 2016 (arXiv 1402.6246, 2014). The multiplier is
  L'Ecuyer's 1999 constant. The only patents found that contain it (Oracle, US 10,922,052 and
  US 11,392,350) cite it as prior art and claim systems of several generators.
- **SplitMix64:** Steele, Lea and Flood, OOPSLA 2014, and Mix13 (Stafford, 2011). Oracle's US
  application 14/043,372 (US 2015/0095389 A1, filed 2013-10-01) describes the generator, but
  its claims need a per-thread pedigree dot-product, and Google Patents lists it as abandoned
  (checked by Chris Paro, 2026-11-06). Oracle's granted patents in this area claim several
  generators combined, or salted parallel generators.

## Alternatives considered

- **Seed with Mix13 alone, without the SplitMix step** — would have sidestepped 14/043,372 while
  its status was unknown. Unnecessary once it was found to be abandoned.
- **A crate such as `rand`** — real-time core crates depend on no third-party crate other than
  `thiserror` (`docs/architecture/crates.md` "Dependency rules").

## Consequences

- The search was not a professional freedom-to-operate search. Several patent databases refused
  automated queries, and only the first page of results per query was read; the research note
  lists the gaps. New evidence that a patent covers the generator would need a new record.
- The generator is for dither noise only. It is not suitable for cryptography or anything
  security-sensitive.
