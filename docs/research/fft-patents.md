# Patent status of radix-2 FFT, packed real FFT, and Hann-windowed STFT

Research note, 2026-11-06. This is not legal advice. It records what primary sources show, so that
a reviewer can check it.

## Question

Are there unexpired patents, or patent claims that are not royalty-free, on the FFT techniques
planned for spectrogram data and future effects (f32, power-of-two sizes 64 to 16384, plans
precomputed so a transform does not allocate)?

1. An iterative radix-2 decimation-in-time Cooley–Tukey FFT: bit-reversal permutation, then
   butterflies with precomputed twiddle factors.
2. A real-input FFT by packing: an N-point real transform as an N/2-point complex FFT plus a
   twiddled "split" post-processing step.
3. A Hann window and overlapping frames (short-time Fourier transform).

Also: precomputed twiddle tables, table-driven in-place bit reversal, and split-radix (not
planned).

## Summary

| Technique | Verdict | Confidence |
|---|---|---|
| Radix-2 DIT Cooley–Tukey | No covering patent found. Published 1965; any patent on the plain method would have expired decades ago. In-force patents found on FFTs claim hardware, GPUs, multi-bank memory or special instructions. | High |
| Packed real FFT (N/2 complex + split) | No covering patent found for a plain CPU implementation. A real-input FFT was published (Bergland, 1968) and patented (Edson, Bell Labs, US 3,584,781, issued 1971, long expired). Two in-force US patents claim packing *on a GPU* (Microsoft) or *via a dedicated SIMD conversion instruction* (Broadcom); neither claim set reads on portable scalar code, but see the Broadcom note. | Medium-high |
| Hann window + overlapping STFT frames | No covering patent found. Hann ("hanning") is in Blackman & Tukey (1958); the STFT framework is in Allen (1977) and Allen & Rabiner (1977). Patents found use a spectrogram inside a larger claimed method. | High |
| Precomputed twiddle table | No claim found on a plain table. Patents found claim twiddle-generation *circuitry* or table compression (Intel's is expired for unpaid fees). | Medium-high |
| Table-driven bit reversal | No claim found on a plain permutation. The Nvidia patent found needs a multi-bank memory written in parallel. | Medium-high |
| Split-radix (not planned) | Algorithm published 1968 (Yavne) and 1984 (Duhamel & Hollmann). Patents found (TI, Novatek) claim hardware datapaths. | Medium |

**What would block calling the plan unencumbered:**

- **Broadcom US 7,676,533 B2** (real/complex FFT conversion by SIMD instruction) is probably in
  force until about 2027-05-14 if fees were paid; its legal status was not checked. Its claims
  need a processor executing a dedicated "FFT conversion instruction" with forward/inverse
  selected by a control bit, opcode or flag. A plain Rust loop does not do this on its face, but a
  reviewer should confirm, or the plan can simply note that the patent ends in 2027.
- The searches were cut short: Google Patents returned 503 after eight queries, Espacenet
  returned 403, and US pre-grant publications and non-US patents were not searched in USPTO PPUBS.

## Findings

### 1. Radix-2 Cooley–Tukey FFT

- J. W. Cooley and J. W. Tukey, "An algorithm for the machine calculation of complex Fourier
  series", *Math. Comp.* 19(90), 297–301, 1965, DOI 10.1090/S0025-5718-1965-0178586-1 (Crossref
  metadata; the AMS page returned 403). <https://doi.org/10.1090/S0025-5718-1965-0178586-1>
- Gauss's earlier work is the subject of M. T. Heideman, D. H. Johnson and C. S. Burrus, "Gauss
  and the history of the fast Fourier transform", *IEEE ASSP Magazine* 1(4), 14–21, October 1984
  (metadata only, not read). <https://doi.org/10.1109/MASSP.1984.1162257>
