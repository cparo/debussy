# Patent status of xorshift, xorshift64* and SplitMix64

Research note, 2026-11-06. This is not legal advice. It records what primary sources show, so that
a reviewer can check it.

## Question

Are there unexpired patents, or patent claims that are not royalty-free, on the three PRNG
techniques planned as the TPDF dither noise source?

1. Marsaglia's xorshift step `x ^= x >> 12; x ^= x << 25; x ^= x >> 27`.
2. Vigna's xorshift64\*: that step, with the output multiplied by `0x2545F4914F6CDD1D`.
3. SplitMix64: state `+= 0x9E3779B97F4A7C15`, then Stafford's Mix13 (shift 30, multiply by
   `0xBF58476D1CE4E5B9`, shift 27, multiply by `0x94D049BB133111EB`, shift 31).

## Summary

| Technique | Verdict | Confidence |
|---|---|---|
| xorshift (12/25/27) | No patent found. Published July 2003, so any valid patent on it would have had to be filed by about July 2004, and a patent filed then would have expired by about 2024. | High |
| xorshift64\* | No patent found. The multiplier is L'Ecuyer's 1999 MCG constant. Vigna says the xorshift-then-multiply idea is "noted in passing" by Marsaglia (2003) and was proposed in *Numerical Recipes* 3rd ed. (2007). The only patents that mention the constant claim multi-generator systems. | Medium-high |
| SplitMix64 | No granted claim found that covers a single plain SplitMix64. Mix13 was published in 2011. Oracle has two granted patents in this area, but their claims need several generators. One Oracle application (14/043,372) describes SplitMix; its published claims need a thread "pedigree" dot-product, and Google Patents lists it as **abandoned** (checked by Chris Paro, 2026-11-06). | Medium-high |

**What would block calling these unencumbered:**

- ~~The status of US application 14/043,372 (US 2015/0095389 A1)~~: abandoned, per Google Patents
  (checked by Chris Paro, 2026-11-06). No continuation was found, but none was searched for
  directly.
- The searches were cut short. Google Patents began refusing automated requests partway through,
  and Espacenet, the EPO Register, Patentscope and Justia all returned 403.

## Findings

### 1. xorshift (Marsaglia)

- G. Marsaglia, "Xorshift RNGs", *Journal of Statistical Software* 8(14), published 2003-07-04.
  The article is licensed CC BY 3.0. JSS licenses the code it publishes under GPL-2 or GPL-3. That
  is a copyright licence on the code text, not on the algorithm, so implementing the step from its
  description does not copy that code. <https://www.jstatsoft.org/article/view/v008i14>
- Vigna's paper (below, Fig. 10) uses the same 12/25/27 triple and gives xorshift64\* in C99.
  <https://arxiv.org/abs/1402.6246>
- Google Patents found no patents with Marsaglia as inventor (search log Q8). Text searches for
  "xorshift" return 184 hits. The top hits are unrelated systems that only use xorshift inside, plus
  two specific designs: Red Hat US 10,474,432 B2 and Intel US 2025/0211435 A1 (see the search log).
  Neither claims a plain software xorshift.

### 2. xorshift64\* (Vigna)

- S. Vigna, "An experimental exploration of Marsaglia's xorshift generators, scrambled". The arXiv
  record shows v1 submitted 2014-01-22 and v5 on 2016-10-13. The paper itself says it extends the
  version published in *ACM Trans. Math. Software* 42(4), 2016, Article 30.
  <https://arxiv.org/abs/1402.6246v5>
- The paper's Table IV lists `M32 = 2685821657736338717`, which equals `0x2545F4914F6CDD1D`
  (checked by computation). Fig. 10, "The suggested xorshift64\* generator", is exactly 12/25/27
  followed by `x * 2685821657736338717`. <https://arxiv.org/pdf/1402.6246v5>
- Prior art cited in the same paper:
  - Multiplying xorshift output by an odd constant "was noted in passing in Marsaglia's paper".
  - It "is also proposed in a more systematic way in the third edition of Numerical Recipes" (Press
    et al., 2007).
  - The multiplier comes from L'Ecuyer, "Tables of linear congruential generators...", *Math.
    Comput.* 68(225), 1999.
  - These citations are taken from Vigna's paper; *Numerical Recipes* was not checked directly.
- Vigna's site has no `xorshift64star.c`, and its xorshift page links only to xorshift128+ and
  xorshift1024\*. Its reference files carry a public-domain dedication; see, for example, the
  header of xorshift1024star.c (2017): "dedicated all copyright and related and neighboring rights
  to this software to the public domain". The site pages themselves contain no statement about
  patents.
  <https://prng.di.unimi.it/xorshift.php>, <https://prng.di.unimi.it/xorshift1024star.c>
- Google Patents lists one patent with Vigna as inventor: Yahoo! US 2010/0161643 A1, about query
  segmentation, which is unrelated. A search for the decimal multiplier returns only the two Oracle
  families below, which cite it as L'Ecuyer's prior art.

