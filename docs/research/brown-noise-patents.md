# Patent status of the in-house brown-noise generator (leaky integrator)

Research note, 2026-11-26. This is not legal advice. It records what primary sources show, so a
reviewer can check it. Stage 1 listed the candidates; a human filled in **Status**; stage 2
(2026-11-26) read the independent claims of the five in-force candidates, rows 1 to 5. For issue
#207, following the precedent note [pink-noise-patents.md](pink-noise-patents.md).

## Planned design

Software, portable scalar Rust, in the `dsp` library crate of an audio editor. The generator feeds
the synthesizer's noise oscillator (the real-time audio path).

1. **White source.** The existing seeded xorshift64\* generator (cleared in
   [prng-patents.md](prng-patents.md) / ADR-0035), uniform values in `[-0.5, 0.5)`.
2. **Leaky integrator.** The white samples pass through one first-order IIR section
   `y[n] = x[n] + a·y[n-1]` (`H(z) = 1 / (1 - a z^-1)`), one real pole `a = exp(-2π · 5 Hz / fs)`
   inside the unit circle (the same 5 Hz corner as the pink generator's lowest pole). The magnitude
   falls at -6 dB/octave above 5 Hz and is flat below it, so the output does not wander (no
   unbounded random walk) and, its input being zero-mean, its output is zero-mean (DC-free in
   expectation). No clamping, reflection or wrap of a random walk.
3. **Normalisation.** One gain computed at construction from the analytic response so `|H| = 1` at
   1 kHz (clamped to a quarter of the sample rate), the same reference as the pink generator, so
   brown and pink are level-matched at 1 kHz.
4. **Seeding.** `new(seed, sample_rate)`: the same seed and rate always give the same sequence. If
   the rate is 0 or too low for the 5 Hz pole, the output is the white source unfiltered.

Not in scope: random walk with bounds or reflection, Voss-McCartney, FFT spectral shaping,
fractional differencing, multi-pole brown approximations.

## Summary

Five candidates have a printed term end in the future, and a human found all five in force (see
Candidates). Stage 2 verdicts: rows 1 to 5 all **Does not read** (see Claim analysis). None claims
a brown-noise generation method: row 1 claims a white cane with a noise generator at its distal
tip (claims 13 and 19 name "red or Brownian noise" only as a member of a group), row 2 a room
masking filter determined from the room's measured acoustical response and ambient noise, row 3
masking noise generated in response to detected silence and measured electrical noise and combined
with an audio signal, rows 4 and 5 height-adjustable desks (no claim of either recites sound or
noise; brown noise is only in their shared description). No patent was found that claims
a one-pole or leaky-integrator generator of brown or red noise, a normalised coloured-noise
generator for a synthesizer, or a DC-free Brownian generator. The technique itself (white noise
through a first-order low-pass or leaky integrator, -6 dB/octave) is described in non-patent
sources dated 1963 (AR(1) "red noise", by index record), 1998 (Bourke, second-hand), 2003 (Nord
Modular book; Haskell `dsp` library), 2005 (Plaszczynski) and 2011 (Smith), see "Prior art". This
rests on web searches and Google Patents page fetches only; no patent database query interface was
usable (see "Refused or failed"). "No candidate found" means the searches in the Search log found
none, not that none exists.

| Technique | Verdict | Confidence |
|---|---|---|
| 1. White source (xorshift64\*, uniform) | Out of scope here; cleared in [prng-patents.md](prng-patents.md). | Not assessed |
| 2. Leaky integrator (one real pole at 5 Hz, -6 dB/octave, zero-mean) | Does not read: row 1 (no white cane, no elongate body, no noise generator located at a distal tip), row 2 (no mask filter determined from a room's measured acoustical response and ambient noise), row 3 (no detection of silence in an audio signal, no measurement of electrical noise, no combining with an audio signal), rows 4 and 5 (no worktop, drive mechanism, presence sensor or desk; no claim recites sound). No other candidate found claiming it (searches 1 to 8, 12). Described in non-patent prior art from 1963 to 2011. | Medium (claims read via fetch summaries, see Open uncertainties); Low for unseen patents |
| 3. Normalisation to unity at 1 kHz | No candidate found (searches 4, 7); trivial step, no prior art sought. Rows 1 to 5 do not read on the design as a whole (above). | Low |
| 4. Seeding; white fallback when the pole does not fit | No candidate found (searches 1, 7); see prng-patents.md for seeding. No independent claim of rows 1 to 5 recites seeding. | Low |

## Prior art

Items marked (unverified) come from a search summary or an index, not from the publication itself.

- **Element 2 (white noise through a first-order low-pass / leaky integrator, -6 dB/octave).**
  - D. L. Gilman, F. J. Mitchell and J. M. Gilman, "On the Power Spectrum of 'Red Noise'", *Journal
    of the Atmospheric Sciences* 20(2), 182 to 184, 1963, DOI
    10.1175/1520-0469(1963)020<0182:otpson>2.0.co;2 (bibliographic record from a
    [library catalogue entry](https://www.katalog.fh-zwickau.de/Record/ai-49-aHR0cDovL2R4LmRvaS5vcmcvMTAuMTE3NS8xNTIwLTA0NjkoMTk2MykwMjA8MDE4MjpvdHBzb24-Mi4wLmNvOzI/Details)
    as returned by search; the DOI redirected to the AMS site and the paper was not read, so that it
    describes a first-order recursive spectrum is not established here) (unverified).
  - P. Bourke, "Generating noise with different power spectra laws", October 1998, as cited by the
    [Wikipedia article on brown noise](https://en.wikipedia.org/wiki/Brownian_noise); the page
    itself was not seen (unverified).
  - J. Clark, *Nord Modular* book, chapter 4 "Noise Generation", (c) 2003
    ([page](https://cim.mcgill.ca/~clark/nordmodularbook/nm_noise.html)): the synthesizer's Noise
    module has a "color" control that sets the cutoff of a first-order low-pass filter on the noise;
    at its maximum the cutoff is about 20 Hz and the spectrum falls 6 dB/octave, "commonly known as
    'brown noise'". This is a description of a synthesizer module and the nearest match to the
    design in a synthesizer; the Nord Modular manual itself, and the product's release date, were
    not seen.
  - M. Donadio, Haskell `dsp` library, module `Numeric.Random.Spectrum.Brown`, copyright 2003
    ([docs, dsp-0.2.5](https://hackage.haskell.org/package/dsp-0.2.5/docs/Numeric-Random-Spectrum-Brown.html)):
    "brown noise, which is integrated white noise" (a pure integrator, no leak). The year is the
    copyright line of the page; no release date is shown.
  - S. Plaszczynski, "Generating long streams of 1/f^alpha noise", arXiv:astro-ph/0510081, v1
    2005-10-04 ([abstract](https://arxiv.org/abs/astro-ph/0510081)): for alpha = 2 the process is a
    random walk that "can be obtained simply by a first order filtering of white noise".
  - J. O. Smith, *Spectral Audio Signal Processing*, W3K Publishing, 2011 (ISBN 978-0-9745607-3-1),
    ["Filtered White Noise"](https://ccrma.stanford.edu/~jos/sasp/Filtered_White_Noise.html): the
    power spectral density of filtered white noise is the squared magnitude response times the
    variance; "brown noise: Filter amplitude response G(f) is proportional to 1/f". The page as
    fetched shows a 2026 copyright line, so the 2011 date of this passage is the book's, not the
    page's, and is not established for the passage.
  - Audacity forum thread ["A Study in Pink"](https://forum.audacityteam.org/t/a-study-in-pink/28026),
    posts of 2013-01-13 to 2013-01-21: a lowest-order low-pass "has a roll off of 6 dB per octave,
    which produces 1/f^2 noise, so called 'brown' noise", and a quoted one-pole recursion
    `y[n]=a0*x[n] + b1*y[n-1]` from Audacity's brown noise code. The thread links musicdsp.org only
    to the pink coefficients.
  - [Wikipedia, Brownian noise](https://en.wikipedia.org/wiki/Brownian_noise) (undated, as fetched):
    leaky integration, multiplying the previous sample by a decay factor just under one, gives noise
    that is flat at low frequencies and "red" above a cutoff set by the decay factor (the same
    construction as the planned design). Not a dated primary source.
  - Patent literature that states one-pole low-pass filtering of white noise as known, all expired:
    US4327419A (Kawai, priority 1980-02-22), whose description lists spectral relations for pink
    noise as white noise through a one-pole low-pass filter (per a search summary, unverified), and
    US4270430A (Kawai, priority 1979-11-19), which cites a 1973 shift-register-plus-filter method
    (Keele) for pink noise. See "Expired by term".
- **Leaky integrator as a filter.** J. O. Smith's Faust `filter.lib` (a copy in a
  [git mirror](https://scm.cri.ensmp.fr/git/Faustine.git/blob_plain/fb68e6e549e4b9bfd6c7a98297f5993770fad640:/interpretor/faust-0.9.47mr3/architecture/filter.lib),
  Faust 0.9.47) names the one-pole feedback filter `y(n) = x(n) + p·y(n-1)` a "leaky integrator"
  (per a search summary, not read in the file; unverified). The file's date was not established.
- **Element 1 (white source).** See [prng-patents.md](prng-patents.md).
- **Element 3 (normalisation).** No source sought; open uncertainty. The 2011 Smith page above
  gives the response of filtered white noise analytically, which is what the gain is computed from.
- **Element 4 (seeding; white fallback).** Deterministic seeding: see
  [prng-patents.md](prng-patents.md). The unfiltered fallback is a guard, not a technique; no
  source sought.

## Candidates

Patents that might cover an element of the design and might still be in force. Status is filled
in by a human from Google Patents or USPTO Patent Center. Dates are as Google Patents prints them
(Google states that its status and expiry fields are assumptions, not legal conclusions). "Element
it may touch" is from stage 1 (abstract and passages quoted); the independent claims of rows 1 to 5
were read in stage 2, see Claim analysis.

| # | Patent | Assignee | Priority / filing | Est. term end | Element it may touch | Link | Status |
|---|---|---|---|---|---|---|---|
| 1 | US9789024B2, White cane navigational device for the visually impaired | Individual (inventor Eric J. Alexander) | 2014-11-03 / 2015-11-03 | 2035-11-03 (Google; no adjustment shown; printed "Active", 8th-year fee paid 2025) | 2 (loosely: independent claims 13 and 19 recite a noise generator emitting noise "selected from the group consisting of white noise, pink noise, red or Brownian noise, and grey noise"; the description defines red or Brownian noise as "power density that decreases by 6 dB per octave"; no generation method seen). Stage 2: **Does not read** (independent claims 1, 13, 19) | [GP](https://patents.google.com/patent/US9789024B2/en) | In force |
| 2 | US7460675 (B2), Auto-adjusting sound masking system and method | Soft DB Inc. | 2004-06-21 / 2004-06-22 | 2027-01-09 (Google, "Adjusted expiration"; printed "Active") | 2 (loosely: claim 1 recites "filtrating a white noise to yield the target masking noise"; white-noise generator plus mask filter; no brown or pink noise in claims or description, per fetch summary). Stage 2: **Does not read** (independent claims 1, 7) | [GP](https://patents.google.com/patent/US7460675) | In force |
| 3 | US8885844B2, Variable noise masking during periods of substantial silence | Continental Automotive Systems Inc. (current assignee per Google: Aumovio Systems Inc.) | 2008-10-23 / 2012-04-12 (divisional of 12/256,574) | 2029-04-14 (Google, adjusted; printed "Active") | 2 (very loosely: masking noise may be white noise whose spectrum "may be shaped using the filter 255"; claims 7 and 8 recite shaping the spectrum of the masking noise; no brown or pink noise mentioned). Stage 2: **Does not read** (independent claims 1, 9) | [GP](https://patents.google.com/patent/US8885844B2/en) | In force |
| 4 | US10827829B1, Height adjustable support surface and system for encouraging human movement and promoting wellness | Steelcase Inc (original and current, per Google) | 2012-10-10 / 2017-02-27 (application 15/442,947; also claims 14/035,974 of 2013-09-25 and 14/612,302 of 2015-02-03) | 2033-09-25 (Google "Anticipated expiration"; no adjustment or terminal disclaimer shown; printed "Active") | 2 (loosely: description mentions brown or pink noise from a sound generator controller). Stage 2: **Does not read** (independent claims 1, 21, 23; no claim recites sound or noise) | [GP](https://patents.google.com/patent/US10827829B1/en) | In force |
| 5 | US11918116B1, Height adjustable support surface and system for encouraging human movement and promoting wellness | Steelcase Inc. (original and current, per Google) | 2012-10-10 / 2020-10-09 (application 17/067,126, a continuation of 16/810,353) | 2033-12-07 (Google "Adjusted expiration"; no adjustment figure or terminal disclaimer shown; printed "Active") | 2 (loosely: as row 4). Stage 2: **Does not read** (independent claims 1, 20; no claim recites sound or noise) | [GP](https://patents.google.com/patent/US11918116B1/en) | In force |

Status values: **In force**, **Lapsed (fees)**, **Expired (term)**, **Abandoned**, **Invalidated**,
**Not found**. Record who checked and when, under the table.

Statuses checked by Chris Paro on 2026-11-26 in Google Patents: rows 1 to 5 all **In force**. Rows 4
and 5 were added at that check, from the B1 pages the stage-1 fetches had missed (the B2 URLs
returned 404).

Expired by term (filing date + 20 years, plus any printed term adjustment, is in the past), listed
for completeness and needing no check:

- [US4327419A](https://patents.google.com/patent/US4327419A/en), Kawai Musical Instruments, filed
  1980-02-22, expiry 2000-02-22 (dates as in pink-noise-patents.md). Digital noise generator for
  musical instruments; its description relates pink noise to white noise through a one-pole
  low-pass filter (element 2, loosely).
- [US4270430A](https://patents.google.com/patent/US4270430A/en), Kawai Musical Instruments, filed
  1979-11-19, Google anticipated expiration 1999-11-19 (printed "Expired - Lifetime"). Noise
  generator for a polyphonic tone synthesizer; pink noise by formant shaping, no filters; brown
  noise not mentioned (element 2, loosely).
- [US5566678](https://patents.google.com/patent/US5566678A/en), Cadwell Industries, filed 1995-01-05
  (priority 1993-09-10), Google anticipated expiration 2016-11-30 (printed "Expired - Fee
  Related"). Digital EEG noise synthesiser; white noise multiplied and low-pass filtered with
  controlled cutoffs; brown, red and 1/f^2 not mentioned (element 2, loosely).
- [US5728963A](https://patents.google.com/patent/US5728963A/en), Reintjes (individual), filed
  1996-06-22, Google anticipated expiration 2016-06-22 (printed "Expired - Fee Related", lapsed
  2006-03-17). Music synthesizer with a pink noise generator into a voltage-controlled low-pass
  filter (element 2, loosely).
- [US7251301](https://patents.google.com/patent/US7251301), Industrial Technology Research
  Institute, filed 2002-10-01, Google adjusted expiry 2025-01-17 (dates as in
  pink-noise-patents.md). Its background discusses low-pass filtering white noise for pink noise.

Seen and not listed as candidates (not in force or not touching any element; term status as
Google printed it):

- WO2015187345A1 (Plantronics, priority 2014-06-02), printed "Ceased", anticipated expiration
  2016-12-02, and its US application
  [US20150348530A1](https://patents.google.com/patent/US20150348530A1/en), printed "Abandoned", no
  patent number issued. Headset noise masking; dependent claims recite pink or brown noise
  (per a search summary of the US application; the WO claims were not in the part of the page read).
  Not a candidate while no granted patent is shown; a human may check USPTO Patent Center for a
  later continuation.
- [US20170025113A1](https://patents.google.com/patent/US20170025113A1/en) (Sound Barrier LLC,
  hunting-area noise), printed "Abandoned"; its parent US9536514B2 printed "Expired - Fee Related".
  Brown noise named only in a list of non-flat spectra.
- [US20030103632A1](https://patents.google.com/patent/US20030103632A1/en) (individual inventors),
  printed "Abandoned" 2004-02-25; white noise only.
- US10827829 and US11918116 (height-adjustable support surface): moved to Candidates as rows 4
  and 5 at the human status check (their B1 pages; the B2 URLs tried in stage 1 returned 404).
- US10785068 (Inphi, relative intensity noise cancellation, grant 2020-09-22): dependent claims
  recite a filter comprising a leaky integrator; data-communication noise cancellation, not noise
  generation. Not opened.
- US3746791A (speech synthesizer from white noise), US4191175A, US4673871, US4050343A, US2773185,
  US7684528B2, CN201956973U, US10404211B1: surfaced in searches and are
  either past term by number (1950s to 1980s) or off the topic (RF, speech, communications); none
  opened except where listed above.

## Claim analysis

Stage 2, 2026-11-26, for the rows a human marked **In force** (1 to 5). Claim text comes from the
Google Patents HTML full text, not OCR. The fetch tool returns text through a summarising model and
limits each quote to 125 characters, so long elements were obtained as consecutive verbatim
fragments and are joined here with no gaps. US9789024B2's claims 13 and 19, US8885844B2's claims 1
and 9 and US7460675B2's claims 1 and 7 were each fetched twice and the two fetches agreed word for
word; US9789024B2's claim 1 and the independent claims of rows 4 and 5 were fetched once. The
granted PDFs were not read: US7460675's PDF is scanned images with no text layer, and no OCR was
available (see "Refused or failed").

Design facts used (from "Planned design"): software in a library crate of an audio editor; one
white stream (xorshift64\*, uniform) through one first-order IIR section `y[n] = x[n] + a·y[n-1]`
with a fixed 5 Hz pole and a fixed gain; seeded; its output feeds the synthesizer's noise
oscillator. The design has no cane or other physical body, no transducer or loudspeaker of its
own, no power switch, no room measurement, no ambient-noise measurement, no masking, no detection
of silence, no measurement of electrical noise, no combining with another audio signal, and no
desk, worktop, motor, sensor or wireless link.

### Row 1: US9789024B2 (Alexander), priority 2014-11-03

Independent claims: 1, 13, 19 (20 claims; 2 to 12 depend on 1, directly or through 3 and 5; 14 to
18 on 13; 20 on 19, per the Google Patents claim list).

**Claim 1** (white cane).

| Element | Planned design |
|---|---|
| "A white cane comprising:" | Not met: the design is software in an audio editor, not a cane. |
| "an elongate body having a proximal end adapted to be held by or secured to a human user, and a distal tip;" | Not met: no physical body. |
| "a signal generator configured to generate an audio signal adapted for use in acoustic wayfinding; and" | Partly: the design generates an audio signal; it is not adapted for acoustic wayfinding. Not met as recited. |
| "a noise generator comprising a transducer operatively connected to the signal generator and located proximate the distal tip, the noise generator being configured to generate sound within a range of frequencies audible to humans." | Not met: the design has no transducer and no distal tip; it outputs samples. |

**Claim 13** (white cane).

| Element | Planned design |
|---|---|
| "A white cane comprising:" | Not met, as for claim 1. |
| "an elongate body having a proximal end adapted to be held by or secured to a human user, and a distal tip; and" | Not met. |
| "a noise generator located proximate the distal tip, the noise generator being configured to emit noise within a range of frequencies audible to humans and selected from the group consisting of white noise, pink noise, red or Brownian noise, and grey noise." | Partly: the design generates red or Brownian noise in the audible range (a group member). It is not located proximate a distal tip and does not itself emit sound. Not met as recited. |

**Claim 19** (white cane).

| Element | Planned design |
|---|---|
| "A white cane comprising:" | Not met. |
| "an elongate body having a proximal end adapted to be held by or secured to a human user, and a distal tip;" | Not met. |
| "a noise generator located proximate the distal tip, the noise generator being configured to emit noise within a range of frequencies audible to humans and selected from the group consisting of white noise, pink noise, red or Brownian noise, and grey noise;" | Partly, as for claim 13. Not met as recited. |
| "a power source; and" | Not met: the design has no power source of its own (it runs on the host computer). |
| "a power switch;" | Not met. |
| "wherein the power source, power switch, and noise generator are operatively connected so as to allow a user of the white cane to operate the power switch to turn the noise generator on and off." | Not met. |

**Verdict: Does not read.** Every independent claim requires "A white cane" with "an elongate body
having a proximal end ... and a distal tip" and a noise generator "located proximate the distal
tip"; the design is software with no body. The only shared feature is generating noise of a
listed colour; the claims recite no generation method, and the description says only that a
transducer "transduces the electrical signal into noise". Prior art in this note that predates its
2014-11-03 priority, for the design's leaky integrator: Gilman et al. 1963 and Bourke 1998 (both
unverified), Nord Modular book 2003, Haskell `dsp` 2003, Plaszczynski 2005, Smith 2011, Audacity
forum 2013, US4327419A (1980).

### Row 2: US7460675B2 (Soft DB), priority 2004-06-21

Independent claims: 1, 7 (15 claims; 2 to 6 depend on 1, directly or through 5; 8 to 15 on 7,
directly or through 8 and 10, per the Google Patents claim list; claim 9 is worded as a system
claim but refers to method claim 8).

**Claim 1** (sound masking system).

| Element | Planned design |
|---|---|
| "A sound masking system for emitting a target masking noise in a room, comprising a mask filter" | Not met: no masking, no room; the design has a filter, but not a mask filter as defined by the rest of the claim. |
| "that is automatically determined by measuring an acoustical response of the room and an ambient noise in the room," | Not met: the filter coefficient is fixed by the sample rate; nothing is measured. |
| "the acoustical response of the room being obtained during emission of an unfiltered white noise in the room and the ambient noise in the room being obtained in the absence of the emission of the unfiltered white noise in the room," | Not met. |
| "and subtracting the ambient noise from an ideal masking noise and correcting by the acoustical response of the room," | Not met. |
| "said mask filter, once determined, filtrating a white noise to yield the target masking noise to be emitted in the room." | Partly: the design filters white noise; the filter is not a mask filter so determined, and nothing is emitted in a room. Not met as recited. |

**Claim 7** (method). The Google Patents text ends this claim with a semicolon after the third step
and claim 8 follows at once, so a final step may be missing from the HTML text (see Open
uncertainties). The verdict does not depend on it: a missing step could only add elements.

| Element | Planned design |
|---|---|
| "A method for generating a target masking noise in a room, comprising:" | Not met: no masking noise, no room. |
| "obtaining an acoustical response of the room during emission of an unfiltered white noise in the room;" | Not met: nothing is measured. |
| "obtaining an ambient noise in the room in the absence of the emission of the unfiltered white noise in the room;" | Not met. |
| "determining a noise correction filter by subtracting the ambient noise from an ideal masking noise and correcting the result by the acoustical response of the room;" | Not met: the filter is fixed analytically from the 5 Hz corner and the sample rate. |

**Verdict: Does not read.** Both independent claims require a filter determined by measuring "an
acoustical response of the room" during emission of unfiltered white noise and "an ambient noise
in the room", then subtracting the ambient noise from an ideal masking noise; the design's filter
is fixed and nothing is measured. Prior art in this note that predates its 2004-06-21 priority, for
the design's leaky integrator: Gilman et al. 1963 and Bourke 1998 (both unverified), Nord Modular
book 2003, Haskell `dsp` 2003 (copyright year only), US4327419A (1980), US4270430A (1979).

### Row 3: US8885844B2 (Continental Automotive Systems), priority 2008-10-23

Independent claims: 1, 9 (11 claims; 2 to 8 depend on 1, directly or through 5 and 7; 10 and 11 on
9, per the Google Patents claim list). Claims 7 ("shaping a frequency spectrum of the masking
noise") and 8 (shaping it "in response to an interference noise frequency spectrum") are
dependent on claim 1 and add to it; they cannot read where claim 1 does not.

**Claim 1** (method).

| Element | Planned design |
|---|---|
| "A method for masking audio noise, comprising:" | Not met: no masking. |
| "detecting a period of substantial silence in an audio signal output from an audio system;" | Not met: the design does not monitor any audio signal. |
| "measuring amplitude of electrical noise that is capable of generating audible noise in the audio signal;" | Not met: nothing is measured. |
| "generating a masking noise signal to be combined with the audio signal in response to detecting the period of substantial silence and in response to the amplitude of the electrical noise; and" | Partly: the design generates a noise signal; not in response to silence or to measured electrical noise, and not for combining with that audio signal. Not met as recited. |
| "combining the masking noise signal with the audio signal during the period of substantial silence." | Not met: the oscillator's output is a synthesizer voice; it is not added to an audio system's output during silence. |

**Claim 9** (machine-readable medium).

| Element | Planned design |
|---|---|
| "A non-transitory, machine-readable medium having stored thereon instructions, which when executed by a machine, cause said machine to perform operations comprising:" | Met in form: the design is software. |
| The four operations, word for word as in claim 1 from "detecting a period of substantial silence" to "during the period of substantial silence." | Not met, as for claim 1. |

**Verdict: Does not read.** Both independent claims require "detecting a period of substantial
silence in an audio signal output from an audio system", "measuring amplitude of electrical noise",
and combining the masking noise with that audio signal during the silence; the design does none of
these. Prior art in this note that predates its 2008-10-23 priority, for the design's leaky
integrator: Gilman et al. 1963 and Bourke 1998 (both unverified), Nord Modular book 2003, Haskell
`dsp` 2003, Plaszczynski 2005, US4327419A (1980).

### Row 4: US10827829B1 (Steelcase), priority 2012-10-10

Independent claims: 1, 21, 23 (23 claims; 2, 10, 11, 14, 15, 17 depend on 1, the rest on those, and
22 on 21, per the Google Patents claim list). Per the fetch, no claim, dependent or independent,
mentions noise, sound or a sound generator.

**Claim 1** (system).

| Element | Planned design |
|---|---|
| "A system for managing the elevation of a workstation, the system comprising:" | Not met. |
| "a frame;" | Not met. |
| "a worktop having an upper worksurface;" | Not met. |
| "at least a first powered vertical-displacement drive mechanism connected between the frame and the worktop and arranged to selectively vertically displace the worktop with respect to the frame;" | Not met. |
| "a presence sensor detecting presence of a user's body within a sensing zone associated with the workstation and generating a signal indicating user presence at the workstation whenever any user's body is present within the sensing zone; and" | Not met. |
| "a memory device storing at least a first height adjustment time specifying a specific time during a day at which the worktop is to be automatically moved to a first scheduled height when at a height that is different than the first scheduled height, the first height adjustment time being prescheduled prior to any use of the workstation during the day and that occurs independent of any prior workstation use during the day;" | Not met. |
| "a processor linked to the drive mechanism and the sensor, the processor programmed to track a current time and identify occurrence of at least the first height adjustment time, the processor using presence sensor signals to track user presence at the workstation and, upon determining in information from the presence sensor that no user is present at the workstation at the at least a first height adjustment time and that the worktop is at a height that is different than the first scheduled height, automatically controlling the drive mechanism to move the worktop from a current height to the first scheduled height." | Not met. |

**Claim 21** (method).

| Element | Planned design |
|---|---|
| "A method for managing the elevation of a workstation, the method comprising the steps of:" | Not met. |
| "detecting presence of a user's body within a sensing zone associated with the workstation and generating a signal indicating user presence at the workstation whenever any user is present within the sensing zone;" | Not met. |
| "storing at least a first height adjustment time specifying a first specific time during a day at which the worktop is to be automatically moved to a first scheduled height when the worktop is at a height different than the first scheduled height, the first height adjustment time being prescheduled prior to any use of the workstation during the day and that occurs independent of any workstation use during the day; and" | Not met. |
| "using presence sensor signals to track user presence at the workstation and, upon determining that no user is present at the workstation at the first height adjustment time and that the worktop is at a height other than the first scheduled height, automatically moving the worktop to the first scheduled height." | Not met. |

**Claim 23** (system).

| Element | Planned design |
|---|---|
| "A system for managing the elevation of a workstation, the system comprising:" | Not met. |
| "a frame;" | Not met. |
| "a worktop having an upper worksurface;" | Not met. |
| "at least a first powered vertical-displacement drive mechanism connected between the frame and the worktop and arranged to selectively vertically displace the worktop with respect to the frame;" | Not met. |
| "a memory device storing a plurality of height adjustment times specifying specific times during a day at which the worktop is to be automatically moved to a scheduled height, each of the plurality of height adjustment times being prescheduled prior to any use of the workstation during the day and that occurs independent of any prior workstation use during the day; and" | Not met. |
| "a presence sensor detecting presence of a user's body within a sensing zone associated with the workstation and generating a signal indicating user presence at the workstation whenever any user's body is present within the sensing zone; and" | Not met. |
| "a processor linked to the drive mechanism and the sensor, the processor tracking current time and identifying occurrence of each of the plurality of height adjustment times, the processor further using presence sensor signals to track user presence at the workstation and automatically adjusting the worktop elevation to the scheduled height each time any of the plurality of prescheduled height adjustment times occurs while the worktop is at a height that is different than the scheduled height while no user is present at the workstation." | Not met. |

**Verdict: Does not read.** Every independent claim requires a workstation with a worktop whose
height is moved at prescheduled times when a presence sensor shows no user; the design has no
workstation, worktop, drive or sensor, and no claim recites sound. Brown noise appears only in the
description (for example "“Brown” noise (also called “red” noise) is a random signal that has been
filtered", and "a basic electronic noise-generating circuit is understood by those of ordinary
skill in the art of electronics"). Prior art in this note that predates its 2012-10-10 priority,
for the design's leaky integrator: Gilman et al. 1963 and Bourke 1998 (both unverified), Nord
Modular book 2003, Haskell `dsp` 2003, Plaszczynski 2005, Smith 2011 (book date; see Prior art),
US4327419A (1980).

### Row 5: US11918116B1 (Steelcase), priority 2012-10-10

Independent claims: 1, 20 (20 claims; 2, 4, 5, 7, 9, 10, 12 to 19 depend on 1; 3 and 6 on 2; 8 on
7; 11 on 10, per the Google Patents claim list). Per the fetch, no claim mentions noise, sound or a
sound generator.

**Claim 1** (desk).

| Element | Planned design |
|---|---|
| "A desk comprising:" | Not met. |
| "a work surface;" | Not met. |
| "a support frame for supporting the worksurface;" | Not met. |
| "a motor coupled to the support frame, the motor operable to move the support frame to change the height of the support frame;" | Not met. |
| "a wireless communication circuit coupled to the work surface, the wireless communication circuit operable to receive messages from a user support device within a communication range of the wireless communication circuit, the messages including indications of sensed user interactions with the support device, the user support device supporting at least a portion of the user's weight;" | Not met. |
| "a controller coupled to the motor, the controller including a processor programmed to identify any one of a plurality of different user positions at the workstation based on messages received from the support device, the processor further programmed to perform the steps of:" | Not met. |
| "receiving a message from the support device indicative of sensed user interaction;" | Not met. |
| "determining a position of the user based on the received message wherein the determined position is one of the plurality of different user positions at the workstation; and" | Not met. |
| "generating a control signal for the motor based on the determined position of the user." | Not met. |

**Claim 20** (desk).

| Element | Planned design |
|---|---|
| "A desk comprising:" | Not met. |
| "a work surface;" | Not met. |
| "a support frame for supporting the worksurface;" | Not met. |
| "a motor coupled to the support frame, the motor operable to move the support frame to change the height of the support frame;" | Not met. |
| "a wireless communication circuit coupled to the work surface, the wireless communication circuit operable to receive messages from a user support device within a communication range of the wireless communication circuit, the messages including indications of sensed user interaction with the support device, the user support device supporting at least a portion of the user's weight at the desk;" | Not met. |
| "a controller coupled to the worksurface and electrically coupled to the motor, the controller operable to:" | Not met. |
| "(i) receive a first message from the support device indicative of a sensed first user interaction;" | Not met. |
| "(ii) determine a first position of the user proximate the desk based on the first message;" | Not met. |
| "(iii) receive a second message from the support device indicative of a sensed second user interaction;" | Not met. |
| "(iv) determine a change in position of the user from the first position to a second position proximate the desk based on the received second message; and" | Not met. |
| "(v) generate a control signal for the motor based on the determined change in position." | Not met. |

**Verdict: Does not read.** Both independent claims require "A desk" with a motor that changes the
support frame's height and a controller acting on messages from a user support device; the design
has none of these, and no claim recites sound. Brown noise appears only in the description (for
example "Brown noise produces a much warmer tone than white noise (0 dB/oct) or pink noise (−3
dB/oct)."). Prior art: as for row 4 (same priority date).

## Search log

All searches 2026-11-26. The web search tool returns a prose summary, not raw result lists, so hit
counts are approximate (about 10 per query).

| Database | Query | Date | Hits | Result |
|---|---|---|---|---|
| Web (general, extended) | 1. patent brown noise generator leaky integrator one-pole low-pass filter white noise red noise | 2026-11-26 | 9 | US4327419A, US7251301, US5797120 and US7570937 (not opened); GitHub noise generators; issue #207; no patent for the design |
| Web (general, extended) | 2. patent Brownian noise generator integrating white noise audio synthesizer noise oscillator colored noise | 2026-11-26 | 10 | US4270430A, US4327419A, US3746791A, US20140249359A1 (sound machine, white noise; not opened); no brown-noise patent |
| Web (general) | 3. brown noise one-pole lowpass filter white noise music-dsp musicdsp.org | 2026-11-26 | 10 | Audacity thread, dsp-kit, Haskell dsp, Nord Modular book, SuperCollider docs |
| Web (general, extended) | 4. patent colored noise generator normalized gain white noise filter -6 dB/octave 1/f^2 sound masking digital | 2026-11-26 | 10 | US20040203454A1, WO2015187345A1, US20030103632A1, US7460675, US7684528B2 (communications, not opened) |
| Web (general, extended) | 5. "brown noise" patent claim "leaky integrator" OR "low pass filter" "white noise" sound generator sleep OR masking OR synthesizer US patent granted | 2026-11-26 | 9 | US10785068 (Inphi), US10827829, US11918116; no brown-noise generation patent |
| Web (general) | 6. Julius O. Smith one-pole lowpass filter white noise "brown noise" OR "red noise" OR "random walk" leaky integrator DSP book | 2026-11-26 | 10 | Smith SASP "Filtered White Noise", DigitizingNewton leaky-integrator page, Faust filter.lib |
| Web (general, extended) | 7. patent "brown noise" generator claim "integrating" white noise digital audio device "brown noise" sound masking tinnitus apparatus granted | 2026-11-26 | 10 | WO2015187345A1, US20030103632A1, US20170025113A1, US8885844B2, US7460675 |
| Web (general, extended) | 8. patent "red noise" OR "Brownian noise" generating first-order recursive filter white noise sequence 1/f^2 "random walk" signal generator US patent | 2026-11-26 | 10 | US9789024B2 (white cane); Nord Modular book; Plaszczynski arXiv; Wikipedia; source-code.biz BrownNoiseGenerator; MathWorks Colored Noise |
| Web (general, extended) | 9. Espacenet OR "Google Patents" noise generator "brown noise" "pink noise" "white noise" selectable spectrum audio generator filter coefficients patent US 2015..2024 | 2026-11-26 | 10 | US7251301, US4191175A; open-source generators; no patent in range |
| Web (general, extended) | 10. sound machine patent digital noise generation "brown noise" "pink noise" filter white noise generator claims sleep device Adaptive Sound Technology OR Hatch OR Marpac | 2026-11-26 | 10 | No sound-machine patent surfaced; US10827829, US11918116 again |
| Web (general, extended) | 11. patent synthesizer noise generator "noise color" low-pass filter white noise cutoff tracking noise oscillator electronic musical instrument digital "red noise" OR "brown noise" OR "Brownian" | 2026-11-26 | 10 | US5728963A, US4327419A, US4270430, US4050343A, US5566678, US10404211B1 (spot noise, not opened) |
| Web (general) | 12. Plantronics "Noise masking in headsets" US patent brown noise pink noise masking signal granted | 2026-11-26 | 10 | Only the application US20150348530A1; no grant found |
| Web (general) | 13. Gilman Fuglister Mitchell 1963 "On the power spectrum of red noise" Journal of the Atmospheric Sciences first-order autoregressive | 2026-11-26 | 10 | Citation and DOI only |
| Google Patents (page fetch by number) | WO2015187345A1, US20030103632A1, US4270430A, US9789024B2 (twice), US8885844B2, US7460675, US20170025113A1, US20150348530A1, US5566678A, US5728963A | 2026-11-26 | 10 | Dates, status and expiry read; claims read only for US9789024B2 (claim 13 fragment) and, as summarised, US7460675 claim 1; others abstract and description only |
| Google Patents (stage 2, claims) | [US9789024B2](https://patents.google.com/patent/US9789024B2/en): claims 1, 13, 19, the dependency list, the description's sentence on the transducer | 2026-11-26 | 1 | Full text of the three independent claims read, in verbatim fragments (13 and 19 fetched twice and agreed); HTML full text, not OCR |
| Google Patents (stage 2, claims) | [US7460675](https://patents.google.com/patent/US7460675) and [US7460675B2](https://patents.google.com/patent/US7460675B2/en): claims 1, 7, 8, the dependency list, dates | 2026-11-26 | 1 | Claims 1 and 7 read in verbatim fragments, three fetches agreed; claim 7 ends in a semicolon in the HTML text; HTML full text, not OCR |
| Google Patents (stage 2, claims) | [US8885844B2](https://patents.google.com/patent/US8885844B2/en): claims 1, 7, 8, 9, the dependency list, dates | 2026-11-26 | 1 | Full text of the two independent claims read, in verbatim fragments, fetched twice and agreed; HTML full text, not OCR |
| Google Patents (stage 2, claims) | [US10827829B1](https://patents.google.com/patent/US10827829B1/en): bibliographic data, claims 1, 21, 23, the dependency list, description passages on noise (page read in five parts, offsets 0 to 400000) | 2026-11-26 | 1 | Assignee, dates, expiry filled in; independent claims read once in verbatim fragments; HTML full text, not OCR |
| Google Patents (stage 2, claims) | [US11918116B1](https://patents.google.com/patent/US11918116B1/en): bibliographic data, claims 1, 20, the dependency list, cross-reference, description passages on noise (page read in five parts, offsets 0 to 400000) | 2026-11-26 | 1 | Assignee, dates, expiry filled in; independent claims read once in verbatim fragments; HTML full text, not OCR |
| Web (general) | 14. "US11918116" OR "11,918,116" Steelcase height adjustable claims | 2026-11-26 | 10 | Nothing on the patent; Steelcase product pages and assignee lists |
| Other pages fetched | GitHub issue #207; Audacity forum; Smith SASP and DigitizingNewton pages; Nord Modular book ch. 4; arXiv astro-ph/0510081; Hackage dsp-0.2.5; Wikipedia Brownian noise; GitHub archibate/noise-gen | 2026-11-26 | 8 | Read (noise-gen: describes brown as "a leaky integrator", MIT, measured -5.72 dB/octave; repository date not shown, so not used as prior art) |

Refused or failed: a Google Patents search-results URL (`patents.google.com/?q=...`) returned only
the page heading (it needs scripts), so no Google Patents query was run. `doi.org` for the Gilman
paper redirected to the AMS site; the redirect was not followed. `patents.google.com/patent/US10827829B2/en`
and `.../US11918116B2/en` returned HTTP 404. The Google Patents pages for WO2015187345A1,
US20150348530A1 and US5566678A were truncated at 100000 characters (the claims of the first two were
not read). In stage 2: `freepatentsonline.com/7460675.html` returned HTTP 403; US7460675's granted
PDF (patentimages.storage.googleapis.com) is scanned fax-encoded images with no text layer, and no
OCR or page renderer was available, so the PDF text was not read. The Google Patents pages of
US10827829B1 and US11918116B1 (about 425000 characters each) were read in five 100000-character
parts; the claims were in the fourth, and the fifth held citation tables only (no legal events). Not tried: Espacenet, USPTO Patent Public Search and Patent Center, FreePatentsOnline,
any search by CPC class (for example G10H 2250/481 noise generation, H03B 29/00 noise generators,
H03H 17/04 recursive digital filters, G10K 15/02 sound masking) or by assignee (Roland, Yamaha,
Korg, Native Instruments, Moog, Dolby), and the citation lists of the candidates beyond
US9789024B2's cited-by list.

## Open uncertainties

- No patent-database search was run (see "Refused or failed"). "No candidate found" is a statement
  about web-search coverage. CPC-class and assignee searches (noise generators, recursive digital
  filters, sound masking, synthesizer noise sources) are the main gap, as in the pink-noise note.
- The earliest dated primary source for the leaky-integrator construction was not read: Gilman et
  al. 1963 and Bourke 1998 are by index or citation only, the Nord Modular book is dated 2003 (the
  module itself is older but undated here), and the Smith 2011 passage is shown on a page with a
  2026 copyright. A human may fetch the Gilman paper or the Nord Modular manual to fix an earlier
  date.
- Rows 1 to 5 (stage 2): the claim text came through the fetch tool's summarising model in
  fragments of at most 125 characters, joined here; it was not compared with the granted PDFs. A
  human should check the quoted claims against the PDFs or USPTO Patent Center before relying on
  the verdicts. US9789024B2's claim 1 and the independent claims of rows 4 and 5 were fetched only
  once, and that no claim of rows 4 and 5 mentions sound rests on the fetch's statement. Not
  checked: certificates of correction, reexamination or other post-grant changes, and family
  members (continuations) with different claims, for all five rows. Rows 4 and 5 belong to a
  Steelcase family (US9486070B2, US10038952B2, US10085562B1 and application 16/810,353 are named
  as related); other members were not opened, and one could claim the description's sound
  generator.
- Row 1: the verdict rests on "A white cane", its "elongate body" and a noise generator "located
  proximate the distal tip". A future feature that emits brown noise from a device attached to a
  cane or mobility aid would need its own check against this patent.
- Row 2: the Google Patents text of claim 7 ends with a semicolon after "correcting the result by
  the acoustical response of the room;", which suggests a final step (presumably filtering white
  noise with the correction filter) is missing from the HTML. The granted PDF is images only and
  was not read. The verdict holds either way (the steps quoted are already missing from the
  design), but a human should read claim 7 in the PDF. Its term end (2027-01-09) is about six
  weeks away; a future room-masking feature that measures the room would need its own check until
  then.
- Row 3: the verdict rests on detecting silence in an audio system's output, measuring electrical
  noise, and combining masking noise with that output. A future feature that adds noise to playback
  during silence (comfort noise, dither keyed to silence) would need its own check against this
  patent.
- Plantronics: the US application is shown abandoned, but continuations or the granted EP/other
  national phases were not searched; its dependent claims name brown noise as a masking signal.
- Elements 3 (normalisation to unity at 1 kHz) and 4 (seeding, white fallback): no prior art
  sought; both are trivial steps. Element 1 relies on prng-patents.md.
- Term end dates are as Google prints them; terminal disclaimers and adjustments not otherwise
  seen. Row 2's date (2027-01-09) is about six weeks away, so a human may confirm whether it
  expires before the feature ships. Row 4's 2033-09-25 is 20 years from the 2013-09-25 application
  it claims, and row 5's 2033-12-07 is printed as adjusted with no figure shown; neither shows a
  terminal disclaimer, and no legal events were seen in the parts of their pages read.