- Cooley's own accounts of the 1965 paper's reception, which other writers cite for IBM's decision
  to leave the algorithm unpatented, were not read: "How the FFT gained acceptance" (ACM HSNC
  1987, <https://doi.org/10.1145/41579.41589>) and "The re-discovery of the fast Fourier transform
  algorithm", *Mikrochimica Acta* 93, 33–45, 1987 (<https://doi.org/10.1007/bf01201681>). The
  verdict rests on the 1965 publication date, not on that account.
- In-force FFT patents from the searches claim specific machines, not the radix-2 method:
  - IBM US 9,582,474 B2 (filed 2014-06-24): butterflies performed "simultaneously" from two memory
    areas, in place in a first phase and to different locations in a second phase. The planned
    FFT is in place throughout. <https://ppubs.uspto.gov/pubwebapp/> (search log U4)
  - Nvidia US 11,568,523 B1: a multi-threaded processor with listed hardware units. Apple
    US 12,530,423 B1: a circuit of single-delay-feedback radix chains. Qualcomm US 8,787,422 B2:
    instructions using shuffle/deal I/O networks. All hardware or ISA claims.
    <https://patents.google.com/patent/US11568523B1/en>,
    <https://patents.google.com/patent/US12530423B1/en>

### 2. Real-input FFT by packing

- J. W. Cooley, P. A. W. Lewis and P. D. Welch, "The fast Fourier transform algorithm:
  Programming considerations in the calculation of sine, cosine and Laplace transforms", *J. Sound
  Vib.* 12(3), 315–337, July 1970 (metadata confirmed; full text not read).
  <https://doi.org/10.1016/0022-460X(70)90075-1>
- G. D. Bergland, "A fast Fourier transform algorithm for real-valued series", *Commun. ACM*
  11(10), 703–710, October 1968. Its abstract describes a power-of-two procedure that achieves "the
  two-to-one reduction in computation and storage which can be achieved when the series is real".
  (This is a different real-FFT method from packing, but the same goal.)
  <https://doi.org/10.1145/364096.364118>
- H. V. Sorensen, D. L. Jones, M. T. Heideman and C. S. Burrus, "Real-valued fast Fourier
  transform algorithms", *IEEE Trans. ASSP* 35(6), 849–863, June 1987 (metadata only).
  <https://doi.org/10.1109/TASSP.1987.1165220>
- E. O. Brigham, *The Fast Fourier Transform*, Prentice-Hall, ISBN 013307496X. Open Library gives
  its first publication year as 1973, not 1974; its contents were not checked.
  <https://openlibrary.org/works/OL5091239W>
- **Edson (Bell Labs) US 3,584,781**, "FFT method and apparatus for real valued inputs", filed
  1968-07-01, issued 1971-06-15. Claim 1: Cooley–Tukey iterations computing only one of each
  conjugate pair, then generating the conjugates. Long expired, and itself prior art.
  <https://ppubs.uspto.gov/pubwebapp/> (search log U1)
- **Microsoft US 9,342,486 B2**, "Fast computation of general Fourier transforms on graphics
  processing units", priority 2008-10-03, granted 2016-05-17, 1296 days of term adjustment; Google
  Patents lists it as active, expiring 2032-04-21. Claim 1 packs even and odd elements of a real
  sequence into a complex sequence, transforms, and unpacks, but requires "a programmable graphics
  processing unit" and a length that is "a non-power of two". Claim 12 has the same two
  limitations; claim 15 requires running the packed FFTs "in parallel on the programmable graphics
  processing unit". The plan (CPU, powers of two) meets neither.
  <https://patents.google.com/patent/US9342486B2/en>
- **Broadcom US 7,676,533 B2**, "System for executing SIMD instruction for real/complex FFT
  conversion" (inventor Taunton), filed 2004-09-30 with 2003 provisionals, issued 2010-03-09, 956
  days of term adjustment. My estimate of expiry, if fees are paid: 2027-05-14. Every independent
  claim (1, 7, 13, 16) requires "executing one or more instances of an FFT conversion
  instruction", with forward or inverse selected by a "control message" (claims 1, 7, as
  means-plus-function) or a "control register that includes a control bit" (13, 16). The
  specification says the instruction uses a SIMD technique and describes it on Broadcom's
  FirePath processor. <https://ppubs.uspto.gov/pubwebapp/> (search log U12)
- **Astel KR 10-2376492 B1**, "Fast Fourier transform device and method using real valued as
  input", priority 2020-08-05, granted 2022-03-21, active. Claims 1–2 are deleted; claim 3 is an
  apparatus of data and coefficient memories, a butterfly unit, a counter, address generators and
  multiplexers doing the packing post-processing in two stages. A hardware claim, enforceable only
  in Korea. <https://patents.google.com/patent/KR102376492B1/ko>