### 3. SplitMix64 and its lineage

- **MurmurHash3 fmix64** (Appleby). The source header reads: "MurmurHash3 was written by Austin
  Appleby, and is placed in the public domain. The author hereby disclaims copyright to this source
  code." <https://raw.githubusercontent.com/aappleby/smhasher/master/src/MurmurHash3.cpp>
- **Mix13** (Stafford). Published 2011-09-28. The post gives Mix13 as 30 / `0xbf58476d1ce4e5b9`
  / 27 / `0x94d049bb133111eb` / 31. It says nothing about licences or patents.
  <https://zimbry.blogspot.com/2011/09/better-bit-mixing-improving-on.html>
- **SplitMix paper.** G. L. Steele Jr. (Oracle Labs), D. Lea (SUNY Oswego) and C. H. Flood (Red
  Hat), "Fast Splittable Pseudorandom Number Generators", OOPSLA '14, October 20–24, 2014, Portland,
  DOI 10.1145/2660193.2660195. The paper says "mix64variant13 is David Stafford's Mix13 variant of
  the MurmurHash3 finalizer". The word "patent" does not appear in it.
  <https://gee.cs.oswego.edu/dl/papers/oopsla14.pdf>
- **Vigna's splitmix64.c** (2015). It uses the same constants and the shifts 30/27/31. It is
  dedicated to the public domain "to the extent possible under law".
  <https://prng.di.unimi.it/splitmix64.c>
- **OpenJDK `SplittableRandom`.** The header reads "Copyright (c) 2013, 2024, Oracle", and the file
  is licensed GPL-2.0-only with the Classpath exception. Its `mix64` is documented as "Stafford
  variant 13" and uses `GOLDEN_GAMMA = 0x9e3779b97f4a7c15L`.
  <https://raw.githubusercontent.com/openjdk/jdk/master/src/java.base/share/classes/java/util/SplittableRandom.java>
  OpenJDK's GPLv2+CE page contains no explicit patent grant from Oracle. Its only patent wording
  comes from the GPLv2 text itself, which has no express patent licence.
  <https://openjdk.org/legal/gplv2+ce.html>
  A GPL'd Java file therefore gives a Rust reimplementation no patent comfort.
- **Oracle US application 14/043,372**, published as US 2015/0095389 A1 (Steele; filed
  2013-10-01; published 2015-04-02). The specification contains SplittableRandom-style code
  (`gamma`, `mix64`, `mix32`) and cites MurmurHash3 and Stafford. Its published claim 1 requires
  all of the following:
  - a dot-product between a thread "pedigree" and an array of coefficients,
  - updated by adding the coefficient for the thread's level,
  - then mixed.

  Claim 4 adds the MurmurHash3 finaliser as the mixer, but only as a dependent claim. A single
  sequential SplitMix64, with no pedigree and no thread hierarchy, does not read on these published
  claims. Granted claims can differ from published ones, though.
  <https://image-ppubs.uspto.gov/dirsearch-public/print/downloadPdf/20150095389>

  **Status: abandoned**, per the Google Patents record for US20150095389A1 (checked by Chris
  Paro, 2026-11-06). The front page (USPTO PDF, read 2026-11-06) confirms Appl. No. 14/043,372,
  filed Oct. 1, 2013, inventor Guy L. Steele, assignee Oracle. Two earlier indirect signs agree:
  - US 10,922,052 (spec written in 2015) and US 11,392,350 (granted in 2022) both refer to it only
    as "application Ser. No. 14/043,372".
  - The 11,392,350 front page cites it as "2015/0095389 A1", not by a patent number.
- **US 10,922,052 B2** (Oracle; Steele). This is the "TwoLCG" patent: "Generating pseudorandom
  number sequences by nonlinear mixing of multiple subsidiary PRNGs".
  - Filed 2015-10-12 (14/880,798), granted 2021-02-16, with 108 days of patent term adjustment.
    It would expire around 2036-01 if fees are paid; this is my estimate.
  - Claim 1 requires two or more subsidiary PRNGs with independent state spaces, combined
    non-linearly and asymmetrically, across two process instances.
  - It mentions SplitMix and `2685821657736338717` (as L'Ecuyer's MCG multiplier) only as
    background.

  <https://image-ppubs.uspto.gov/dirsearch-public/print/downloadPdf/10922052>
  The EP counterpart is EP 3 362 887 B1 (application 16767421.7), granted 2019-07-24, and its
  claim 1 also needs two or more subsidiary generators.
  <https://data.epo.org/publication-server/rest/v1.2/patents/EP3362887NWB1/document.html>
  Other members of the family: WO 2017/065894, CN 108139889 A.
