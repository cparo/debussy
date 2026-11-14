# Patent status of the in-house pink-noise generator (pole-zero pinking filter)

Research note, 2026-11-13. This is not legal advice. It records what primary sources show, so a
reviewer can check it. Stage 1 listed the candidates; a human filled in **Status**; stage 2
(2026-11-13) read the claims of the three in-force candidates, rows 1 to 3. For issue #149.

## Planned design

Software, portable scalar Rust, in a library crate of an audio editor. The generator feeds an
offline analysis (a display of an effect's gain over time), not the real-time audio path.

1. **White source.** The existing seeded xorshift64\* generator (cleared in
   [prng-patents.md](prng-patents.md) / ADR-0035), uniform values in `[-0.5, 0.5)`.
2. **Pinking filter.** The white samples pass through a cascade of first-order IIR sections, each
   `H_k(z) = (1 - q_k z^-1) / (1 - p_k z^-1)` with one real pole `p_k` and one real zero `q_k`
   inside the unit circle. Poles and zeros alternate along the frequency axis: pole frequencies
   are log-spaced from about 5 Hz up to the Nyquist frequency, one per octave (settled
   2026-11-13, after stage 2; no verdict depends on it), and each zero sits half an octave above
   its pole, at the geometric mid-point between two poles, so the magnitude response
   approximates a −3 dB/octave (1/f power) slope. Pole and zero radii are computed from their
   frequencies for the sample rate (`r = exp(-2π f / fs)`). This is the textbook design in
   J. O. Smith, *Spectral Audio Signal Processing*, "Synthesis of 1/f noise" (pole-zero
   alternation after Keshner / Corsini and Saletti).
3. **Normalisation.** A gain computed once for the sample rate so the filter's magnitude at
   1 kHz is the same at every rate.
4. **Seeding.** `new(seed, sample_rate)`: the same seed and rate always give the same sequence.

Not in scope: the Voss-McCartney row-summing algorithm, fractional differencing, FFT shaping of
spectra.

## Summary

Four candidates have a printed term end in the future (rows 1 to 4); a human found rows 1
(US10615764B1), 2 (US10841717B2) and 3 (US8653354B1) in force and row 4 lapsed (see Candidates).
Stage 2 verdicts: row 1 **Does not read**, row 2 **Does not read**, row 3 **Does not read** (see
Claim analysis). None claims a cascade of first-order pole-zero sections for pink or 1/f noise:
row 1 claims summed probabilistically updated stochastic generators, row 2 a loudspeaker test
signal combining broadband noise with random impulses, row 3 a filter chain that turns broadband
noise into pitched musical sound. The alternating real pole-zero pinking filter is described in
dated non-patent sources from 1982 (Keshner), 1988 (Corsini and Saletti, by index summary), 1998
(Bristow-Johnson) and 2016 (Smith and Smith), see "Prior art". This rests on web searches and Google
Patents page fetches only; no patent database query interface was used (see "Refused or failed").
"No candidate found" means the searches in the Search log found none, not that none exists.

| Technique | Verdict | Confidence |
|---|---|---|
| 1. White source (xorshift64\*, uniform) | Out of scope here; cleared in [prng-patents.md](prng-patents.md). | Not assessed |
| 2. Pinking filter: cascade of first-order real pole-zero sections, poles log-spaced, zeros at geometric mid-points | Does not read: row 1 (no stochastic generators updated with probability 1/2^N and summed; no stored values replaced into a running sum), row 2 (no impulsive noise source combined with the broadband noise for a crest factor), row 3 (no slit-spacing or noise-to-band parameters, no pitched musical output). Row 4 not in force. No other candidate found claiming it (searches 1 to 4, 6, 8). Described in non-patent prior art from 1982 to 2016. | Medium (claims read via fetch summaries, see Open uncertainties); Low for unseen patents |
| 3. Normalisation by a fixed gain | No candidate found (searches 1, 2, 7); trivial step, no prior art sought. Rows 1 to 3 do not read on the design as a whole (above). | Low |
| 4. Seeded, deterministic (seed and rate give the same sequence) | No candidate found (searches 1, 5). Row 1's independent claims recite no seeding; it does not read (above). | Low |

## Prior art

Items marked (unverified) come from a search summary or an index, not from the publication itself.

- **Element 2 (pinking filter by alternating real poles and zeros).**
  - M. S. Keshner, "1/f Noise", *Proceedings of the IEEE* 70(3), 212 to 218, March 1982, DOI
    10.1109/PROC.1982.12282 (bibliographic details from a search summary of
    [index records](https://manuscript.isc.ac/inventory/49/965396.htm); a
    [PDF copy](https://mathematicians.korea.ac.kr/ywkim/wp-content/uploads/sites/8/attachments/TopicsGeometry2k7Fall/Keshner_1982_01456550.pdf)
    could not be read by the fetch tool, so whether it describes pole-zero synthesis is not
    established here) (unverified).
  - G. Corsini and R. Saletti, "A 1/f^gamma power spectrum noise sequence generator", *IEEE Trans.
    Instrumentation and Measurement* 37(4), 615 to 619, December 1988, DOI 10.1109/19.9825; indexes
    summarise it as white noise through a digital filter designed by the matched z-transform
    ([index](https://manuscript.isc.ac/Inventory/49/1560208.htm) as returned by search; paper not read)
    (unverified).
  - N. J. Kasdin, "Discrete Simulation of Colored Noise and Stochastic Processes and 1/f^alpha Power
    Law Noise Generation", *Proc. IEEE* 83(5), 802 to 827, May 1995, DOI 10.1109/5.381848
    ([Princeton record](https://collaborate.princeton.edu/en/publications/discrete-simulation-of-colored-noise-and-stochastic-processes-and/));
    abstract read, body not; it reviews digital 1/f^alpha generators and closes with approximate
    real-time methods.
  - R. Bristow-Johnson, music-dsp mailing list, 1998-06-30: a "pinking" filter of three real poles
    and three real zeros, response within +/-0.3 dB of -3 dB/octave over 10 octaves, "an equiripple
    approximation to the ideal pinking filter can be realized by alternating real poles with real
    zeros"; as reported on [firstpr.com.au](https://www.firstpr.com.au/dsp/pink-noise/) (page dated
    1999-10-18, updated to 2011); the original post was not seen.
  - J. O. Smith and H. F. Smith, "Closed Form Fractional Integration and Differentiation via Real
    Exponentially Spaced Pole-Zero Pairs", arXiv:1606.06154, 2016-06-07
    ([abstract](https://arxiv.org/abs/1606.06154)): poles evenly spaced on a log scale, zeros
    interleaved, for alpha = 1/2 zeros at the midpoints between poles. The same material is on
    [CCRMA](https://ccrma.stanford.edu/~jos/spectilt/Exponentially_Distributed_Real_Pole_Zero.html)
    (Spectral Tilt Filters; page copyright 2025).
  - J. O. Smith, *Spectral Audio Signal Processing*, W3K Publishing, 2011 (ISBN
    978-0-9745607-3-1), "Example: Synthesis of 1/F Noise (Pink Noise)"
    ([DSPRelated](https://www.dsprelated.com/freebooks/sasp/Example_Synthesis_1_F_Noise.html)).
    The page as fetched gives a 3rd-order filter by coefficients and does not mention Keshner,
    Corsini or Saletti, or alternation; the planned design's attribution to them is therefore not
    confirmed by this page (see "Open uncertainties"). The alternation is on the spectral-tilt page
    [Approximating Arbitrary Slopes](https://ccrma.stanford.edu/~jos/spectilt/Approximating_Arbitrary_Slopes.html):
    a pole at each octave point, zeros at the half-octave points, average slope -10 dB/decade
    (publication year of that page not shown).
  - Patents that state the technique or neighbours of it as known, all expired: US6671663 (TI,
    priority 1997-06-30) describes a 1/f response approximated by "several first order filters in
    parallel, with the pole frequencies logarithmically spaced" (per Google Patents text), and
    US4327419A (Kawai, 1980-02-22) says white noise "can also be processed by a digital filter to
    produce" pink noise (per a search summary, unverified). See "Expired by term".
- **Element 1 (white source).** See [prng-patents.md](prng-patents.md).
- **Element 3 (normalisation).** No source sought; open uncertainty.
- **Element 4 (seeding).** Deterministic seeding of a PRNG: see [prng-patents.md](prng-patents.md).
- Not in scope, listed for orientation: R. F. Voss and J. Clarke, "'1/f noise' in music and
  speech", *Nature* 258, 317 to 318, 1975, DOI 10.1038/258317a0
  ([Nature](https://www.nature.com/articles/258317a0)), the octave row-summing method.

## Candidates

Patents that might cover an element of the design and might still be in force. Status is filled
in by a human from Google Patents or USPTO Patent Center. Dates are as Google Patents prints them
(Google states that its status and expiry fields are assumptions, not legal conclusions). Claims
of the in-force rows 1 to 3 were read in stage 2; see Claim analysis.

| # | Patent | Assignee | Priority / filing | Est. term end | Element it may touch | Link | Status |
|---|---|---|---|---|---|---|---|
| 1 | US10615764B1, Tunable pink noise generator with no low frequency cutoff | HRL Laboratories, LLC | 2018-09-10 / 2019-09-04 | 2039-09-04 (Google; printed "Active", 4th-year fee paid 2023) | Pink noise generation in general (2, 4); abstract: summed stochastic generators that each hold or update a random value by a probability, not a filter. Stage 2: **Does not read** (independent claims 1, 7, 14, 20; see Claim analysis) | [GP](https://patents.google.com/patent/US10615764) | In force (checked by Chris Paro, 2026-11-13) |
| 2 | US10841717B2, Signal generator and method for measuring the performance of a loudspeaker | Meyer Sound Laboratories Inc | 2018-06-21 / 2019-06-17 | 2039-06-17 (Google; no adjustment shown; printed "Active") | 2 (loosely: description says white noise can be passed through a "pink filter"; abstract is a pink-noise-plus-impulse test signal with a crest-factor equaliser). Stage 2: **Does not read** (independent claims 1, 7, 12, 15, 21; see Claim analysis) | [GP](https://patents.google.com/patent/US10841717B2/en) | In force (checked by Chris Paro, 2026-11-13) |
| 3 | US8653354 (B1), Audio synthesizing systems and methods | Sonivox LP | 2011-08-02 / 2011-08-02 | 2031-08-14 (Google adjusted; printed "Active") | 2 (very loosely: a noise-like source passed through a sequence of shaping filters; pink noise defined in the description). Stage 2: **Does not read** (independent claims 1, 14, 22; see Claim analysis) | [GP](https://patents.google.com/patent/US8653354) | In force (checked by Chris Paro, 2026-11-13) |
| 4 | US8994919B2, Lithographic apparatus and device manufacturing method | ASML Netherlands BV | 2007-10-02 / 2008-10-02 | 2033-11-17 (Google adjusted; page prints "Expired - Fee Related", so lapsed for fees, not by term) | 2 (loosely: a "partial order filter", a half-order low-pass built by shifting poles and zeros of a sqrt(s) approximation, in a position-control system; not noise synthesis) | [GP](https://patents.google.com/patent/US8994919B2/en) | Not in force: lapsed (checked by Chris Paro, 2026-11-13) |

Status values: **In force**, **Lapsed (fees)**, **Expired (term)**, **Abandoned**, **Invalidated**,
**Not found**. Record who checked and when, under the table.

Expired by term (filing date + 20 years, plus any printed term adjustment, is in the past), listed
for completeness and needing no check:

- [US7251301](https://patents.google.com/patent/US7251301), Industrial Technology Research
  Institute, filed 2002-10-01, Google "adjusted" expiry 2025-01-17 (also printed "Expired - Fee
  Related", lapsed 2019-07-31). Digital 1/f noise by summing band signals; its background
  discusses filtering white noise and the Voss method (element 2, loosely).
- [US6671663](https://patents.google.com/patent/US6671663), Texas Instruments, filed 1998-06-24,
  adjusted expiry 2020-07-15. Circuit noise simulation; describes a 1/f filter as parallel
  first-order sections with log-spaced poles (element 2).
- [US4327419A](https://patents.google.com/patent/US4327419A/en), Kawai Musical Instruments, filed
  1980-02-22, expiry 2000-02-22. Digital noise generator for musical instruments with a
  spectral-shape parameter, without digital filters (elements 2, 4).
- [US7385537B2](https://patents.google.com/patent/US7385537B2/en), Texas Instruments, filed
  2005-02-28, Google adjusted expiry 2026-10-15 (past as of 2026-11-13; printed "Active" in the
  page as fetched, which is stale). LFSR first-order noise generator (element 1, loosely). The
  expiry is within a month of today; a human should confirm it.
- [US6823176B2](https://patents.google.com/patent/US20040203454A1/en) (publication
  US20040203454A1), Sony (original assignee individual), filed 2002-09-23, printed "Expired -
  Lifetime", Google "adjusted expiration" event 2023-01-04. Masking noise from a pseudo-noise
  generator, optionally filtered (elements 1, 2, loosely). Family member
  [CN100449610C](https://patents.google.com/patent/CN100449610C/en), filed 2003-09-22, expiry
  2023-09-22, printed "Expired - Fee Related".

Seen and not listed (not opened): US4673871 (Digital noise generator) and US4191175A (noise-like
audible signal), both surfaced in search 7 and both from the 1980s or earlier by number, so past
term; EP1543495B1 and JP5007024B2, granted family members of US6823176B2, not opened.

## Claim analysis

Stage 2, 2026-11-13, for the rows a human marked **In force** (1, 2 and 3). Claim text comes from
the Google Patents HTML full text, not OCR. The fetch tool returns text through a summarising model
and limits each quote to 125 characters, so every independent claim was obtained as consecutive
verbatim fragments of at most 120 characters each, joined here with no gaps. US8653354B1's claims
1, 14, 22 and US10841717B2's claims 1, 12, 21 were each fetched twice, and the two fetches agreed
word for word; US10841717B2's claims 7 and 15 and US10615764B1's four independent claims were
fetched in full once (the first US10615764B1 fetch gave only preambles, which match). The granted
PDFs were not opened (see Open uncertainties). The Google Patents text flattens subscripts and
superscripts: "NS" is N with subscript S, and "½N" is 1/2^N, as the description confirms ("the
probability p is decreased by a factor of two in each succeeding cascaded stochastic signal").

Design facts used (from "Planned design"): one white stream (xorshift64\*, uniform), passed
through a cascade of first-order IIR sections `(1 - q_k z^-1) / (1 - p_k z^-1)`, each section's
input the previous section's output; a fixed output gain; seeded. The output is non-pitched 1/f
noise for an offline gain display. The design has no bank of generators each holding or
updating its own random value, no update probabilities, no stored values selected and replaced,
no impulsive noise source, no crest-factor control, no loudspeaker drive, no user parameters for
the spectrum, no feedback or band suppression, no modulation of one section by another, and no
pitched output.

### Row 1: US10615764B1 (HRL Laboratories), priority 2018-09-10

Independent claims: 1, 7, 14, 20 (23 claims; claims 2 to 6 depend on 1, 8 to 13 on 7, 15 to 19
on 14, 21 to 23 on 20, per the Google Patents claim list).

**Claim 1** (method).

| Element | Planned design |
|---|---|
| "A method for providing pink noise and for improving computational efficiency for providing pink noise comprising:" | Met as to providing pink noise. |
| "providing NS stochastic signal generators, wherein each of the NS stochastic signal generators has a respective output at each discrete time n;" | Not met: the design has one random source; the filter sections are deterministic, not stochastic signal generators, and do not each draw random numbers. |
| "generating a respective first random number g with variance σ for each respective stochastic signal generator at each discrete time n;" | Not met: one uniform value per sample in total, not one per generator. |
| "generating a respective second random number u for each respective stochastic signal generator at each discrete time n;" | Not met: no second random number. |
| "comparing for each respective stochastic signal generator the respective second random number u with an update probability for the respective stochastic signal generator at each discrete time n, wherein the update probability for an Nth respective stochastic signal generator is ½N, where N ranges from 1 to NS for the respective stochastic signal generators;" | Not met: no comparison, no update probability. Pole frequencies are log-spaced, which is a deterministic analogue of halving rates, but no probability is involved. |
| "if the respective second random number u is greater than the update probability for the respective stochastic signal generator, then not updating a respective output of the respective stochastic signal generator;" | Not met: every section updates its state every sample. |
| "if the respective second random number u is less than or equal to the update probability for the respective stochastic signal generator, then updating the respective output of the respective stochastic signal generator to be equal to the first random number; and" | Not met: no section's output is set equal to a random number; each is a recursive combination of its input and its previous state. |
| "summing at each discrete time n the respective output of each of the NS stochastic signal generators to provide a sum." | Not met: the sections are in series, not summed. |

**Claim 7** (device).

| Element | Planned design |
|---|---|
| "A device for generating pink noise and for improving computational efficiency for providing pink noise comprising:" | Software; treated as met for the analysis below, which does not turn on it. |
| "NS stochastic signal generators, wherein each respective stochastic signal generator of the NS stochastic signal generators comprises;" | Not met, as for claim 1. |
| "a random number generator for a generating a respective first random number g with variance σ for the respective stochastic signal generator and for generating a respective second random number u for the respective stochastic signal generator;" | Not met: one generator for the whole design, one value per sample. |
| "a comparator coupled to the random number generator for comparing for the respective second random number u with an update probability for the respective stochastic signal generator, wherein the update probability for an Nth respective stochastic signal generator is ½N, where N ranges from 1 to NS for the respective stochastic signal generators; and" | Not met: no comparator, no update probability. |
| "a register for storing a respective output of the respective stochastic signal generator, wherein if the respective second random number u is greater than the update probability for the respective stochastic signal generator, then not updating or changing the respective output stored in the register for the respective stochastic signal generator, and wherein if the respective second random number u is less than or equal to the update probability for the respective stochastic signal generator, then storing the first random number in the register; and" | Not met: each section holds a state, but it is updated every sample by the filter recursion, never conditionally replaced with a random number. |
| "a summer coupled to an output of each respective stochastic signal generator for summing at each discrete time n the respective output of each of the NS stochastic signal generators." | Not met: sections in series, no summer. |

**Claim 14** (device).

| Element | Planned design |
|---|---|
| "A device for generating pink noise and for improving computational efficiency for providing pink noise comprising:" | As for claim 7. |
| "a random number generator for generating a first random number g with variance σ output at each discrete time n, and for generating a second uniformly random number u output at each discrete time n;" | Partly: the design generates one uniform number per sample; it generates no second number. Not met as recited. |
| "NS registers, each of the NS registers for storing a respective one of NS stochastic values;" | Not met: the section states are filter states, not stored random values. |
| "a priority encoder coupled to the second random number u output for determining at each discrete time n a selected respective stochastic value of the NS stochastic values to update with the first random number g with variance σ output;" | Not met: no selection of one stored value per sample. |
| "a sum register; and" | Not met in the claimed sense (see the next elements); the design keeps no running sum of stored values. |
| "a summer coupled to the NS registers, the first random number g with variance σ output, and to the sum register, and having an output coupled to the sum register;" | Not met. |
| "wherein the summer adds the first random number g with variance σ to a sum stored in the sum register and subtracts the selected respective stochastic value to form a new sum at each discrete time n;" | Not met: no add-new, subtract-replaced running sum. A first-order section `y[n] = p y[n-1] + x[n] - q x[n-1]` adds the new input and subtracts a scaled old input, but the subtracted term is its own previous input, not a selected stored value being replaced, and the state is scaled by `p`. |
| "wherein the new sum is stored in the sum register at each discrete time n; and" | Not met, as above. |
| "wherein the first random number g with variance σ output is stored in the respective register for the selected respective stochastic value at each discrete time n." | Not met: no random number is stored in a selected register. |

**Claim 20** (method).

| Element | Planned design |
|---|---|
| "A method for generating pink noise and for improving computational efficiency for providing pink noise comprising:" | Met as to generating pink noise. |
| "generating a first random number g with variance σ output at each discrete time n;" | Met: one white sample per time step. |
| "generating a second uniformly random number u output at each discrete time n;" | Not met: no second random number. |
| "storing NS stochastic values;" | Not met: filter states, not stored random values. |
| "using the second random number u output for determining at each discrete time n a selected respective one of the stored NS stochastic values to update with the first random number g with variance σ output at each discrete time n;" | Not met: no selection. |
| "storing a sum;" | Not met in the claimed sense (see claim 14). |
| "adding the first random number g with variance σ output to the sum and subtracting the stochastic value of the selected respective one of the stored NS stochastic values to form a new sum at each discrete time n;" | Not met, as for claim 14. |
| "replacing the stored sum with the new sum at each discrete time n; and" | Not met. |
| "replacing the stochastic value of the selected respective one of the stored NS stochastic values with the first random number g with variance σ output at each discrete time n." | Not met. |

**Verdict: Does not read.** Claims 1 and 7 require NS stochastic signal generators each updated
with probability 1/2^N and summed; claims 14 and 20 require a second random number that selects one
of NS stored stochastic values to be replaced, with a running sum adjusted by adding the new value
and subtracting the replaced one. The design has none of these: it filters one white stream
through deterministic first-order sections in series. Prior art in this note that predates its
2018-09-10 priority, for the claimed method: Voss and Clarke 1975 (octave row-summing, the
method's ancestor; out of scope for the design); for the design's pinking filter: Keshner 1982,
Corsini and Saletti 1988 (both unverified), Kasdin 1995, Bristow-Johnson 1998, Smith 2011, Smith and
Smith 2016, US6671663 (priority 1997).

### Row 2: US10841717B2 (Meyer Sound Laboratories), priority 2018-06-21

Independent claims: 1, 7, 12, 15, 21 (claims 2 to 6 depend on 1, 8 to 11 on 7, 13 and 14 on 12,
16 to 20 on 15, per the Google Patents claim list; the list shows no dependent of claim 21).

**Claim 1** (noise generator).

| Element | Planned design |
|---|---|
| "A noise generator for generating a test signal for measuring the performance of a loudspeaker over an operating broad band of frequencies ranging from low to high frequencies, said noise generator comprising:" | Not met as to purpose: the output feeds an offline display of an effect's gain, not a loudspeaker measurement. The verdict does not turn on the preamble. |
| "a broadband noise source for generating broadband noise over the operating broad band of frequencies," | Met: the pink noise generator. |
| "an impulsive noise source for generating random impulses of noise," | Not met: the design has no impulsive source. |
| "means for equalizing the broadband noise generated by the broadband noise source, and" | Arguably met: the pinking filter shapes the white noise's spectrum. |
| "means for equalizing the random noise impulses," | Not met: no impulses. |
| "wherein the equalized broadband noise and the equalized randomly generated noise impulses are combined into a composite noise signal and wherein the broadband noise and the randomly generated noise impulses can be equalized to produce a composite noise signal having a desired crest factor versus frequency over the operating broad band of frequencies." | Not met: nothing is combined, and no crest factor is controlled. |

**Claim 7** (noise generator).

| Element | Planned design |
|---|---|
| Preamble, identical to claim 1's | As for claim 1. |
| "a first noise signal path having a broadband noise source for generating broadband noise over the operating broad band of frequencies, and having a filter configured to set a desired level of broadband noise over the operating broad band of frequencies," | Arguably met: white source plus pinking filter. |
| "a second noise signal path having an impulsive noise source for randomly generating noise impulses, and having a filter configured to set an energy level in the noise impulses within the operating broad band of frequencies, and" | Not met: no second path, no impulses. |
| "gain controls for setting gain in the first and second signal paths," | Partly: one fixed normalisation gain; no second path. Not met as recited. |
| "wherein a desired crest factor can be established as a function of frequency in a combined noise signal from the first and second noise signal paths by setting and configuring at least one of the gain controls and filter configurations in both the first and second signal paths, and wherein such settings and configurations are established such that the crest factor of the combined noise signal increases with frequency." | Not met: no combined signal, no crest-factor setting. |

**Claim 12** (noise generator).

| Element | Planned design |
|---|---|
| Preamble, identical to claim 1's | As for claim 1. |
| "a first noise signal path having a broadband source of pink noise over the operating broad band of frequencies, and having a filter configured to reduce an average level of the broadband pink noise at high frequencies within the operating broad band of frequencies," | Partly: a source of pink noise is met; the design has no further filter that reduces the pink noise's level at high frequencies (the pinking filter is what makes it pink). |
| "a second noise signal path having an impulsive noise source that randomly generates noise impulses, said second noise signal path further having a filter configured to reduce an energy level in the randomly generated noise impulses at low frequencies within the operating broad band of frequencies," | Not met: no impulses. |
| "said impulsive noise source further being configurable for controlling an average rate at which impulsive noise is generated, and" | Not met. |
| "gain controls for setting gain in the first and second signal paths," | Not met as recited (one path). |
| "wherein at least one of the following controllable parameters in each of the first and second noise signal paths is controllable to produce a composite noise signal output from the noise generator that has a crest factor that increases with frequency: i) in the first noise signal path, the filter and gain control; ii) in the second signal path, the filter, the gain control and the average rate at which the noise impulses are generated." | Not met: no composite signal, no crest factor. |

**Claim 15** (method).

| Element | Planned design |
|---|---|
| "A method of measuring the performance of a loudspeaker over an operating broad band of frequencies ranging from low to high frequencies, comprising:" | Not met: no loudspeaker measurement. |
| "generating broadband noise over the operating broad band of frequencies," | Met. |
| "randomly generating impulses of noise," | Not met. |
| "separately equalizing the broadband noise and random noise impulses," | Not met as to impulses. |
| "combining the equalized broadband noise and equalized random noise impulses into a composite test signal having a crest factor, and" | Not met. |
| "driving the loudspeaker to be measured with the composite test signal," | Not met: the output goes to an offline analysis, not a loudspeaker. |
| "wherein the broadband noise and random noise impulse are equalized such that the crest factor for the test signal increases with frequency." | Not met. |

**Claim 21** (method).

| Element | Planned design |
|---|---|
| "A method of measuring the performance of a loudspeaker over an operating broad band of frequencies ranging from low to high frequencies, comprising:" | Not met, as for claim 15. |
| "generating broadband pink noise over the operating broad band of frequencies," | Met. |
| "randomly generating impulses of pink noise," | Not met. |
| "combining the broadband noise and random noise impulses into a composite test signal having a crest factor," | Not met. |
| "equalizing the broadband noise," | Arguably met (pinking filter). |
| "equalizing the random noise impulses, and" | Not met. |
| "driving the loudspeaker to be measured with the composite test signal," | Not met. |
| "wherein the broadband noise and random noise impulse are equalized such that the crest factor for the test signal gradually increases with frequency over the operating frequency range." | Not met. |

**Verdict: Does not read.** Every independent claim requires an impulsive noise source (random
impulses of noise) combined with the broadband noise into a composite signal whose crest factor
is set or increases with frequency (claims 1, 7, 12: impulsive source and composite; claims 15,
21: random impulses, composite, and driving a loudspeaker). The design has no impulsive source,
no combination and no loudspeaker. The description's white noise "passed through a "pink filter"
for converting the white noise into pink noise" names the design's function but gives no filter
design and is not claimed on its own. Prior art in this note that predates its 2018-06-21
priority, for the pinking filter: Keshner 1982, Corsini and Saletti 1988 (both unverified), Kasdin
1995, Bristow-Johnson 1998, Smith 2011, Smith and Smith 2016, US6671663 (1997).

### Row 3: US8653354B1 (Sonivox), priority 2011-08-02

Independent claims: 1, 14, 22 (24 claims; claims 2 to 13 depend on 1, directly or through claim
6; 15 to 21 on 14; 23 and 24 on 22, per the Google Patents claim list). The page was truncated at
100000 of 108572 characters; the claims were within the part read.

**Claim 1** (method).

| Element | Planned design |
|---|---|
| "A method for synthesizing audio to produce a musical sound, comprising the steps of:" | Not met as to "musical sound": the output is noise for an offline gain display. See the last element, which decides it. |
| "inputting an audio source;" | Met: the white noise stream. |
| "setting parameters to specify the desired frequency slit spacing and the desired noise-to-frequency-band ratio;" | Not met: the design takes only a seed and a sample rate; it has no slit-spacing or noise-to-band parameter. Pole and zero frequencies are fixed by the sample rate. |
| "filtering the audio source through at least one sequence of at least two filters to filter the audio source into a series of frequency-bands-with-noise;" | Partly: the cascade is a sequence of at least two filters. Its output is one continuous 1/f spectrum, not a series of frequency bands separated by slits. Not met as recited, on the plain reading of "series of frequency-bands-with-noise" with "slit spacing". |
| "during the step of filtering, conforming the series of frequency-bands-with noise to the parameters to produce the desired frequency slit spacing and the desired noise-to-frequency-band ratio;" | Not met: no such parameters. |
| "wherein input to the first filter is the audio source, and the input to a subsequent filter is the output of the previous filter, whereby the last filter produces audio output; and" | Met: the sections are in series in exactly this way. |
| "outputting audio output to produce the musical sound," | Not met (see preamble). |
| "wherein the audio source comprises non-pitched, broad-spectrum audio with no discernible pitch and timbre, and" | Met: white noise. |
| "the audio output comprises pitched, musical sounds with discernible pitch and timbre." | Not met: pink noise is non-pitched; it has no discernible pitch. |

**Claim 14** (method).

| Element | Planned design |
|---|---|
| "A method for synthesizing audio to produce a musical sound, comprising the steps of:" | Not met, as for claim 1. |
| "receiving an audio source;" | Met. |
| "filtering the audio source through a first filter to filter the audio source into a series of frequency-bands-with-noise;" | Not met as recited, as for claim 1 (a first filter exists; no series of bands). |
| "suppressing high energy bands to increase feedback in the series of frequency-bands-with-noise;" | Not met: no band suppression and no feedback control. |
| "re-filtering the series of frequency-bands-with-noise having suppressed high energy bands through a second filter; and" | Partly: later sections re-filter earlier sections' output; nothing is suppressed between them. Not met as recited. |
| "outputting the series of frequency-bands-with-noise as audio output to produce musical sound," | Not met. |
| "wherein the audio source comprises non-pitched, broad-spectrum audio with no discernible pitch and timbre, and" | Met. |
| "the audio output comprises pitched, musical sounds with discernible pitch and timbre." | Not met. |

**Claim 22** (method).

| Element | Planned design |
|---|---|
| "A method for synthesizing audio to produce a musical sound, comprising the steps of:" | Not met, as for claim 1. |
| "receiving an audio source;" | Met. |
| "accepting parameters to specify the desired frequency slit spacing and the desired noise-to-frequency-band ratio;" | Not met. |
| "filtering the audio source through at least one sequence of at least two filters to filter the audio source into a series of frequency-bands-with-noise with the desired frequency slit spacing and the desired noise-to-frequency-band ratio;" | Partly (sequence of filters); not met as to bands, slit spacing and ratio. |
| "suppressing, between each filter in the sequence, high energy bands to increase feedback in the series of frequency-bands-with-noise;" | Not met. |
| "modulating, between each filter in the sequence, the output of at least one of the filters in the sequence using the output of another filter in the sequence;" | Not met: no section's output modulates another's; each section only filters its input. |
| "calculating, between each filter in the sequence, the parameters and at least one co-efficient of the filter to prevent passing of unity gain; and" | Not met: coefficients are computed once at construction from the sample rate, not between filters, and not to prevent unity gain. |
| "outputting audio output to produce musical sound." | Not met. |

**Verdict: Does not read.** Claims 1 and 14 require "audio output [that] comprises pitched,
musical sounds with discernible pitch and timbre"; the design outputs non-pitched noise. Claim 1
also requires parameters specifying "frequency slit spacing" and "noise-to-frequency-band ratio",
claim 14 suppressing high energy bands to increase feedback, and claim 22 the same parameters,
the suppression, and modulating one filter's output by another's; the design has none. The only
shared elements are a non-pitched broadband input and filters in series. Prior art in this note
that predates its 2011-08-02 priority, for the pinking filter: Keshner 1982, Corsini and Saletti
1988 (both unverified), Kasdin 1995, Bristow-Johnson 1998, US6671663 (1997); Smith, *Spectral
Audio Signal Processing* is dated 2011 (month not established, so its order relative to the
priority date is not known).

## Search log

All searches 2026-11-13. The web search tool returns a prose summary, not raw result lists, so hit
counts are approximate (about 10 per query).

| Database | Query | Date | Hits | Result |
|---|---|---|---|---|
| Web (general) | 1. patent pink noise generator cascaded first-order filters pole zero 1/f noise digital | 2026-11-13 | 10 | US10615764, US7251301; Favrot paper; DSPRelated and sampo.kapsi.fi code |
| Web (general) | 2. patent 1/f noise synthesis digital filter alternating poles and zeros pinking filter audio | 2026-11-13 | 10 | No patent claiming it; Smith spectral tilt pages; US7251301 |
| Web (general, extended) | 3. patent "pink noise" generator digital IIR filter white noise shaped 3 dB per octave filter coefficients claim | 2026-11-13 | 9 | US7251301, US8653354; no coefficient patent |
| Web (general, extended) | 4. patent flicker noise 1/f noise generation digital filter cascade first-order sections white noise "1/f" generator simulation | 2026-11-13 | 9 | US6671663, US7385537, US4327419; EP0586021B1 and US3746791 surfaced, not opened |
| Web (general) | 5. patent fractional integrator approximation real poles and zeros interleaved logarithmically spaced "half-order" filter 1/f | 2026-11-13 | 10 | US8994919; Smith arXiv; fractional-order filter papers |
| Web (general, extended) | 6. patent "pink noise" OR "1/f noise" generation "poles" "zeros" cascade "first-order" digital audio synthesizer noise source filter -Voss | 2026-11-13 | 9 | US4327419; firstpr.com.au (Bristow-Johnson); US4729112, US5502663, US4797846, US7831645 (general filters, not opened) |
| Web (general) | 7. patent pink noise Voss-McCartney random generator audio test signal generation claims granted 2010..2024 | 2026-11-13 | 10 | US10615764, US7251301; no Voss-McCartney patent |
| Web (general, extended) | 8. patent noise generator "pink noise" digital audio signal processor "filter" white noise source hearing test OR tinnitus OR sound masking OR audio measurement US patent | 2026-11-13 | 9 | US4327419, US4191175, US4673871, US20040203454, CN100449610C, US10841717 |
| Web (general) | 9. Keshner "1/f noise" Proceedings of the IEEE 1982 vol 70 no 3 pp 212-218 doi | 2026-11-13 | 10 | Citation and DOI |
| Web (general) | 10. Corsini Saletti "A 1/f^gamma power spectrum noise sequence generator" IEEE Trans Instrum Meas 1988 doi | 2026-11-13 | 10 | Citation and DOI; matched z-transform |
| Web (general) | 11. Voss Clarke "1/f noise in music and speech" Nature 258 317 1975 doi | 2026-11-13 | 10 | Citation and DOI |
| Google Patents (page fetch by number) | US10615764, US7251301, US8994919B2, US4327419A, US7385537B2, US6671663, US8653354, US10841717B2, US20040203454A1, CN100449610C | 2026-11-13 | 10 | Dates and status read; abstracts only, claims not read; cited and cited-by lists read for US10615764 and US7251301 |
| Google Patents (stage 2, claims) | [US10615764B1](https://patents.google.com/patent/US10615764B1/en): claims 1, 7, 14, 20, the dependency list of claims 1 to 23, and the description's reading of "½N" | 2026-11-13 | 1 | Full text of the four independent claims read, in verbatim fragments; HTML full text, not OCR |
| Google Patents (stage 2, claims) | [US10841717B2](https://patents.google.com/patent/US10841717B2/en): claims 1, 7, 12, 15, 21, the dependency list, and the "pink filter" sentence of the description | 2026-11-13 | 1 | Full text of the five independent claims read, in verbatim fragments (1, 12, 21 fetched twice and agreed); HTML full text, not OCR |
| Google Patents (stage 2, claims) | [US8653354B1](https://patents.google.com/patent/US8653354B1/en): claims 1, 14, 22, the dependency list, and the description's filter types | 2026-11-13 | 1 | Full text of the three independent claims read, in verbatim fragments, each fetched twice and agreed; HTML full text, not OCR |
| Other pages fetched | CCRMA SASP and spectilt pages; DSPRelated SASP page; arXiv 1606.06154; firstpr.com.au pink noise; sampo.kapsi.fi PinkNoise; Princeton record for Kasdin | 2026-11-13 | 6 | Read |

Refused or failed: `patents.google.com/patent/US6671663B2/en` returned HTTP 404 (the URL without
the kind code worked). The Keshner PDF (korea.ac.kr) and the NIST technical note 1169 PDF came back
as binary the fetch tool could not read. Google Patents pages for US8994919B2 and US8653354 were
truncated at 100000 characters. In stage 2 the fetch tool once declined to quote US8653354B1's
claims in fragments (it gave a summary); a second request, asking for the claim words as numbered
short quotes, returned them. Its first request for US10615764B1's claims gave only preambles; the
claims were then quoted in fragments. Not tried: Espacenet, USPTO Patent Public Search and Patent Center,
FreePatentsOnline, any search by CPC class (for example G10H 2250/481 noise generation, H03B 29/00
noise generators, G06F 17/10 and H03H 17/02 digital filters) or by assignee, and citation lists
beyond those of US10615764 and US7251301.

## Open uncertainties

- No patent-database search was run (see "Refused or failed"). "No candidate found" is a statement
  about web-search coverage. CPC-class searches (noise generators, digital filters, fractional-order
  filters, audio test signals) and assignee searches (Dolby, Audio Precision, Meyer Sound, Texas
  Instruments, Analog Devices, Yamaha, Roland) are the main gap.
- Keshner 1982 and Corsini and Saletti 1988: neither paper was read, so whether either describes a
  cascade of alternating real poles and zeros is not established here. The planned design credits
  them via Smith, *Spectral Audio Signal Processing*; the "Synthesis of 1/f noise" page as fetched
  does not mention them. The earliest source seen that states the alternation is Bristow-Johnson
  1998-06-30 (second-hand, via firstpr.com.au), and Smith's spectral-tilt page (undated here).
- ~~Pole spacing~~: settled at one pole per octave with zeros at the half-octaves, as on Smith's
  spectral-tilt page (see "Planned design"). None of the stage 2 verdicts turns on the spacing.
- Elements 3 (normalisation by a fixed gain) and 4 (seeding): no prior art sought; both are
  trivial steps. Element 1 relies on prng-patents.md.
- Rows 1 to 3 (stage 2): the claim text came through the fetch tool's summarising model in
  fragments of at most 120 characters, joined here; it was not compared with the granted PDFs. A
  human should check the quoted claims against the PDFs or USPTO Patent Center before relying on
  the verdicts. US10615764B1's independent claims were fetched in full only once. Not checked:
  certificates of correction, reexamination or other post-grant changes, and family members
  (continuations) with different claims, for all three rows.
- Row 1: the verdict rests on the claims requiring separate stochastic generators (or stored
  random values) that are conditionally replaced by fresh random numbers. A first-order section
  adds its new input and subtracts a scaled previous input, which resembles the add-and-subtract
  of claims 14 and 20 only in form; a human may confirm that no construction treats filter
  states as "stored NS stochastic values". On this reading the second random number u and the
  selection step are still missing.
- Row 2: the design does not read; a future feature that generates a loudspeaker or measurement
  test signal mixing pink noise with random impulses would need its own check against this
  patent.
- Row 3: the verdict rests on "pitched, musical sounds with discernible pitch and timbre" (claims
  1, 14) and on the slit-spacing parameters, band suppression and inter-filter modulation; the
  meaning of "series of frequency-bands-with-noise" is not needed to decide it. A future
  feature that shapes noise into pitched tones (a noise-excited synthesiser voice) would need its
  own check against this patent.
- Row 4 was not analysed: a human found it lapsed. US7385537B2's expiry (2026-10-15) is recent and
  should be confirmed.
- Term end dates are as Google prints them; terminal disclaimers and adjustments not otherwise
  seen.