- Other packing hits are tied to a field: Ericsson US 9,699,013 B2 (a TDD wireline transceiver
  combining two blocks into one complex FFT) and CGG US 9,348,044 B2 (seismic simulation).

### 3. Hann window and STFT

- R. B. Blackman and J. W. Tukey, "The measurement of power spectra from the point of view of
  communications engineering", *Bell Syst. Tech. J.* 37(1), 185–282, January 1958, and 37(2),
  485–569, March 1958. Part I gives the lag window "whose use is called 'hanning'",
  ½(1 + cos πτ/T_m), and its glossary defines hanning as smoothing with weights 0.25, 0.50, 0.25
  "(After Julius von Hann.)". Part II calls it "after the Austrian meteorologist Julius von Hann".
  <https://doi.org/10.1002/j.1538-7305.1958.tb03874.x>, <https://archive.org/details/bstj37-1-185>,
  <https://archive.org/details/bstj37-2-485>
- J. B. Allen, "Short term spectral analysis, synthesis, and modification by discrete Fourier
  transform", *IEEE Trans. ASSP* 25(3), 235–238, June 1977.
  <https://doi.org/10.1109/TASSP.1977.1162950>
- J. B. Allen and L. R. Rabiner, "A unified approach to short-time Fourier analysis and
  synthesis", *Proc. IEEE* 65(11), 1558–1564, 1977. <https://doi.org/10.1109/PROC.1977.10770>
- F. J. Harris, "On the use of windows for harmonic analysis with the discrete Fourier
  transform", *Proc. IEEE* 66(1), 51–83, 1978. <https://doi.org/10.1109/PROC.1978.10837>
- (Allen, Rabiner and Harris: metadata confirmed via Crossref; full texts not read.)
- Spectrogram patents found claim a larger method that uses one: for example, Google US 9,263,061
  B2 (detecting chopped speech) and US 9,948,173 B1 (power-converter control). None claims
  windowed, overlapped framing itself.

### 4. Twiddle tables, bit reversal, split-radix

- **Twiddle tables.** Intel US 9,317,480 B2 claims circuitry that reconstructs twiddles from a
  smaller "intermediate factors vector"; Google Patents lists it "Expired - Fee Related".
  <https://patents.google.com/patent/US9317480B2/en> Freescale US 8,386,552 B2 claims a twiddle
  factor unit's look-up table for a mixed-radix DFT whose maximum size has a non-power-of-two
  radix, with a precoded uplink signal. No claim to a plain precomputed table was found.
- **Bit reversal.** Nvidia US 7,640,284 B1 (priority 2006-06-15, 785 days of adjustment, active to
  2028-08-08 per Google Patents) claims bit-reversing indices, then modifying them so that writes
  land in different banks of a multi-bank memory, written in parallel. A single-threaded table
  permutation has no banks. <https://patents.google.com/patent/US7640284B1/en>