- **US 11,392,350 B2** (Oracle; Steele). This is the "brined initial states" patent, the LXM line.
  - Provisional application filed 2019-06-18; the full application was filed 2019-08-22; granted
    2022-07-19, with 330 days of term adjustment. It would expire around 2040 if fees are paid; this
    is my estimate.
  - Claim 1 requires a salt, a collection of PRNGs each created from a "brine" (part of the salt
    plus a unique index), and two or more of them running in parallel on separate threads.

  <https://image-ppubs.uspto.gov/dirsearch-public/print/downloadPdf/11392350>
  The EP counterpart is EP 3 987 389 B1, granted 2024-01-24.
  <https://data.epo.org/publication-server/rest/v1.2/patents/EP3987389NWB1/document.html>
  Other members of the family: WO 2020/256851, CN 114008585 A.
- No patents relevant to random number generation were found for Lea, Flood, Appleby or Stafford.
  Flood's patents are on garbage collection and work-stealing.

## Patent search log (2026-11-06)

**Google Patents.** All queries went through the `patents.google.com/xhr/query` endpoint, and only
the first page of results (up to 10) was reviewed for each. After these queries Google began
returning 503 ("automated queries"), and the search was not continued there.

| # | Query | Hits | Relevant |
|---|---|---|---|
| Q1 | `inventor=Guy L Steele & assignee=Oracle & q=pseudorandom` | 12 | US 2015/0095389, US 10,922,052, US 11,392,350, WO 2017/065894 |
| Q2 | `inventor=Guy L Steele & q=random` | 27 | Same families; the rest are hashing and LDA patents |
| Q3–Q10 | `inventor=` Doug Lea / Douglas Lea / Christine H Flood (each `q=random`); Christine Flood; Sebastiano Vigna; George Marsaglia; Austin Appleby; David Stafford (`q=hash`) | 0 / 1 / 7 / 17 / 1 / 0 / 2 / 2 | None relevant (Flood: GC; Vigna: Yahoo query segmentation; Appleby: 1925 grease gun) |
| Q11 | `q="xorshift"` | 184 | US 10,474,432 B2 (Red Hat, filed 2017-11-02; claim 1 is several PRNGs with seeds sent by a randomisation engine); US 2025/0211435 A1 (Intel, filed 2023-12-26; claim 1 is an FHE accelerator with a configurable RNG); the rest are applications that use xorshift |
| Q12 | `q="xorshift64"` | 6 | None claim the generator; JP 2022-096222 A (Fujitsu) was not read |
| Q13 | `q="splitmix"` | 9 | Oracle families; WO 2021/138716 (Mesinja), EP 4295224 (Cassy) not read |
| Q14 | `q="splitmix64"` | 2 | None (Intel bit permutation, a CN error-concealment method) |
| Q15 | `q="SplittableRandom"` | 2 | EP 3362887 B1, US 2020/0401378 |
| Q16 | `q="MurmurHash3" & q=finalizer` | 4 | US 2015/0095389, EP 3987389 |
| Q17–Q19 | each SplitMix constant in hex | 5 / 2 / 2 | Only post-2025 CN filings that use the constants inside other systems |
| Q20 | `q=0x2545F4914F6CDD1D` | 0 | — |
| Q21 | `q=2685821657736338717` | 2 | EP 3362887 A1, CN 114008585 A (Oracle) |
| Q22–Q24 | `splittable` + `pseudorandom` / `random number`, assignee Oracle or Sun | 2 / 2 / 2 | Oracle families only |

**Other sources:**

- Full patent texts came from the USPTO (`image-ppubs.uspto.gov/.../downloadPdf/<number>`). They
  are scanned images and were read by OCR.
- EP bibliographic data and claims came from the EPO publication server, `data.epo.org`.
- **Refused or failed:**
  - Espacenet, the EPO Register, WIPO Patentscope, Justia and uspto.report returned 403.
  - The USPTO Open Data Portal API returned 401 (it needs an API key).
  - ACM DL returned 403.
  - dblp is behind a bot wall.

## Open uncertainties

1. **US 14/043,372 is abandoned** (Google Patents, checked by Chris Paro, 2026-11-06), so it
   cannot issue. Any continuation would have had to be filed while it was pending; none was found,
   though none was searched for directly in USPTO Patent Center.
2. **Coverage of the search.**
   - Only the first page of results was reviewed for each query.
   - Full-text hits on "xorshift" (184) were not all read.
   - Patents that describe the method without using these names would be missed, for example "shift
     register XOR multiply generator".
   - A professional freedom-to-operate search would cover CPC G06F 7/58x by date range.
3. **The legal status of the granted Oracle patents** (fees paid, EP lapses) was not checked; their
   claims do not reach the planned design either way.
4. **The *Numerical Recipes* 3rd ed. (2007) prior art** is taken from Vigna's paper and was not
   read directly.
5. **The expiry argument for xorshift** assumes a normal 20-year term. Term adjustment of a few years
   would not change the conclusion for a patent filed by 2004.
