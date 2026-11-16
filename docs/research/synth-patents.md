# Patent status of the subtractive synth's oscillators, envelope and voice allocation

Research note, 2026-11-16. This is not legal advice. It records what primary sources show, so a
reviewer can check it.

## Planned design

A polyphonic subtractive synthesizer node (issue #173). Each element as it will be built:

1. **Oscillators.** Naive phase-accumulator saw, pulse/square (variable pulse width) and triangle
   waves. The phase advances by `f / fs` per sample and wraps at 1. Saw and pulse are corrected at
   each discontinuity with a 2-point polynomial band-limited step (PolyBLEP): with `dt = f / fs`,
   the residual `2u − u² − 1` for `u = t / dt` on the sample just after the edge, and `u² + 2u + 1`
   for `u = (t − 1) / dt` on the sample just before it, scaled by the step height and subtracted. The triangle is either corrected with the
   integrated residual (PolyBLAMP) at its slope changes, or built as a leaky integral of the
   PolyBLEP square. Two oscillators per voice, each with octave, detune (cents), pulse width and
   level, summed.
2. **Envelope.** An ADSR generator with stages Idle, Attack, Decay, Sustain, Release. Each stage is
   an exponential one-pole segment toward a target (attack aims past 1.0 and stops at 1.0; decay
   aims at the sustain level; release aims at 0 and goes idle below a threshold). A new gate-on
   retriggers from the current level, without resetting to 0. One envelope for amplitude, one for
   the filter cutoff, per voice.
3. **Filter.** A per-voice state-variable filter (`dsp::Svf`, the trapezoidal/topology-preserving
   SVF already in the workspace) used as a resonant low-pass. Its cutoff is modulated every sample,
   in log2 Hz, by the filter envelope (amount in octaves), the LFO and note velocity. The cutoff is
   clamped below 0.49·fs, which keeps the claims of US11202147B2 from reading (as recorded in
   `docs/research/eq-filter-patents.md`).
4. **Voice allocation and stealing.** A fixed pool of up to 16 voices. A new note takes a free
   (idle) voice first; otherwise the quietest voice in its release stage; otherwise the oldest
   held voice. A stolen voice fades out over about 3 ms before starting the new note. A note
   already sounding retriggers its own voice. MIDI channels are omni.
5. **Pitch and velocity.** 12-tone equal temperament from A4 = 440 Hz:
   `f = 440 · 2^((note − 69) / 12)`. Pitch bend shifts the note by `bend · range` semitones,
   where `range` is a parameter in semitones. Velocity scales amplitude and filter cutoff by
   per-parameter amounts. CC 64 holds released notes until the pedal lifts.
6. **LFO.** Reuses `modulation::Lfo`, already cleared in ADR-0119; no new research. Destination is
   pitch, cutoff or amplitude, with a depth.

Voices are summed to mono and written to both channels of a stereo output.

**Revision, 2026-11-16.** After the claim analysis of candidate 1, Chris Paro chose (flag-risk,
tag `patent`) to replace element 1's PolyBLEP correction with a minimum-phase band-limited step
(MinBLEP, Brandt 2001) read from a table built in `prepare`, with the triangle as a leaky integral
of the MinBLEP square. The analysis above of the PolyBLEP form is kept as the reason.

## Summary

Stage 2 (claim reading) done 2026-11-16 for all four in-force candidates (3 and 4 in a second
pass the same day). Per-claim reading under Claim analysis.

| Technique | Verdict | Confidence |
|---|---|---|
| 1a. Saw and pulse corrected by the 2-point PolyBLEP residual | **Reads.** US7589272B2 (Korg, to 2028-07-04) claims 2, 15 and 60 read element by element: the residual is exactly odd symmetric about the edge. Claim 1 reads too if "essentially no DC bias" means the area of the pulse, as the spec uses it (PolyBLEP area is exactly 0). Raise with `flag-risk` (tag `patent`) before building it; see the design-arounds under Claim analysis. | High |
| 1b. Triangle by PolyBLAMP at slope changes | **Does not read.** No claim covers it: a triangle has no discontinuity, and the PolyBLAMP residual is even, not odd, and has nonzero area (1/6 per unit slope change). | Medium (no claim covers slope discontinuities; "discontinuity" is not defined in the claims) |
| 1c. Triangle as a leaky integral of a PolyBLEP square | **Reads**, through the square it integrates (1a; claims 2, 6, 15, 19). | High |
| 1d. Two oscillators per voice, summed | **Does not read** claims 28, 43, 58, 59 (hard sync): the design has no second waveform whose discontinuity phase depends on another waveform's event. | High |
| 2, 6. Envelope, LFO | No in-force candidate touches them, so no claims were read (LFO cleared by ADR-0119). | Low for 2 (search gaps in Open uncertainties) |
| 4. Voice allocation and stealing | **Does not read** US8017856B2 (Roland, to 2029-07-12). Every independent claim needs several timbre "parts" per note: claims 1, 14, 20, 26 assign several parts to one sound; claims 10, 16, 22 need "a plurality of parts comprising different timbers" reassigned among the sounding notes. The design gives each note one voice, all voices play the same patch, and nothing is stacked. | High for the claims (search gaps for element 4 in Open uncertainties) |
| 5. Pitch, bend, velocity, CC 64 | **Does not read** EP2775475B1 (Yamaha), nor its US family member US9064485B2 (candidate 5, in force; same independent claim structure, read in short pieces only, Medium confidence). Claims 1, 10 and 11 need chord information, a check that a tone's pitch glides from a first note pitch to a second, and conversion of both pitches to fit the chord. The design has no chord input and makes no such check. Google lists the EP as "Not-in-force", which disagrees with the Status column (Open uncertainties). | High for the claims |
| 3. Filter (per-sample cutoff modulation of `dsp::Svf`) | **Does not read** US11202147B2, carried over from `docs/research/eq-filter-patents.md`: its claims need a resonant frequency modulated through zero into negative frequency. The synth modulates cutoff every sample in log2 Hz (always positive) and clamps it below 0.49·fs. | High |

## Prior art

Dates marked (unverified) came from a search summary or a citing document; I could not open the
primary text. See Open uncertainties.

- **Element 1 (band-limited step corrections of saw/pulse).**
  Stilson and Smith, "Alias-Free Digital Synthesis of Classic Analog Waveforms", ICMC 1996, pp.
  332-335, Hong Kong, August 1996 (unverified; venue and date from a search summary; the
  [Michigan Publishing page](https://quod.lib.umich.edu/i/icmc/bbp2372.1996.101/--alias-free-digital-synthesis-of-classic-analog-waveforms?view=image)
  returned 403 to me). This is BLIT, not a step correction.
  Brandt, "Hard Sync Without Aliasing", ICMC 2001, pp. 365-368 (page range as cited in
  [US7589272B2](https://patents.google.com/patent/US7589272B2/en); the paper is on
  [ResearchGate](https://www.researchgate.net/publication/2406328_Hard_Sync_Without_Aliasing), not
  opened). Introduces the band-limited step (BLEP) correction added to a naive waveform, and the
  minimum-phase form (MinBLEP). Korg's patent says of it that important details for practical
  implementation are not described.
  Välimäki and Huovilainen, "Antialiasing Oscillators in Subtractive Synthesis", IEEE Signal
  Processing Magazine 24(2), pp. 116-125, March 2007,
  [DOI 10.1109/MSP.2007.323276](https://doi.org/10.1109/MSP.2007.323276) (unverified; citation
  data and "PolyBLEP" in the index keywords come from a search summary; I did not read the text, so
  whether it holds the two-point residual of the Planned design is not established).
  Nam, Välimäki, Abel, Smith, "Alias-Suppressed Oscillators Based on Differentiated Polynomial
  Waveforms", IEEE TASLP 18(4), May 2010
  ([PDF](https://mac.kaist.ac.kr/pubs/jnam-taslp2010.pdf); header read in a search summary only).
  Välimäki, Pekonen, Nam, "Perceptually informed synthesis of bandlimited classical waveforms using
  integrated polynomial interpolation", JASA, 2012
  ([PDF](https://mac.kaist.ac.kr/pubs/ValimakiPeknenNam-jasa2012.pdf); abstract seen in a search
  summary, full text not read): the correction is the difference of a non-bandlimited step and a
  polynomial approximation of the ideal bandlimited step, added around each discontinuity.
  Pekonen, Nam, Smith, Välimäki, "Optimized Polynomial Spline Basis Function Design for
  Quasi-Bandlimited Classical Waveform Synthesis", IEEE Signal Processing Letters, 2012,
  [DOI 10.1109/LSP.2012.2183123](https://doi.org/10.1109/LSP.2012.2183123) (unverified, from a
  search summary).
  The Korg patent's priority date is 2005-01-03, so only the 1996 and 2001 papers precede it; the
  2007-2012 papers postdate it and do not serve as prior art against it.
- **Element 1 (triangle via PolyBLAMP).** Esqueda, Välimäki, Bilbao, "Rounding Corners with
  BLAMP", DAFx-2016, Brno, 2016
  ([DAFx archive](https://dafx.de/paper-archive/details/xRM0GiuF-JmeN0_aaP9VNg)); derives polyBLAMP
  for triangle synthesis, clipping and rectification. The same paper (via a search summary of a
  related Edinburgh paper) credits Huovilainen with proposing the band-limited ramp earlier;
  I did not see that source. No dated source found for a triangle built as a leaky integral of a
  PolyBLEP square.
- **Element 1 (older hard-sync/aliasing art, patent form).** JP3399272B2 (Yamaha), filed 1997-01-14,
  hard sync with a window function near the reset to suppress aliasing
  ([Google Patents](https://patents.google.com/patent/JP3399272B2/en)). Cited by US7589272B2.
- **Element 2 (ADSR, exponential segments).** US4144789A (Kawai), filed 1977-06-06, digital ADSR
  computed by a recursive routine per curve portion
  ([Google Patents](https://patents.google.com/patent/US4144789A/en)); that the routine is
  exponential is not established (reported by a search summary). US4426904A (Casio), priority
  1980-08-01, filed 1983-01-05, fully digital envelope generator with an exponential shape
  ([Google Patents](https://patents.google.com/patent/US4426904)). No dated primary source found for
  one-pole exponential segments toward a target that overshoots (attack aimed past 1.0).
- **Element 2 (retrigger from current level).** Applied Acoustics Systems manuals state a
  retriggered envelope starts from its current output value
  ([CV-2](https://applied-acoustics.com/multiphonics-cv-2/manual/adsr),
  [CV-3](https://www.applied-acoustics.com/multiphonics-cv-3/manual/adsr)); undated, not cited as
  prior art. US5149903 (listed by a search summary as noting that conventional designs start a new
  envelope at a fixed initial value; not opened) is the opposite case.
- **Element 3 (SVF).** See `docs/research/eq-filter-patents.md`, "Prior art" (Zavalishin, Simper).
  Per-sample cutoff modulation by envelope or velocity: US4291605 (Tavel, Beigel, "Music
  synthesizer", not opened) was returned by a search summary as using an envelope on filter
  gain and a modulator on cutoff; US4283984 and the Yamaha SY85 manual (undated) and Cherry Audio
  manuals (undated) show velocity affecting filter envelope or cutoff. None opened or dated by me;
  none cited as prior art.
- **Element 4 (voice allocation, stealing, fade-out).** US4481851A (Norlin), priority 1981-06-22,
  with all tone generators busy, a new key takes the generator of the oldest released key
  ([Google Patents](https://patents.google.com/patent/US4481851)). US4570520A (Kawai), 1984-03-19,
  forcibly zeroes the oldest generator when all are busy
  ([Google Patents](https://patents.google.com/patent/US4570520)). US4703680A (Nippon Gakki/Yamaha),
  priority 1985-04-24, truncate prioritization, lists lowest-level tone and earliest-released key
  as criteria (as relayed by a search summary)
  ([Google Patents](https://patents.google.com/patent/US4703680)). US4909120A (Kawai), priority
  1987-09-29, quick release of the previous tone's envelope before the new tone's attack starts, to
  avoid clicks ([Google Patents](https://patents.google.com/patent/US4909120)). US5245129A
  (Yamaha), priority 1989-01-19, forced damping then a new tone after a wait set from the tone's
  level ([Google Patents](https://patents.google.com/patent/US5245129)). US5159144A (Roland),
  priority 1990-06-29, new note takes the channel with the smallest integrated residual volume or
  lowest envelope level ([Google Patents](https://patents.google.com/patent/US5159144)). Together
  these cover free voice first, then released/quietest, then oldest, with a fast fade before reuse;
  none is a software synth source, and I found no single dated source for the exact three-tier
  order of the Planned design.
- **Element 5 (pitch, bend, velocity, sustain pedal).** No primary source for the equal-temperament
  formula, MIDI pitch bend, velocity or CC 64: I did not open the MIDI 1.0 specification (1983). See
  Open uncertainties. Patent-form art, all from search summaries unless noted: US4653374 (Yamaha,
  1987 grant; summary says the bend value is multiplied by a maximum range set in semitones; not
  opened, [Google Patents](https://patents.google.com/patent/US4653374A/enExample)); US4813327
  (1980s; pitch change in cents from lever position; not opened); US5280129A (Roland, filed
  1992-05-15, [Google Patents](https://patents.google.com/patent/US5280129A/en); the summary says
  it orders released channels by pedal state: damper-on channels before sostenuto-on, after
  hold-off ones, element 4 and 5); US5095800 and US5322967 (early 1990s by number; sustain-pedal
  channel handling; not opened). Velocity scaling of filter cutoff: US5827987A (Kawai, priority
  1996-06-25, [Google Patents](https://patents.google.com/patent/US5827987A/en)), a table of
  resonant frequency and Q indexed by key touch sets filter coefficients (element 3 and 5; opened,
  abstract only).
- **Element 6.** Cleared by ADR-0119 (`docs/research/modulation-patents.md`); not researched.

## Candidates

Patents that might cover an element of the design and might still be in force. Status is filled
in by a human from Google Patents or USPTO Patent Center.

| # | Patent | Assignee | Priority / filing | Est. term end | Element it may touch | Link | Status |
|---|---|---|---|---|---|---|---|
| 1 | US7589272B2, Bandlimited digital synthesis of analog waveforms (publication US20060145733A1; JP counterpart JP2005375217A, priority 2005-12-27) | Korg Inc (inventors Leary, Bright) | 2005-01-03 / 2005-01-03; granted 2009-09-15 | 2028-07-04 (Google, adjusted) | 1 (a sum of a naive saw/square/pulse with an alias-reducing pulse around each discontinuity, "odd symmetric" and "essentially no DC bias" per claims 1, 2, 15) | [GP](https://patents.google.com/patent/US7589272B2/en) || In force |
| 2 | US11202147B2, Audio filter with through-zero linearly variable resonant frequency (see `docs/research/eq-filter-patents.md` row 1) | Rossum Electro-Music LLC | 2018-12-26 / 2019-12-20 | 2039-12-20 (as in that note; term adjustment not seen) | 3 (SVF with trapezoidal integration and modulated cutoff) | [GP](https://patents.google.com/patent/US11202147B2/en) || In force |
| 3 | US8017856B2, Electronic musical instrument (publication US20100077908A1; Japanese priority 2008-250239). Abstract: in a "Unison 2" mode four instrument parts are assigned to the depressed notes and divided among them as evenly as possible. Claims not read. | Roland Corp (inventor Tanaka) | 2008-09-29 / 2009-05-18; granted 2011-09-13 | 2029-07-12 (Google, adjusted) | 4 (marginal: assignment of voices to depressed notes, CPC G10H 1/18; the design has no unison parts and allocates by free, releasing, oldest, so a reading is unlikely) | [GP](https://patents.google.com/patent/US8017856B2/en) || In force |
| 4 | EP2775475A1 / B1 (granted 2017-05-03), In a music synthesizer, diatonic scalar transposition, or pitch bend according to chord type. Abstract: pitch-bend target pitch identified and converted according to chord. Claims not read; no US counterpart seen. | Yamaha Corp (inventor Yamauchi) | 2013-03-06 / 2014-03-05 | 2034-03-05 (Google) | 5 (marginal: pitch bend handling, but chord-based conversion of the bend target; the design scales the bend by a semitone range with no chord logic) | [GP](https://patents.google.com/patent/EP2775475A1/en) || Not in force |
| 5 | US9064485B2, US family member of candidate 4 (pitch bend according to chord type) | Yamaha Corp (inventor Yamauchi) | 2013-03-06 / see GP | 2034-03-06 (Google) | 5 (pitch bend) | [GP](https://patents.google.com/patent/US9064485B2/en) | In force |

Statuses checked by Chris Paro on 2026-11-16: candidates 1, 2, 3 and 5 are in force; candidate 4 (EP2775475B1) is not in force.

Status values: **In force**, **Lapsed (fees)**, **Expired (term)**, **Abandoned**, **Invalidated**,
**Not found**. Record who checked and when, under the table.

Expired by term (filing date + 20 years, plus any printed term adjustment, is in the past), listed
for completeness and needing no check. Expiry dates are as Google Patents prints them (it states
these are assumptions, not legal conclusions); patents granted before mid-1995 run from grant date
or filing, whichever Google applied.

- [US4144789A](https://patents.google.com/patent/US4144789A/en), Kawai, filed 1977-06-06, expiry
  1997-06-06. Digital ADSR generator (element 2).
- [US4426904A](https://patents.google.com/patent/US4426904), Casio, filed 1983-01-05, expiry
  2001-07-28. Exponential digital envelope (element 2).
- [US4481851A](https://patents.google.com/patent/US4481851), Norlin, filed 1981-06-22, expiry
  2001-11-13 (also lapsed 1997 for fees). Oldest-released-key assignment (element 4).
- [US4570520A](https://patents.google.com/patent/US4570520), Kawai, filed 1984-03-19, expiry
  2004-03-19. Oldest-voice kill assignment (element 4).
- [US4703680A](https://patents.google.com/patent/US4703680), Nippon Gakki (Yamaha), filed
  1986-04-23, expiry 2006-04-23. Truncate prioritization (element 4).
- [US4909120A](https://patents.google.com/patent/US4909120), Kawai, filed 1988-08-31, expiry
  2008-08-31. Fast release of the old tone before the new one starts (element 4, steal fade-out).
- [US5159144A](https://patents.google.com/patent/US5159144), Roland, filed 1990-06-29, expiry
  2010-06-29. Channel by smallest residual volume (element 4).
- [US5245129A](https://patents.google.com/patent/US5245129), Yamaha, filed 1990-01-17, expiry
  2010-09-14. Force damp then new tone (element 4, steal fade-out).
- [US5315059A](https://patents.google.com/patent/US5315059), Kawai, filed 1993-02-24, priority
  1990-12-28, expiry 2011-12-27. Channel vacating order (element 4).
- [US5596159A](https://patents.google.com/patent/US5596159A/en), Invision Interactive (Beatnik),
  filed 1995-11-22, expiry 2015-11-22. Software synth dropping voices by softness, pitch or priority
  (element 4).
- [US5981859A](https://patents.google.com/patent/US5981859A/en), Yamaha, filed 1998-09-23, expiry
  2018-09-23. Software tone generator truncating channels on CPU load (element 4).
- [JP3399272B2](https://patents.google.com/patent/JP3399272B2/en), Yamaha, filed 1997-01-14,
  expiry 2017-01-14 (cancelled for fees 2012). Hard sync with window against aliasing (element 1).
- [WO2005015537A1](https://patents.google.com/patent/WO2005015537A1/en), Ericsson, priority
  2003-07-18, "Ceased"; US counterpart US7038119B2 listed "Expired - Fee Related". Wavetable
  synth processing-load control (element 4). The 20-year date 2023-07-18 has passed; printed term
  adjustment not seen.
- [US7728213B2](https://patents.google.com/patent/US7728213B2/en), Stone Family Trust of 1992
  (inventors Stone, Davis), priority 2003-10-10, filed 2006-04-25, adjusted expiry 2025-03-29.
  Dynamic note assignment to a fixed number of voices (element 4).
- [US5786541A](https://patents.google.com/patent/US5786541A/en), Yamaha, filed 1997-05-14, expiry
  2017-05-14. Physical-model synth; a new note during a current note continues the envelope from
  its current level or damps it first (elements 2 and 4).
- [US5827987A](https://patents.google.com/patent/US5827987A/en), Kawai, filed 1997-06-25, expiry
  2017-06-25. Filter coefficients from a key-touch table (element 3 and 5, velocity to cutoff).
- [US5280129A](https://patents.google.com/patent/US5280129A/en), Roland, filed 1992-05-15, expiry
  2012-02-21. Channel assignment by successive-strike relation and pedal state (elements 4, 5).
- [US6806413B1](https://patents.google.com/patent/US6806413), Young Chang, filed 2002-07-31,
  expiry 2022-07-31. Variable-waveshape oscillator with aliasing control (element 1; a
  different technique, not a step correction).
- [US6900381B2](https://patents.google.com/patent/US6900381), Ericsson, filed 2002-05-16, expiry
  2022-05-16. Low-pass against aliasing in wavetable pitch transposition (element 1, different
  technique).

## Claim analysis

### Candidate 1: US7589272B2 (Korg)

Source: Google Patents HTML text of the grant
([US7589272B2](https://patents.google.com/patent/US7589272B2/en)), not OCR. The fetch tool only
returns quotes in short pieces, so each claim below is the tool's pieces joined; the same
wording came back in more than one fetch. 60 claims. Independent: 1, 2, 15, 28, 43, 58, 59, 60.
Legal events on the page: certificate of correction 2010-12-14 (its text not seen), maintenance
fees for years 4, 8 and 12 paid (12th on 2021-03-15). Priority date 2005-01-03.

**What the spec says the pulse is.** "The shape of the alias reducing pulse can be determined by
using a bandlimited impulse signal, called a sinc function"; "A windowing function selects N
samples around the center of the bandlimited impulse"; these "are integrated using a digital
integration filter to produce a bandlimited step function"; "Subtracting an ideal
(non-bandlimited) step function from the bandlimited step function moves the discontinuity from
the end of the signal to the origin", giving a pulse that "starts at zero, ends at zero, has a
discontinuity in the center, and has essentially no DC bias". That is a linear-phase BLEP residual.
The spec gives no formal definition of "odd symmetric" or "DC bias". It says "the alias reducing
pulse for the sawtooth wave reset is odd symmetric about the reset time". It contrasts the pulse
with Brandt: "the minimum-phase bandlimited step has a DC bias and therefore creates a
frequency-dependent DC error". N is free ("a different length N may be used"; one implementation
has four members). Tables ("a lookup table having 10 entries, each shifted in phase by an increment
of 1/10 of the sample period") and interpolation are examples. Polynomials, triangle waves,
oversampling and latency are not mentioned.

**The PolyBLEP residual measured against those terms.** Write the residual as a function of
signed time `x` from the edge, in samples. The design's pair is `r(x) = (1 + x)²` for
`-1 ≤ x < 0` (the `u² + 2u + 1` branch) and `r(x) = -(1 - x)²` for `0 ≤ x ≤ 1` (the `2u − u² − 1`
branch). Then:

- `r(-x) = -r(x)` for every `x`: the pulse is exactly odd symmetric about the edge's phase.
- `r(-1) = r(1) = 0` and it jumps at `x = 0`: it "starts at zero, ends at zero, has a
  discontinuity in the center".
- Its area is `1/3 - 1/3 = 0`: no DC bias, in the sense the spec uses against Brandt (a
  minimum-phase residual has nonzero area).
- Sampled at a fractional edge offset `a` (the after-edge sample sits `a` samples after the edge),
  the two samples are `a²` and `-(1 - a)²`. They are not negatives of each other, and they sum to
  `2a - 1`, unless `a = 0.5`. The same holds for any sampled odd pulse, the patent's own windowed
  sinc included. So the claims' "odd symmetric" and "no DC bias" can only sensibly describe the
  pulse's shape, not each sampled pair. This last point is my reasoning; the spec does not
  discuss it.
- Subtracting the residual is the same as summing with `-r`, which is also odd with zero area.

(Aside, not a claim matter: the pair as written has a jump of 2, so it matches a step of height 2;
"scaled by step height" must mean a scale of height / 2.)

**Claim 1** (method):

| Element | Planned design |
|---|---|
| "A method for generating a bandlimited digital signal in a waveform synthesizer, comprising:" | The synth node's oscillator. |
| "generating, using the waveform synthesizer, a sequence of digital samples representing values of an analog waveform within respective sampling intervals of a waveform having a first fundamental frequency and having a discontinuity with a phase within a particular sampling interval; and" | The naive phase-accumulator saw or pulse at `f`; each wrap (saw) or edge (pulse) falls at a fractional phase inside one sample interval. Met. |
| "summing, using the waveform synthesizer, the sequence of digital samples around the discontinuity with a set of digital samples of an alias reducing pulse, based on the phase of the discontinuity" | The two PolyBLEP samples, one before and one after the edge, computed from `u = t / dt`, i.e. from the edge's phase, and subtracted (summed with `-r`). Met. A 2-member set is a "set"; N is free in the spec. |
| "and having essentially no DC bias, to provide a digital output waveform." | The residual's area is exactly 0. Met, if DC bias means the pulse's area, as the spec's Brandt passage uses it. Not met only on the strained reading that each sampled pair must sum to zero, which the patent's own pulse would fail too. |

Verdict: **Reads** (on the spec's own usage of DC bias).

**Claim 2** (method): the first four elements are as in claim 1 (worded "generating a sequence of
digital samples using the waveform synthesizer ..." and "summing the sequence of digital samples
around the discontinuity using the waveform synthesizer with a set of digital samples of an alias
reducing pulse, based on the phase of the discontinuity to provide a digital output waveform,"),
and all are met as above. Last element: "wherein the alias reducing pulse is odd symmetric relative
to the phase of the discontinuity." Met: `r(-x) = -r(x)` exactly. Verdict: **Reads**.

**Claim 15** (system): "A system for generating a bandlimited digital signal, comprising: data
processing resources, including memory storing instructions adapted for execution by the data
processing resources," is met by a computer running Debussy. Its logic "generates a sequence of
digital samples representing values of an analog waveform within respective sampling intervals of
a waveform having a first fundamental frequency and having a discontinuity with a phase within a
particular sampling interval; and sums the sequence of digital samples around the discontinuity
with a set of digital samples of an odd-symmetric alias reducing pulse, based on the phase of the
discontinuity, to provide a digital output waveform." Met as in claim 2. Verdict: **Reads**.

**Claim 60** (method): the elements are as claim 2 (pulse "odd symmetric"), and then "transducing
the digital output waveform into an analog waveform." The software does not do this itself; the
audio interface's DAC does it whenever the synth is heard. The pulse elements are met. Verdict:
**Reads** when output is played; who performs the last step is a legal question (Open uncertainties).

**Claims 28, 43, 58, 59** (hard sync). Each needs a first waveform with "a first waveform event with
a first phase", and a second waveform whose discontinuity has "a second phase [that] is a function
of the first phase", "first and second magnitudes before and after the discontinuity, respectively,
that are a function of the second phase", and a pulse "based on the second phase and the first and
second magnitudes". 28 and 43 also recite "an odd symmetric alias reducing pulse"; 58 and 59 do not.
The design's two oscillators per voice run free and are only summed; neither resets the other, and
each pulse wave comes from one phase accumulator, not from a second waveform. Missing element:
"the second phase is a function of the first phase" (with its magnitudes). Verdict: **Does not
read**. Adding hard sync later would need a fresh reading: claims 58 and 59 would cover even a
residual that is not odd symmetric.

**Dependent claims** (they do not change the verdict, since the independents read). Of claims
3-14, the ones on 2: 3 (detect the phase), 4 (pulse from the "difference between the first and
second magnitudes", i.e. the step height), 5 (sawtooth), 6 (square), 7 (pulse) and 12 ("slope that
is substantially equal on both sides of the discontinuity") are met. 8 and 9 (table look-up, with
or without interpolation) are not, since the design evaluates the polynomial directly. 10 and 11
(a buffer, or a FIFO "having a length of at least N−1") are met only if the implementation delays
the output to correct the sample before the edge, rather than predicting the edge one sample
ahead. 13 and 14 (transducing) are met on playback. 16-27 mirror these for claim 15.

**Triangle (element 1, two options).**

- *PolyBLAMP.* A triangle has no value discontinuity, only slope changes. The claims recite a
  "discontinuity" with magnitudes before and after, and claim 12's "slope that is substantially
  equal on both sides of the discontinuity" implies a value step. The PolyBLAMP residual is the
  integral of `r`: `(1 + x)³ / 3` before the corner and `(1 - x)³ / 3` after it. That is even, not
  odd, and its area is 1/6 per unit slope change, not zero. It fails the discontinuity element
  and also both "odd symmetric" (2, 15, 60) and "essentially no DC bias" (1). **Does not read.**
- *Leaky integral of a PolyBLEP square.* The square is made by the method of claims 2 and 6 (and 1,
  15, 19, 60) before it is integrated. **Reads**, through the square.

**Design-arounds** (each judged only against the independent claims above):

| Alternative | Claim 1 "essentially no DC bias" | Claims 2, 15, 60 "odd symmetric" | Other elements | Avoids? |
|---|---|---|---|---|
| Minimum-phase residual (Brandt's MinBLEP, table of a min-phase integrated windowed sinc, minus the step) | Not met: the spec states the min-phase step "has a DC bias" | Not met: a min-phase residual is causal, not odd | Summing a pulse around the discontinuity is still met | Yes, for 1, 2, 15, 60. Practises Brandt 2001, which predates the priority date. Cost: the frequency-dependent DC error the spec names (a DC blocker removes it), plus a table. Claims 58 and 59 would cover it if hard sync were added. |
| Causal or asymmetric polynomial residual (corrections only on samples at and after the edge, polynomial fitted to a min-phase step) | Not met if its area is clearly nonzero | Not met | Met | Probably yes, but "essentially" and "substantially" set no threshold; a residual that is nearly odd with nearly zero area would be Unclear. No published source for it was found, so it would be an in-house design. |
| Higher-order or integrated-polynomial BLEP residuals with a symmetric kernel (Välimäki et al. 2012; 4-point B-spline BLEP) | Met (zero area) | Met (odd) | Met | **No.** These are the same claimed structure. |
| DPW, differentiated polynomial waveforms (Nam et al. 2010) | No pulse is summed | No pulse | "summing ... with a set of digital samples of an alias reducing pulse" is literally absent: the polynomial of the naive waveform is differentiated | Literally yes. Second-order DPW equals a half-sample-delayed naive saw plus a correction near the edge, so an equivalence argument cannot be ruled out (Unclear). |
| BLIT integrated into saw or square (Stilson and Smith 1996) | No naive waveform is summed with a pulse | — | Summing element absent | Yes. Predates the priority date. Integrator DC drift, which the spec itself notes ("DC offsets at startup"). |
| Oversampling the naive waveform, then decimating with the in-house polyphase resampler (ADR-0036) | No pulse | — | Summing element absent | Yes. Costs CPU per voice. |
| Band-limited wavetables (one table per octave or less) | No pulse | — | Summing element absent | Yes. Costs memory, plus crossfades between tables. |

**Prior art predating 2005-01-03:** Brandt 2001 (BLEP and MinBLEP; the patent cites it and says
it "describes a theoretical method of adding a minimum-phase bandlimited step"), Stilson and Smith
1996 (BLIT), and JP3399272B2 (Yamaha, filed 1997, windowed hard sync). Whether Brandt 2001 also
describes a linear-phase (odd, zero-area) BLEP, which would bear on validity, is not established:
the paper was not read. The PolyBLEP papers (Välimäki and Huovilainen 2007; 2010; 2012) postdate
the priority date. Validity is not the question here.

**Verdict: Reads** (claims 1, 2, 15, 60 on element 1's saw, pulse and leaky-integrated triangle).
**Does not read** claims 28, 43, 58, 59 (no hard sync), nor the PolyBLAMP triangle.

### Candidate 2: US11202147B2 (Rossum)

Not reread; verdict carried over from `docs/research/eq-filter-patents.md` (Summary row 1 and its
Claim analysis): its claims need a resonant frequency modulated through zero into negative
frequency. The synth modulates `dsp::Svf`'s cutoff every sample, in log2 Hz (positive for every
finite value), and clamps it below 0.49·fs; it never reaches zero or a negative frequency.
**Does not read.** Per-sample modulation does not change this, since the eq-filter design already
recomputed coefficients every sample.

### Candidate 3: US8017856B2 (Roland)

Source: Google Patents HTML text of the grant
([US8017856B2](https://patents.google.com/patent/US8017856B2/en)), second text window (offset
100000). The page does not say whether the text is OCR. It has typos ("timbers" in the claims,
"Unites States" in the description) that may be in the grant itself or may come from OCR, so treat
the quotes as possibly OCR. The fetch tool returns quotes of under 125 characters, so each claim
below is its pieces joined. Claim 1 came back whole in consecutive pieces. Claims 10 and 14 came back
as element lists, so a connecting word between pieces may be missing. Independent claims: 1, 10, 14,
16, 20, 22, 26. Status on the page: Active, adjusted expiry 2029-07-12. Priority date 2008-09-29
(JP 2008-250239).

**What a "part" is.** In the spec, each part is an instrument part with its own timbre: "different
timbres are set according to the respective parts", for example trumpet, clarinet, alto saxophone
and trombone as Parts 1 to 4, ordered by pitch. The aim is that "parts are generally equally
assigned to notes". The spec never mentions voice stealing, fading out, or tone generator channels.
(These spec passages are as the fetch tool quoted them.)

**Claim 1** (instrument):

| Element | Planned design |
|---|---|
| "An electronic musical instrument comprising:" | Debussy running the synth node. Assume met. |
| "an input device that inputs a sound generation instruction that instructs to start generating a musical sound at a predetermined pitch and a stop instruction that instructs to stop the musical sound being generated by the sound generation instruction;" | MIDI note-on and note-off (CoreMIDI or clip playback). Met. |
| "a sound generation control device receiving the sound generation instructions from the input device and performing:" | The voice allocator. Met. |
| "assigning a plurality of parts to the musical sound at the predetermined pitch whose sound generation is instructed by the sound generation instruction inputted by the input device, wherein the parts assigned to the musical sound generate the musical sound with predetermined timbres;" | **Not met.** Each note gets exactly one voice. There is no unison or stacking, and all voices play the node's single patch. |
| "receiving an additional sound generation instruction to generate an additional musical sound while generating the musical sound; and" | A new note-on while others sound. Met. |
| "reassigning the parts to the additional musical sound and the at least one musical sound being generated based on a number of the musical sounds to generate." | Partly. When all 16 voices are busy (a count), one voice is stolen and given to the new note. But the notes still sounding are not given parts again: they keep their voices or lose them. Even if this were met, the element before it is not. |

Verdict: **Does not read**. Missing: "assigning a plurality of parts to the musical sound".

**Claim 10** (method; claims 16 and 22 have the same steps, and claim 16 adds "from the input device"):

| Element | Planned design |
|---|---|
| "A method implemented in an electronic musical instrument for generating electronic musical sounds, comprising:" | As claim 1. Assume met. |
| "receiving inputs of sound generation instructions to generate musical sounds at predetermined pitches" | MIDI note-on. Met. |
| "assigning at least one of a plurality of parts comprising different timbers to each of the musical sounds to generate in response to receiving an input for a musical sound to generate" | **Not met.** The pool's 16 voices are identical copies of one patch, not "parts comprising different timbers" (timbres). One voice is assigned per note. |
| "for each musical sound, generating the at least one part at the predetermined pitch ... wherein the musical sounds that are generated include at least one of musical sounds previously generated that are continuing and the musical sound for the received input for which the assigning was performed" | Each voice plays its note's pitch, and held notes keep sounding. Met if a voice counted as a part. |
| "receiving an additional sound generation instruction to generate an additional musical sound while generating the musical sounds according to the assignment of the parts" | Met, as claim 1. |
| "reassigning the parts to the additional musical sound and the musical sounds being generated based on a number of the musical sounds to generate" | Partly, as claim 1. Stealing gives one voice to the new note when the pool is full, and does not hand out parts again among the notes still sounding. |

Verdict: **Does not read**. Missing: "a plurality of parts comprising different timbers". If a court
read "parts" as covering identical voices, the reassigning element would decide it. Whether
stealing one voice counts as "reassigning the parts to ... the musical sounds being generated" is
then a construction question. I judge that it does not (see Open uncertainties).

**Claim 14** (method; claims 20 and 26 have the same steps, and claim 20 adds "from the input
device"): after the preamble and "receiving inputs of sound generation instructions to generate
musical sounds at predetermined pitches;" (met), it requires "assigning at least one of a plurality
of parts comprising different timbers to each of the musical sounds to generate in response to
receiving an input for a musical sound to generate by:" then "determining whether a number of the
musical sounds is less than a number of the plurality of parts; and" then "assigning multiple parts
to at least one of the musical sounds to generate for the at least one musical sound in response to
determining that the number of musical sounds is less than the number of the parts," "wherein at
least one of the musical sounds is assigned more parts than assigned to at least one of the other
musical sounds, and wherein each musical sound is assigned a different subset of the parts; and"
then the generating step as in claim 10. The design never compares the number of notes with the
number of voices to give spare voices to a note, and never gives a note more than one voice. Missing:
"assigning multiple parts to at least one of the musical sounds" (and the different-timbre parts, as
claim 10). Verdict: **Does not read**.

**Prior art predating 2008-09-29:** everything listed under Prior art for element 4 (US4481851A
1981, US4570520A 1984, US4703680A 1985, US4909120A 1987, US5245129A 1989, US5159144A 1990). Those
are about plain voice allocation and stealing, not the unison part division that the claims are
about. Validity is not the question here.

**Verdict: Does not read** (all seven independent claims; High confidence). Any future unison,
stacked or layered voices per note, or several timbres split across held notes, would need a fresh
reading before 2029-07-12.

### Candidate 4: EP2775475B1 (Yamaha)

Source: Google Patents HTML text of the B1
([EP2775475B1](https://patents.google.com/patent/EP2775475B1/en)), English claims, second text
window (offset 100000). The page does not say whether the text is OCR. The description has
transcription errors ("beast" for "beat", "dose not"), so treat it as possibly OCR. The claim pieces
quoted below read cleanly. Claim 1 came back whole in consecutive pieces of under 125 characters.
Claims 10 and 11 came back only as pieces, which the tool said mirror claim 1, so section becomes
step. Reference numerals are left out below. Independent claims: 1, 10, 11. Priority 2013-03-06
(JP2013044252A), filed 2014-03-05, granted 2017-05-03. **The page lists the legal status as
"Not-in-force"** (Google's assumption), which disagrees with the Status column. No lapse,
opposition or other event was shown.

**US family.** The "Country Status" list on the page shows US9064485B2 (Yamaha, inventor Yamauchi,
filed 2014-03-06, priority 2013-03-06, granted 2015-06-23; Google: Active, expiry 2034-03-06;
publication US20140251115A1), plus CN104036764B and JP6175812B2. I opened
[US9064485B2](https://patents.google.com/patent/US9064485B2/en). Its independent claims 1, 10
and 11 recite, among other things, "a chord information acquisition section configured to acquire
chord information designating a chord", "a determination section configured to determine whether
one tone indicated by the tone information" (continues: varies continuously from a first to a second
note pitch, as relayed by the tool), and "converts the first note pitch and the second note pitch
independently of each other". I saw these only as short pieces, not whole claims. They lack the
same elements as the EP claims below, so it **does not read** for the same reason (Medium
confidence, since the claims were not seen whole).

**Claim 1** (apparatus):

| Element | Planned design |
|---|---|
| "A tone information processing apparatus comprising: a tone information acquisition section ... configured to acquire tone information indicative of a tone having a pitch element; and" | MIDI note number and pitch-bend messages reaching the synth node. Met. |
| "a chord information acquisition section ... configured to acquire chord information designating a chord," | **Not met.** The design has no chord input or chord detection. |
| "characterized in that said tone information processing apparatus further comprises: a determination section ... configured to determine whether one tone indicated by the tone information, acquired by said tone information acquisition section, has such a pitch variation that a pitch of the tone continuously varies at least from a first note pitch to a second note pitch, different from the first note pitch, for a particular time period within a sounding time period of the tone; and" | **Not met.** The design applies the current bend value as `bend · range` semitones and never classifies a tone as gliding from one note pitch to another. |
| "a pitch conversion section ... configured to convert a pitch of the acquired tone information on the basis of a predetermined pitch conversion rule and the chord designated by the chord information acquired by said chord information acquisition section, wherein, when it is determined that the one tone indicated by the tone information has the pitch variation varying continuously at least from the first note pitch to the second note pitch, said pitch conversion section determines at least a first target note pitch corresponding to the first note pitch and a second target note pitch corresponding to the second note pitch independently of each other on the basis of the predetermined pitch conversion rule and the chord designated by the acquired chord information so that each of the determined target note pitches matches the chord designated by the acquired chord information, and converts at least the first note pitch and the second note pitch to the determined first and second target note pitches respectively." | **Not met.** The pitch is `440 · 2^((note − 69 + bend · range) / 12)`. No conversion rule, no chord, no target pitches. |

Verdict: **Does not read**. Missing: "a chord information acquisition section" (and the
determination and chord-based conversion that depend on it).

**Claim 10** (method, "performed by a processor") and **claim 11** (non-transitory storage medium)
have the same steps as claim 1: "a chord information acquisition step ... of acquiring chord
information designating a chord", "a determination step ... of determining whether one tone
indicated by the tone information" "has such a pitch variation that a pitch of the tone continuously
varies at least from a first note pitch", and "said pitch conversion step determines at least a
first target note pitch corresponding to the first note pitch" "independently of each other on the
basis of the predetermined pitch conversion rule". Missing as in claim 1. Verdict: **Does not
read**.

**Prior art predating 2013-03-06:** US4653374 (Yamaha, granted 1987; per a search summary, bend
scaled by a range in semitones; not opened) and the MIDI 1.0 specification (1983, not opened). Both
are about plain bend, not the claimed chord conversion. Validity is not the question here.

**Verdict: Does not read** (claims 1, 10, 11; High confidence). A future chord-aware or
scale-quantised pitch bend or transpose feature would need a fresh reading of this patent and of
US9064485B2.

## Search log

All entries 2026-11-16. The search tool returns a prose summary, not result lists, so "Hits" are
approximate (about 10 per query unless noted).

| Database | Query | Date | Hits | Result |
|---|---|---|---|---|
| Web (general) | Välimäki Pekonen Nam "Perceptually informed synthesis of bandlimited classical waveforms using integrated polynomial interpolation" 2012 polyBLEP | 2026-11-16 | 10 | JASA 2012 paper, TASLP 2010, Pekonen SPL 2012 |
| Web (general) | Brandt "Hard sync without aliasing" ICMC 2001 BLEP bandlimited step; Stilson Smith "Alias-free digital synthesis of classic analog waveforms" 1996 BLIT (two searches) | 2026-11-16 | 20 | Papers via secondary sources; US7589272 |
| Web (general) | patent band-limited oscillator polynomial band-limited step correction sawtooth pulse wave aliasing reduction synthesizer | 2026-11-16 | 10 | No patent; papers |
| Web (general) | patent polyphonic tone generator voice assignment steal voice in release state lowest envelope level new note allocation | 2026-11-16 | 10 | US4481851, US4570520, US4703680, US5315059, US5159144, EP0204122, EP0675482 |
| Web (general) | Välimäki Huovilainen "Antialiasing Oscillators in Subtractive Synthesis" IEEE Signal Processing Magazine 2007 March | 2026-11-16 | 10 | Citation data only |
| Web (general) | Esqueda Välimäki Bilbao "Rounding Corners with BLAMP" DAFx-16 2016 | 2026-11-16 | 10 | DAFx paper |
| Web (general) | Brandt "Hard Sync Without Aliasing" ICMC 2001 quod.lib.umich.edu | 2026-11-16 | 10 | Paper not found on quod; ResearchGate listing |
| Web (general) | patent musical tone generation voice stolen fade out short time before new note starts click prevention voice assignment truncation | 2026-11-16 | 10 | US4909120, US5245129, US5149903, JPS62111290A, EP0322871, EP0343958 |
| Web (general) | patent envelope generator exponential attack decay release one-pole digital retrigger from current level without reset to zero ADSR synthesizer | 2026-11-16 | 10 | No patent; manuals |
| Web (general) | patent "band-limited step" OR "bandlimited step" OR "BLEP" oscillator waveform discontinuity correction residual added to samples digital synthesizer | 2026-11-16 | 10 | US7589272 only |
| Web (general) | patent anti-aliased oscillator triangle wave slope discontinuity correction band-limited ramp BLAMP integrated residual | 2026-11-16 | 10 | No patent |
| Web (general) | patent software synthesizer polyphonic voice allocation stealing quietest releasing voice oldest note digital audio workstation virtual instrument | 2026-11-16 | 10 | No patent; forums and docs |
| Web (general) | patent velocity controls filter cutoff frequency envelope generator modulates filter cutoff per voice digital synthesizer subtractive | 2026-11-16 | 10 | US4291605, US4344347, US4283984, US4322995 (all by number from the 1980s; not opened except as noted); manuals |
| Web (general, extended) | patents.google.com "voice stealing" synthesizer polyphony limit release phase voice selected for stealing software instrument | 2026-11-16 | 10 | US3986423, EP1653443A1 (not opened), US7935879 (opened; drum micro edits, not relevant; Google expiry 2026-10-20) |
| Web (general) | patent digital envelope generator exponential curve attack decay release rate coefficient recursive multiplication target level musical tone Yamaha OR Roland OR Korg OR Casio | 2026-11-16 | 10 | US4426904, US4144789, US4185532, US4453440, US4421003 (last three not opened) |
| Web (general, extended) | patent software tone generator CPU load polyphony limit sounding voice forcibly muted fade-out new note sounding channel selection after 2005 Yamaha Roland Korg Casio | 2026-11-16 | 10 | US5981859, US5596159, WO2005015537 |
| Web (general, extended) | patent musical sound generating apparatus band-limited oscillator aliasing reduction sawtooth rectangular wave phase accumulator correction waveform near discontinuity Yamaha OR Roland OR Casio OR "Native Instruments" | 2026-11-16 | 10 | US7589272 and US20060145733A1 only |
| Web (general, extended) | Google Patents "alias" oscillator synthesizer "sawtooth" "phase accumulator" correction function discontinuity polynomial patent 2010..2024 virtual analog | 2026-11-16 | 10 | US7589272; no 2010-2024 patent |
| Google Patents (page fetch by number) | US7589272B2 (both text windows), US4481851, US5159144 (both windows), US5315059, US4909120, US5245129, US4426904, US4703680, US4570520, US4144789A, US5981859A, US5596159A, WO2005015537A1, JP3399272B2, JP4614131B2, US7935879, US11087732B2 | 2026-11-16 | 18 | Dates and status read. Not candidates: JP4614131B2 (Casio, additive oscillator allocation; expiry 2025-09-14), US11087732B2 (Rossum, hard sync plus ring modulation, to 2039-12-20; the design has no hard sync or ring modulation) |
| Google Patents (citations) | Cited-by and cited lists of US7589272B2, US4481851, US5159144 | 2026-11-16 | about 45 | Cited-by of US7589272B2: GB2508417B (speech), US11087732B2, US11817069B2 (Rossum resynthesizer, not opened); of US4481851: all 1984-2002 filings; of US5159144: all 1990-1999 filings; none opened beyond those listed |
| Google Patents (query URL, pass 2) | `https://patents.google.com/?q=voice+assignment+release&cpc=G10H1/18&after=priority:20060101` (WebFetch) | 2026-11-16 | 0 | Failed: page is script-rendered, fetch returned only the title "Google Patents" |
| Google Patents (xhr query endpoint) | `patents.google.com/xhr/query?url=cpc=G10H1/18&after=priority:20060101&num=50` | 2026-11-16 | 0 | Refused by the permission classifier; not retried or worked around |
| Espacenet | `worldwide.espacenet.com/3.2/rest-services/search?q=cpc=G10H1/18 and pd>2006` | 2026-11-16 | 0 | HTTP 403 |
| USPTO Patent Public Search | `ppubs.uspto.gov/pubwebapp/external.html?q=(voice near2 steal$).clm.&db=USPAT` | 2026-11-16 | 0 | Failed: script-rendered, empty page |
| FreePatentsOnline | `freepatentsonline.com/result.html?query_txt=ACN/(Yamaha OR Roland OR Korg OR Casio) AND CCL/84/604 AND voice AND release` | 2026-11-16 | 0 | HTTP 403 |
| Justia | `patents.justia.com/assignee/korg-inc?page=2` (WebFetch) | 2026-11-16 | 0 | HTTP 403; a web search summary of this page lists Korg filings to about 2006 only |
| Web (patent-aimed) | patent G10H1/18 voice assignment polyphonic synthesizer note stealing software synthesizer filed after 2006 (extended) | 2026-11-16 | 9 | US7728213B2 (opened: expired 2025-03-29), JP2838645B2, US5376752A, US4998960A (all pre-2000) |
| Web (patent-aimed) | patents.google.com envelope generator exponential one-pole attack decay release retrigger current level musical tone generator patent filed 2008 OR 2012 OR 2016 | 2026-11-16 | 10 | US4426904A, US4079650A, US4344347, RE32726 (all by number pre-1990; not opened); no post-2006 patent |
| Web (patent-aimed) | Yamaha patent voice assignment tone generator polyphony release state lowest level channel selection G10H1/183 2010 2015 2020 | 2026-11-16 | 10 | US7750228 (opened: fragment-based tone data, adj. expiry 2028-05-16, not touching the design), US8035021 (opened: NAND flash waveform streaming, to 2030-04-15, not touching), US10147408 (opened: arpeggiator routing, to 2036-11-01, not touching); all carry G10H 1/183 |
| Web (patent-aimed) | Roland patent sound source polyphonic voice assignment new note sounding sound muted fade out G10H1/18 2012 2018 | 2026-11-16 | 10 | US5280129 (opened, expired), US8017856B2 (via US20100077908; candidate 3), EP0343958, EP2863384A1 (not opened) |
| Web (patent-aimed) | Native Instruments OR Ableton OR Arturia OR Apple patent software synthesizer oscillator envelope voice allocation virtual instrument | 2026-11-16 | 10 | No patent from those assignees; US5596159, US5864080 (Invision), US5376752 |
| Web (patent-aimed) | Korg OR Casio OR Kawai patent oscillator anti-aliasing sawtooth pulse waveform correction digital audio synthesizer 2010..2024 G10H2250/545 | 2026-11-16 | 10 | US20060145733A1 (candidate 1 family), US6806413B1 (opened: expired 2022-07-31); no 2010-2024 hit |
| Web (patent-aimed) | patent sustain pedal damper CC64 hold released notes pitch bend range semitones MIDI synthesizer software sound source | 2026-11-16 | 18 | MIDI reference pages and manuals only; no patent |
| Web (patent-aimed) | "voice stealing" OR "voice allocation" patent 2010..2025 software instrument virtual polyphony MIDI note-on release voice reuse crossfade claims (extended) | 2026-11-16 | 9 | US20060155543A1 (opened: Korg, priority 2005-01-13, filed 2005-07-22, Google status Abandoned, never granted; its description covers ramp-down of stolen voices), US5376752A, US5596159A, US4998960A, US7728213B2 |
| Web (patent-aimed) | patents.justia.com Korg Inc patents synthesizer oscillator envelope tone generation 2018 2020 2022 2024 | 2026-11-16 | 10 | Justia listing page and product pages; no Korg filing after about 2006 seen |
| Web (patent-aimed) | patent virtual analog oscillator aliasing suppression sawtooth rectangular wave "phase" correction waveform data generation Yamaha OR Roland OR Casio G10H7/00 granted 2012..2023 (extended) | 2026-11-16 | 9 | US7589272B2; papers; no new patent |
| Web (patent-aimed) | Casio patent envelope generation electronic musical instrument envelope level recursive coefficient attack decay release 2010 2015 2019 sound source | 2026-11-16 | 10 | US5109746, US4336736, US4079650, US4421003, US4144789, US4426904 (by number, all pre-1995; not opened) |
| Web (patent-aimed) | patent keyboard velocity cutoff frequency scaling touch sensitivity filter envelope modulation digital synthesizer 2010..2024 Moog OR Sequential OR Arturia OR Korg | 2026-11-16 | 10 | US4344347, US4283984, US5827987A (opened: Kawai, expired 2017-06-25) |
| Web (patent-aimed) | patent "envelope" "retrigger" OR "re-trigger" synthesizer note-on during release envelope starts from current level not zero click-free claims 2010..2024 (extended) | 2026-11-16 | 10 | US5786541A (opened: Yamaha, expired 2017-05-14), US20050120870A1 (opened: Ludwig, abandoned application, different subject), US9542923B1 (opened: Roland, band-splitting synthesizer, to 2035-09-29, not touching), US4248122A, US4344347 |
| Web (patent-aimed) | Native Instruments patent synthesizer OR "sound generation" voice oscillator Google Patents assignee "Native Instruments GmbH" G10H | 2026-11-16 | 10 | No Native Instruments patent; CPC scheme pages only |
| Web (patent-aimed) | Apple Inc patent GarageBand OR "Logic Pro" software instrument synthesizer voice note stealing OR oscillator OR envelope G10H | 2026-11-16 | 10 | US9928817 (UI), US7973232B2 (channel strip mapping); none on synthesis; not opened |
| Web (patent-aimed) | patent pitch bend data converted to pitch change range semitones sound source tone generator pitch bend sensitivity electronic musical instrument 2008..2020 | 2026-11-16 | 10 | US4653374, US4813327 (1980s, not opened); EP2775475A1 (opened: Yamaha, candidate 4) |
| Web (patent-aimed) | patent damper pedal sustain note-off held until pedal released tone generator channel assignment sustained tones polyphony limit 2008..2022 | 2026-11-16 | 10 | US5455380, US5095800, US5280129, US5322967 (all pre-2000 by number); US10109268, US10204610 (Casio sympathetic resonance; US10109268B2 opened: filed 2017-12-18, expiry 2037-12-18; it adds resonance to piano samples and does not touch the design) |
| Web (patent-aimed) | Roland OR Yamaha OR Casio patent 2015..2024 oscillator waveform generation aliasing noise reduction analog modeling synthesizer sawtooth square sound source circuit (extended) | 2026-11-16 | 9 | US6900381B2 (opened: expired 2022-05-16), US20030033338A1, US7936192B2 (frequency synthesizer, not audio), US6518801; none by Roland, Yamaha or Casio |
| Web (patent-aimed) | Moog Music OR "Sequential" OR "Dave Smith Instruments" OR Arturia patent synthesizer voice oscillator envelope filter G10H granted | 2026-11-16 | 10 | US4050343, US4291605, US4178822, US3986426 (all 1970s-80s; not opened); nothing recent |
| Web (patent-aimed) | Ableton patent audio instrument MIDI note voice polyphony envelope synthesizer Google Patents | 2026-11-16 | 10 | No Ableton patent; manuals |
| Google Patents (page fetch by number, pass 2) | US7728213B2, US7750228B2, US8035021B2, US10147408B2, US6806413, US20100077908A1 and US8017856B2, US5280129A, US20060155543A1, US5827987A, US9542923B1, US5786541A, US20050120870A1, EP2775475A1, US6900381, US10109268B2 | 2026-11-16 | 16 | Dates and status read from the page header; claims were beyond the truncated text window on every page, so no claim was read |
| Google Patents (stage 2 claim reading) | US7589272B2 claims and description, both text windows (offset 0 and 100000); FreePatentsOnline tried for a cross-check (403) | 2026-11-16 | 1 | All 60 claims read in the fetch tool's short verbatim pieces (HTML text, not OCR); independents 1, 2, 15, 28, 43, 58, 59, 60; spec passages on the pulse's derivation, odd symmetry, DC bias and Brandt; legal events (CoC 2010-12-14, fees paid to year 12) |
| Google Patents (stage 2 claim reading, candidates 3 and 4) | US8017856B2 (offset 0 and 100000: header, spec on "parts", claims 1, 10, 14 and the steps of 16, 20, 22, 26); EP2775475B1 (offset 0 and 100000: header, status, Country Status family list, claims 1, 10, 11); US9064485B2 (offset 0 and 100000: header, independent claims 1, 10, 11 in pieces) | 2026-11-16 | 3 | HTML text, possibly OCR (the page does not say; typos in the text). Quotes come back in pieces of under 125 characters. EP2775475B1 shown as "Not-in-force"; US family member US9064485B2 shown as Active to 2034-03-06 |

Refused or failed: Michigan Publishing page for Stilson and Smith returned 403; earlevel.com
"Envelope generators ADSR code" returned 404; the KAIST JASA 2012 PDF fetch timed out (300 s) and
was not read; no PDF text of any paper was read. In pass 2, Google
Patents' search interface, Espacenet, USPTO Patent Public Search, FreePatentsOnline and Justia
were all tried and none returned a result list (script-rendered, 403 or refused; see the rows
above). So no CPC-filtered or assignee-filtered query ran in a patent database. The "G10H 1/18",
"G10H 7/00" and "G10H 2250/545" searches in pass 2 were web searches with the class named in the
text, plus CPC codes read from the header of each opened Google Patents page. Every candidate still
comes from a web search or a citation list.

## Open uncertainties

- No paper text was read. The dates and the content of Stilson and Smith 1996, Brandt 2001,
  Välimäki and Huovilainen 2007 (does it contain the two-point polynomial residual, and is it the
  earliest?), Nam 2010 and the JASA 2012 paper, and Esqueda 2016 come from search summaries or
  citing documents. Who first published the two-point PolyBLEP residual of element 1, and when, is
  not established.
- No source found for a triangle built as a leaky integral of a PolyBLEP square, or for Huovilainen's
  earlier BLAMP proposal.
- Candidate 1 (Korg US7589272B2, to 2028-07-04) **reads** on the PolyBLEP saw and pulse and on a
  triangle integrated from the PolyBLEP square (stage 2, Claim analysis). For a human to decide:
  (a) raise it with `flag-risk` (tag `patent`), and choose between a design-around (MinBLEP,
  oversampling, band-limited wavetables, BLIT, or DPW, the last carrying equivalence risk), a
  licence, or waiting until 2028-07-04; (b) the triangle should be PolyBLAMP, not a leaky integral
  of the PolyBLEP square, whatever is chosen for saw and pulse; (c) the claim text came from Google
  Patents' HTML via a tool that returns short pieces. Before relying on it, a human should compare
  claims 1, 2, 15 and 60 with the USPTO grant PDF and the 2010-12-14 certificate of correction
  (not seen). (d) Whether "essentially no DC bias" (claim 1) means the pulse's area, as I read the
  spec, or each sampled set's sum is a construction question; claims 2, 15 and 60 read either way.
  (e) Claim 60's "transducing ... into an analog waveform" is done by the user's DAC; who performs
  that step is a legal question. (f) If Brandt 2001 also describes a linear-phase (odd, zero-area)
  BLEP, that bears on validity; the paper was not read. (g) If hard sync is ever added, claims 58
  and 59 cover even a non-odd residual and need a new reading.
- Candidate 2 (US11202147B2) was not reread; the eq-filter note's verdict (the design clamps `fc`
  below 0.49 fs and never reaches zero or negative frequency) is assumed for element 3, including the
  per-sample modulation by envelope, LFO and velocity. If the filter cutoff ever passes through zero
  it needs rereading. No patent on per-sample envelope or velocity cutoff modulation of an SVF was
  found, but only 1980s patents on envelope-controlled filters were returned and not opened
  (US4291605, US4322995, US4344347, US4283984); by their numbers they were granted in the early
  1980s, but I did not read their filing dates.
- Element 4 (pass 2): the only in-force-looking G10H 1/18 hit is candidate 3, marginal. No patent
  filed after 2006 on free-voice, quietest-released, oldest-held stealing or a stolen-voice fade was
  found, but the databases that could list by class (Google Patents search, Espacenet, USPTO) were
  unreachable, so this rests on web searches that return about ten hits and cannot enumerate G10H
  1/18 (reported by a search summary as carrying many Yamaha, Roland and Casio filings). Korg's
  US20060155543A1 (stealing with ramp-down, vector processor) was an abandoned application per
  Google, not seen granted; a granted family member was not looked for beyond that page. Still not
  opened: EP1653443A1, EP0204122, EP0675482, EP0322871, EP0343958, JPS62111290A, US5149903,
  EP2863384A1, US4344347. "No candidate" cannot yet be stated for element 4; a human or a
  session with browser access should run `cpc=G10H1/18` (and G10H1/183) with
  `after=priority:20060101` in Google Patents.
- Element 2 (pass 2): still nothing in any patent on exponential one-pole segments, attack aimed
  past 1.0, or retrigger from current level; no post-2006 envelope patent surfaced. US5786541A
  (Yamaha, expired 2017) continues an envelope from its current level. G10H 1/057 and 1/0575 could
  not be searched as classes. The manuals for retrigger remain undated.
- Element 5 (pass 2): no in-force patent found on 12-TET pitch, bend scaled by a semitone range,
  velocity scaling or CC 64 hold; only expired patents (US5280129A, US5827987A) and unopened 1980s-90s
  ones (US4653374, US4813327, US5095800, US5322967). Candidate 4 (Yamaha EP2775475, chord-based bend)
  is marginal. US10109268 and US10204610 (Casio, damper-pedal resonance for piano samples) were
  judged not to touch a plain CC 64 hold, but US10204610 was not opened. The MIDI 1.0 specification
  was not opened, so the dates of CC 64 and pitch bend (1983) are not verified, and there is no
  primary source for the 12-TET formula.
- Element 1 (pass 2): no band-limited-oscillator patent after 2006 was found beyond US7589272B2; US6806413B1
  and US6900381B2 are other aliasing techniques and expired. Yamaha, Roland, Casio, Korg, Native
  Instruments, Arturia, Moog and Sequential assignee searches returned nothing in this area, but
  were web searches only, with no class listing (G10H 2250/545, G10H 7/00-7/12 not enumerated).
- Assignee searches for Native Instruments, Ableton, Apple, Arturia, Moog and Sequential returned
  no patent on synthesis; this shows only that web search does not surface them. No Ableton or Native
  Instruments patent database search was possible.
- Candidate 3 (US8017856B2) **does not read** (stage 2). For a human: (a) if "parts comprising
  different timbers" (claims 10, 16, 22) were read to cover identical voices of one patch, the
  claim would turn on whether stealing one voice for a new note is "reassigning the parts to the
  additional musical sound and the musical sounds being generated based on a number of the musical
  sounds". I read it as not, since the held notes are not given voices again. (b) The claim text is
  Google's HTML, in short pieces, and possibly OCR. Claims 10 and 14 came back as element lists.
  Compare them with the USPTO grant PDF before relying on them. (c) Adding unison, layered patches
  or multitimbral splits before 2029-07-12 needs a new reading.
- Candidate 4 (EP2775475B1) **does not read** (stage 2), and Chris Paro found it not in force
  (2026-11-16). (b) Its US family member US9064485B2 is candidate 5, in force. Its independent
  claims also need chord information and a glide determination, but they were seen only in short
  pieces; confirming them from the USPTO text remains open. (c) The EP claims were read in English only, from Google's HTML, possibly OCR; claims
  10 and 11 only as pieces. The authentic text is the EPO B1 publication, which was not opened.
- Term ends are as Google prints them (Google says they are assumptions). Candidate 1's adjusted
  date 2028-07-04 was not checked against the USPTO record; terminal disclaimers and fee lapse
  are for the human check of Status.
- WO2005015537A1's US counterpart US7038119B2: its printed term adjustment was not seen.