- **Split-radix.** R. Yavne, "An economical method for calculating the discrete Fourier
  transform", AFIPS Fall Joint Computer Conference, December 1968, 115–125
  (<https://doi.org/10.1145/1476589.1476610>). P. Duhamel and H. Hollmann, "'Split radix' FFT
  algorithm", *Electronics Letters* 20(1), 14–16, 1984-01-05
  (<https://doi.org/10.1049/el:19840012>).
  Duhamel's only US FFT-titled patent found is US 4,831,574 (filed 1987, a DCT device), expired.
  In-force split-radix patents claim hardware: TI US 8,819,097 B2 (summing-circuit datapaths) and
  Novatek US 8,601,045 B2 (multi-bank memory and an SRFFT processor).

## Patent search log (2026-11-06)

**Google Patents** (`patents.google.com/xhr/query`; first page of up to 10 reviewed; all filtered to
granted, priority after 2006-01-01). After G8 every query and claim page returned 503.

| # | Query | Hits | Relevant |
|---|---|---|---|
| G1 | `"real-valued" FFT "complex FFT" "half length"` | 0 | — |
| G2 | `"twiddle factor" "lookup table" FFT software` | 53 | Hardware (TI US 8,549,059, Qualcomm EP 2013772); none on a plain table |
| G3 | `"split-radix" FFT` | 94 | First page unrelated (codecs, music teaching) |
| G4 | `"real-valued" "fast Fourier transform" complex half`, CPC G06F17/142 | 18 | US 9,342,486 (Microsoft), KR 10-2376492 |
| G5 | `real FFT packing "real input"`, CPC G06F17/142 | 2 | KR 10-2376492, US 11,568,523 |
| G6 | `"bit reversal" "lookup table" software`, CPC G06F17/142 | 6 | US 7,640,284, US 7,836,116 (Nvidia, not read), US 12,530,423 |
| G7 | `"precomputed twiddle"` | 2 | US 9,317,480 (expired) |
| G8 | `spectrogram "Hann window" STFT overlapping frames`, CPC G06F17/142 | 0 | — |

**USPTO PPUBS** (`ppubs.uspto.gov` search API, granted US patents only, BRS syntax; hits filed
before 2006 noted but not reviewed unless named).

| # | Query | Hits (filed ≥ 2006) | Relevant |
|---|---|---|---|
| U1 | `(real ADJ valued).ti. AND (fourier OR FFT).ti.` | 1 (0) | US 3,584,781 (Edson, 1968) |
| U2 | real valued/input/data `.clm.`, FFT `.ti.`, N/2 or half length `.clm.` | 5 (0) | — |
| U3 | FFT, real valued, complex, pack/combin, even odd, all `.clm.` | 2 (2) | US 9,342,486; US 9,699,013 |
| U4 | twiddle factor + precomput/lookup table `.clm.`, FFT `.ti.` | 12 (9) | US 8,386,552; US 9,582,474; rest hardware |
| U5 | `(split ADJ radix OR split-radix).clm.` | 7 (8 returned) | US 8,819,097; US 8,601,045 |
| U6 | bit reversal + table `.clm.`, FFT `.ti.` | 4 (1) | US 12,045,582 (TI pipeline hardware) |
| U7 | real, FFT, N/2 or half, complex, twiddle/post-processing/split, all `.clm.` | 6 (2) | US 11,630,880 (XSail circuit); US 9,348,044 |
| U8 | hann/hanning + spectrogram + overlap `.clm.` | 3 (1) | US 9,263,061 |
| U9 | `spectrogram.ti. AND (FFT OR fourier).clm.` | 20 (18) | Applications of spectrograms only |
| U10 | radix-2 + bit reversal + twiddle `.clm.` | 6 (6) | US 8,787,422; US 8,601,045; others hardware |
| U11 | `duhamel.in.` / `hollmann.in.` with FFT titles; split radix `.ti.` | 2 / 0 / 5 | US 4,831,574 (expired) |
| U12 | `(fourier OR FFT).ti. AND (real).ti.` | 32 (all dates) | US 7,676,533 (Broadcom); US 12,775,677 (Bosch: an IC matrix multiplier) |

Claims and filing data came from PPUBS document records and Google Patents pages, and were read in
full for each patent named above.

**Refused or failed:** Google Patents 503 after G8; Espacenet 403; the AMS page for Cooley–Tukey
403; PatentsView API unreachable; WIPO Patentscope redirected (302) and was not pursued.

## Open uncertainties

1. **Broadcom US 7,676,533 B2.** Legal status (fees, terminal disclaimer) was not checked, so the
   2027-05-14 date is my estimate. Whether "FFT conversion instruction" could be read to cover
   compiled code, rather than a dedicated SIMD instruction, was judged from the claims and a
   skim of the specification, not the prosecution history.
2. **Coverage.** Only granted US patents (PPUBS) and a few Google Patents queries were searched.
   Pending US applications, EP and other national patents beyond the KR hit, and documents that
   describe these methods without the searched words were not covered. A professional search would
   cover CPC G06F 17/142 by date range.
3. **Papers read only as metadata.** Cooley–Lewis–Welch (1970), Sorensen et al. (1987), Allen
   (1977), Allen & Rabiner (1977), Harris (1978), Heideman et al. (1984) and Brigham were confirmed
   by DOI or catalogue but not read, so the claim that the 1970 paper contains the packing split
   step is not verified here. Bergland (1968, abstract) and Edson (1971, claims) independently
   date real-input FFTs to 1968.
4. **Legal status** of Microsoft US 9,342,486 and Nvidia US 7,640,284 is from Google Patents'
   "active" label and expiry estimate, not the USPTO fee record; their claims do not reach the plan
   either way.
5. **Expiry arguments** assume normal terms: 17 years from grant for pre-1995 US filings, 20 years
   from filing (plus adjustment) after.
