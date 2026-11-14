# Patent status of the in-house Chorus and Flanger effects (LFO phase accumulator and shapes, stereo spread, tempo-synced LFO, modulated delay with 4-point Hermite read)

Research note, 2026-11-13. This is not legal advice. It records what primary sources show, so a
reviewer can check it. Stage 1 listed the candidates; a human marked **Status** (all three in
force); stage 2 (2026-11-13) analysed the independent claims of all three (see "Claim analysis").
Dates in the Candidates table are as Google Patents prints them (Google states that its status and
expiry fields are assumptions, not legal conclusions). For issue #62.

## Planned design

Software, portable scalar Rust, "Chorus" and "Flanger" effect nodes in an audio editor's effect
graph; per-sample processing on the real-time audio thread; two channels (L/R).

1. **LFO phase accumulator.** f64 phase in 0..1, incremented by frequency / sample_rate each
   sample, wrapped. Frequency 0.05 to 5 Hz (free) or tempo-synced. Not band-limited (LFO rates
   only).
2. **LFO shapes.** Each a pure function of phase giving -1..1: sine, triangle, saw up, saw down,
   square, sample-and-hold random (a new random value from a PRNG each time the phase wraps),
   smooth random (random points chosen at each wrap, joined by cosine or cubic Hermite
   interpolation across the cycle). The PRNG is the in-house `dsp::Noise`, already cleared in
   [prng-patents.md](prng-patents.md).
3. **Stereo spread.** The right channel reads the same LFO clock at a phase offset (0 to 180
   degrees); the random shapes use a separate decorrelated random stream for the offset read.
4. **Tempo-synced LFO period.** Period = note value x feel (straight, dotted x1.5, triplet x2/3)
   from frames per whole note; note list 1/16 up to 4 bars.
5. **Modulated delay with 4-point cubic Hermite fractional read.** Delay time = base + depth x
   LFO, smoothed by a linear parameter ramp; read from a circular buffer at a fractional position
   with a 4-point, 3rd-order Hermite (Catmull-Rom) interpolator across taps floor(f)-1 to
   floor(f)+2.
6. **Chorus.** 1 to 4 voices, each a modulated delay tap with LFO phases evenly spaced; base delay
   5 to 30 ms, depth 0 to 10 ms; voices summed; dry/wet mix; no feedback.
7. **Flanger.** One modulated tap per channel, base delay 0.1 to 10 ms, depth as a percentage of
   base, feedback -95 % to +95 % (the wet read times feedback added to the input before writing
   into the line), dry/wet mix.

Out of scope: through-zero flanging (dual delay lines crossing zero), bucket-brigade or tape circuit
models, band-limited audio-rate oscillators. Candidates touching only those are not pursued here.

## Summary

Three candidates have a printed term end in the future (rows 1 to 3). Rows 1 and 2 do not read on
the planned design; row 3 is **Unclear** on whether the smooth random LFO, a sub-audio control
waveform, is the "digital audio signal" its claim generates (see "Claim analysis"). Row 3's US
family member US9741329B2, not a candidate row, has a broader claim 1 and needs a human decision
(see "Open uncertainties"). The basic art for an
LFO-modulated delay chorus (1975 to 1980), the multi-voice phase-offset chorus (1975 to 1980) and
the delay-time modulation device (1981, 1993) is expired. This rests on web searches and Google
Patents page fetches only, because no patent database query interface could be used (see "Refused
or failed"). "No candidate found" below means the searches in the Search log found none, not that
none exists.

| Technique | Verdict | Confidence |
|---|---|---|
| 1. LFO phase accumulator, frequency / sample rate, wrapped | No candidate found (searches 1 to 4, 9). Row 1 (US10109265B2) recites an LFO phase only in dependent claim 5, under a claim 1 that does not read: **Does not read**. | Low |
| 2. LFO shapes (sine, triangle, saws, square, S&H random, smooth random) | Row 3 (EP3105753B1): **Unclear** for smooth random only. Its claim 1 is a waveform interpolated between control points (amplitude, period phase) changed by control signals; the smooth random shape fits that shape of words, but whether a sub-audio LFO is a "digital audio signal" and a PRNG draw a "control signal" decides it. Fixed shapes (sine, triangle, saws, square, S&H) have no changeable interpolated control points: does not read. US family member US9741329B2 not analysed as a row; see "Open uncertainties". Sine, sawtooth and triangle LFOs for delay modulation are in expired 1975 to 1980 art. | Low |
| 3. Stereo spread by right-channel phase offset, decorrelated random stream | No patent found (searches 7, 8). Phase-offset channels appear only in expired art (US4038898A, US4144790A, US4384505A). | Low |
| 4. Tempo-synced LFO period (note value x feel) | Row 1 (US10109265B2): **Does not read**. Independent claims 1, 9, 11 need a second effect provided in place of the first on request, the first effect's parameter kept varying meanwhile, and the first effect resumed from it; the design has no such mechanism. Tempo appears only in dependent claims 2, 8, 10, 12. Not otherwise found (searches 10, 11). | Medium |
| 5. Modulated delay, linear ramp on delay time, 4-point Hermite fractional read | Row 2 (US8060226B2): **Does not read**. Independent claims 1, 11, 29, 30 need interpolated *input* samples written at a fractional write address; claim 21 needs a "fractionally modulated" write pointer. The design writes whole input samples at a write pointer that advances one frame per sample and interpolates on the read. No patent found claiming Hermite or Catmull-Rom for a delay read (searches 13, 14). Fractional delay with allpass and crossfader is expired (US5742532A, see [delay-patents.md](delay-patents.md)). | Low |
| 6. Chorus: 1 to 4 voices, evenly spaced LFO phases, dry/wet, no feedback | No in-term candidate found (searches 1, 2, 15). Earlier art, expired: US4038898A, US4144790A, US4205579A, US4384505A, US4569268A. | Medium |
| 7. Flanger: one tap per channel, feedback -95 to +95 %, dry/wet | No candidate found (searches 3, 16, 17). Row 1 (US10109265B2) has a flanger in its embodiment but claims an effect-switching scheme: **Does not read**. | Low |

## Prior art

Items marked (unverified) come from a search summary or an index, not from the publication itself.

- **Element 5 (fractional read, Hermite interpolation).**
  - E. Catmull and R. Rom, "A class of local interpolating splines", in *Computer Aided Geometric
    Design*, ed. R. E. Barnhill and R. F. Riesenfeld, Academic Press, New York, 1974, pp. 317 to 326
    (conference at the University of Utah, 1974-03-18 to 21). Bibliographic details agree across
    several indexes, for example [MathWorld](https://mathworld.wolfram.com/Catmull-RomSpline.html);
    the paper's own text was not opened (unverified).
  - O. Niemitalo, "Polynomial Interpolators for High-Quality Resampling of Oversampled Audio",
    2001. The publication's PDF ([yehar.com](https://yehar.com/blog/wp-content/uploads/2009/08/deip.pdf))
    returned binary the fetch tool could not read, so its content and exact date are not
    established here. The 2001 date and title come from a source comment in mpg123
    ([source](https://grok.dragonflybsd.org/xref/dports/audio/mpg123/mpg123-1.29.3/src/libsyn123/resample.c?a=true)),
    per a search summary (unverified).
  - T. I. Laakso, V. Valimaki, M. Karjalainen and U. K. Laine, "Splitting the Unit Delay", *IEEE
    Signal Processing Magazine* 13(1), 30 to 60, 1996, DOI 10.1109/79.482137
    ([dblp](https://dblp.dagstuhl.de/rec/journals/spm/LaaksoVKL96.html)); content not read.
  - J. Dattorro, "Effect Design, Part 2: Delay-Line Modulation and Chorus", *J. Audio Eng. Soc.*
    45(10), 764 to 788, 1997 (October per a search summary; Wikipedia's reference gives the year
    only) ([Wikipedia, Dattorro industry scheme](https://en.wikipedia.org/wiki/Dattorro_industry_scheme);
    PDF at [CCRMA](https://ccrma.stanford.edu/~dattorro/EffectDesignPart2.pdf) is a scanned image the
    fetch tool could not read, so what it says about Hermite interpolation is not established).
    Wikipedia says fractional delays need interpolation "such as Lagrange or all-pass".
  - Fractional delay by two-reader allpass and crossfader (1996): US5742532A, expired, see
    [delay-patents.md](delay-patents.md) "Prior art".
  - EP1234302A1 (STMicroelectronics Asia Pacific), priority and filing 1999-11-24: aliasing
    cancellation in delay-modulated audio effects by band-limiting the input to half the
    instantaneous sampling frequency before interpolation; Google prints "Expired - Lifetime",
    expiry 2019-11-24 ([Google Patents](https://patents.google.com/patent/EP1234302A1/en)). Already
    recorded in [amplifier-patents.md](amplifier-patents.md); it touches element 5 only for
    aliasing, and is expired.
  - US4569268A (Nippon Gakki / Yamaha), priority 1981-12-23, granted 1986-02-11: digital memory
    with a modulated read address as the modulation effect, replacing BBD or CCD delays
    ([Google Patents](https://patents.google.com/patent/US4569268A/en)). Interpolation not stated
    in the abstract.
  - US5559300A (Yamaha), priority 1993-09-03, granted 1996-09-24: delay-time modulation from the
    write to read memory lag, modulation data from an LFO, an address limit keeping the read behind
    the write ([Google Patents](https://patents.google.com/patent/US5559300A/en)).
  - US5243658A (Casio), priority 1990-08-10, granted 1993-09-07: input written to memory and read
    with a delay varied by an LFO; the LFO waveform is low-pass filtered and offset so the read
    never passes the write ([Google Patents](https://patents.google.com/patent/US5243658A/en)).
  - Practitioner descriptions of a 4-point Hermite read for modulated taps, undated:
    [DSPRelated code listing](https://dsprelated.com/showcode/3.php), by a search summary only
    (unverified).
- **Element 1 (LFO phase accumulator).** No dated primary source for the accumulator form found.
  US4384505A (Baldwin), priority and filing 1980-06-24, granted 1983-05-24: a microprocessor lookup
  table generates the modulation signal, a 6.25 Hz sine superimposed on a 0.78 Hz sine
  ([Google Patents](https://patents.google.com/patent/US4384505)). Open uncertainty for a dated
  source for increment-and-wrap phase.
- **Element 2 (LFO shapes).**
  - Sine: US4384505A above (1980); US4144790A (Arp), priority and filing 1977-02-14, a modulated
    delay line per channel ([Google Patents](https://patents.google.com/patent/US4144790)).
  - Sawtooth and triangle: US4205579A (Roland), continuation filed 1978-06-12 of an application
    filed 1976-03-03, granted 1980-06-03: sawtooth delay modulation, with sinusoidal or triangular
    amplitude modulators ([Google Patents](https://patents.google.com/patent/US4205579)). Its
    summary says sinusoidal delay modulation leaves a vibrato effect.
  - Random: the knob table in Dattorro 1997 lists sinusoidal and "low-pass noise" modulation, per
    [Wikipedia](https://en.wikipedia.org/wiki/Dattorro_industry_scheme) (the paper itself not
    read). R. Belcham, "Smooth random LFO Generator", musicdsp.org, posted 2009-06-30: random
    levels held for random times, linearly interpolated, driving a sine oscillator, meant for
    modulating delay lines ([musicdsp](https://www.musicdsp.org/en/latest/Synthesis/269-smooth-random-lfo-generator.html)).
    The Max/MSP `rand~` object outputs linear interpolation between random values chosen at a given
    rate, described as suiting chorus, undated ([Cycling '74 tutorial](https://docs.cycling74.com/max5/tutorials/msp-tut/mspchapter30.html),
    per a search summary, unverified).
  - Square and the exact cosine or Hermite-joined random points: no dated source found. Product
    manuals (Korg step and random LFO in stereo random phasers) are undated.
- **Element 3 (stereo phase offset).** Phase-offset modulation across channels: US4038898A
  (Norlin), priority and filing 1975-03-03, granted 1977-08-02, bucket-brigade delay lines with
  clock rates modulated by phase-offset oscillators
  ([Google Patents](https://patents.google.com/patent/US4038898A/en)); US4384505A offsets three
  copies by 120 degrees (per a search summary of its text, unverified). Left/right LFO phase offset
  in plug-ins is documented in manuals, undated ([FL Studio Fruity Chorus](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/plugins/Fruity%20Chorus.htm)).
  No dated source for a separate decorrelated random stream for the offset read; open uncertainty.
- **Element 4 (tempo-synced LFO period).** Note value x feel arithmetic and tempo-synced delay
  times: see [delay-patents.md](delay-patents.md) "Prior art", element 2 (US5430243A, expired).
  Tempo-synced LFO: US10109265B2 (Casio), priority 2016-09-27, LFO angular velocity from tempo and
  beat, omega = TEMPO / (60 x BEAT) per a search summary of its description
  ([USPTO PDF](https://image-ppubs.uspto.gov/dirsearch-public/print/downloadPdf/10109265)), in term,
  see Candidates row 1. Manual references, undated: Korg Kronos "MIDI/Tempo Sync" for modulation
  effects ([manual](https://www.manualslib.com/manual/1264657/Korg-Kronos.html?page=230)); Yamaha
  A4000 tempo phaser with LFO rate in note lengths
  ([manual](https://manualslib.com/manual/374579/Yamaha-A4000.html?page=268)), both by search
  summary (unverified).
- **Element 6 (chorus, voices, phase spacing).**
  - US4038898A (above, 1975): several bucket-brigade delay lines with phase-offset modulators.
  - US4144790A (Arp), 1977-02-14, granted 1979-03-20: "choral generator" of several delay
    channels, outputs mixed with the undelayed audio.
  - US4164884A (Roland), priority 1975-06-24, granted 1979-08-21 ([Google Patents](https://patents.google.com/patent/US4164884)).
  - US4205579A and US4384505A above.
  - US4080861A (Thomas International), priority and filing 1976-07-09, granted 1978-03-28: three
    shift-register delays with out-of-phase vibrato-modulated clocks
    ([Google Patents](https://patents.google.com/patent/US4080861)).
  - Evenly spaced phases, digital: diffFx-pytorch implements N voices with offsets 2 pi i / N, undated
    ([source](https://difffx-pytorch.readthedocs.io/en/latest/_modules/diffFx_pytorch/processors/modulation/chorus.html),
    per a search summary, unverified).
- **Element 7 (flanger with feedback).**
  - J. O. Smith III, *Physical Audio Signal Processing*, W3K Publishing, 2010, "Flanging": basic
    flanger y(n) = x(n) + g x[n - M(n)], M(n) varied by a triangle or sine, an interpolated delay
    line for non-integer M; feedback is in a separate subsection not read
    ([CCRMA](https://ccrma.stanford.edu/~jos/pasp/Flanging.html)).
  - Dattorro 1997 (above): flanger delay 0 to 10 ms per Wikipedia's account.
  - Dry/wet mix: US4144790A sums channel outputs with the undelayed audio (1977).
  - A feedback loop in a flanger section: US5703312A (Yamaha), priority 1994-09-13, granted
    1997-12-30, per a search summary of its description (unverified; its abstract as fetched does
    not mention a flanger) ([Google Patents](https://patents.google.com/patent/US5703312A/en)).

## Candidates

Patents that might cover an element of the design and might still be in force. Status is filled
in by a human from Google Patents or USPTO Patent Center.

| # | Patent | Assignee | Priority / filing | Est. term end | Element it may touch | Link | Status |
|---|---|---|---|---|---|---|---|
| 1 | US10109265B2, Effect providing apparatus, effect providing method, storage medium and electronic musical instrument | Casio Computer Co., Ltd. | 2016-09-27 / 2017-09-26 | 2037-09-26 (Google; no adjustment printed; page prints "Active") | 4, 1, 7 (an LFO whose angular velocity is set from tempo and beat; flanger and filter effects; claim 1 gist: a first effect with a time-varying parameter, a second effect that replaces it on request, the parameter kept varying meanwhile and the first effect resumed from it) | [GP](https://patents.google.com/patent/US10109265B2/en) | **In force** |
| 2 | US8060226B2, Method and signal processing device to provide one or more fractional delay lines | Creative Technology Ltd | 2006-08-01 / 2007-07-31 | 2030-09-14 (Google adjusted; page prints "Active") | 5 (delay lines with one write pointer and several read pointers, fractional addresses of 25-bit integer and 7-bit fraction; claim recited as a fractionally modulated write pointer, per a search summary; claims not read; interpolation method not established) | [GP](https://patents.google.com/patent/US8060226B2/en) | **In force** |
| 3 | EP3105753B1 (A1: EP3105753A1), Method for the synthetic generation of a digital audio signal | Jobomusic AG | 2014-02-11 / 2015-02-09 | 2035-02-09 (Google; page prints Active / Granted; national validations not seen) | 2 (weak: a periodic waveform from control points interpolated or approximated into a continuous waveform, with a sample-and-hold noise component in the description; audio-rate synthesis; claims not read) | [GP](https://patents.google.com/patent/EP3105753A1/en) | **In force** |
| 4 | US9741329B2, US family member of row 3 | Jobomusic AG | 2014-02-11 / 2015-02-09 | 2035-02-09 (Google; page prints "Active", maintenance fees paid 2021 and 2025) | 2 (smooth random only; claim 1 ends "using the amplitude values for generating the digital audio signal"; found in stage 2) | [GP](https://patents.google.com/patent/US9741329B2/en) | Not checked by a human; moot after the decision below |

Checked by Chris Paro, 2026-11-13: all three in force.

Status values: **In force**, **Lapsed (fees)**, **Expired (term)**, **Abandoned**, **Invalidated**,
**Not found**. Record who checked and when, under the table.

Expired by term (filing date + 20 years, plus any printed term adjustment, is in the past), listed
for completeness and needing no check:

- [US4038898A](https://patents.google.com/patent/US4038898A/en), Norlin, filed 1975-03-03, expiry
  1994-08-02 per Google. Chorus by bucket-brigade delays with phase-offset modulation (elements 3, 6).
- [US4072079A](https://patents.google.com/patent/US4072079), CBS, filed 1976-08-09, expiry
  1995-02-07 per Google. Shift-register detuning for celeste and chorus (element 6).
- [US4080861A](https://patents.google.com/patent/US4080861), Thomas International, filed 1976-07-09,
  expiry 1995-03-28 per Google. Chorus with out-of-phase modulated delays (element 6).
- [US4144790A](https://patents.google.com/patent/US4144790), Arp, filed 1977-02-14, expiry
  1997-02-14 per Google. Multi-channel modulated-delay chorus (elements 3, 6, 7 loosely).
- [US4164884A](https://patents.google.com/patent/US4164884), Roland, filed 1978-02-01 (priority
  1975-06-24), expiry 1996-08-21 per Google. Chorus device (element 6).
- [US4205579A](https://patents.google.com/patent/US4205579), Roland, filed 1978-06-12 (continuation
  of 1976-03-03), expiry 1997-06-03 per Google. Sawtooth-modulated chorus (elements 2, 6).
- [US4384505A](https://patents.google.com/patent/US4384505), Baldwin, filed 1980-06-24, expiry
  2000-06-24 per Google. Three-channel chorus, lookup-table modulation (elements 1, 2, 6).
- [US4569268A](https://patents.google.com/patent/US4569268A/en), Nippon Gakki, filed 1984-12-14
  (continuation of 1982-12-13, priority 1981-12-23), expiry 2003-02-11 per Google. Digital memory
  with modulated read address (element 5).
- [US5243658A](https://patents.google.com/patent/US5243658A/en), Casio, filed 1991-07-19, expiry
  2011-07-19 per Google. LFO-varied memory delay with filtered LFO waveform (elements 2, 5).
- [US5559300A](https://patents.google.com/patent/US5559300A/en), Yamaha, filed 1994-09-02, expiry
  2014-09-02 per Google. Delay-time modulation with read/write address limit (element 5).
- [US5000074A](https://patents.google.com/patent/US5000074A/en), Yamaha, filed 1989-06-23, expiry
  2009-06-23 per Google. Click-free change of delay length by fade or crossfade, said to apply to
  vibrato and phase effects (element 5, delay-time change).
- [US5740716A](https://patents.google.com/patent/US5740716), Stanford (Stilson), filed 1997-05-01,
  expiry 2017-05-01 per Google (lapsed for fees 2010-04). Length-modulated digital delay line for
  synthesis (element 5, loosely).
- [US5742532A](https://patents.google.com/patent/US5742532), Stanford, expiry 2016-05-09 (see
  [delay-patents.md](delay-patents.md)). Fractional delay by allpass and crossfader (element 5).
- [EP1234302A1](https://patents.google.com/patent/EP1234302A1/en), STMicroelectronics Asia Pacific,
  filed 1999-11-24, expiry 2019-11-24 per Google. Aliasing cancellation in delay-modulated effects
  (element 5; see [amplifier-patents.md](amplifier-patents.md)).
- [US7444194B2](https://patents.google.com/patent/US7444194B2/en) (filed 2006-08-28, priority
  2001-03-05) and [US7865257B2](https://patents.google.com/patent/US7865257B2/en) (filed
  2008-10-24), Microsoft, "Audio buffers with audio effects": adjusted expiry 2022-10-24 and
  2022-11-11 per Google, printed "Expired - Fee Related". Chorus, flange and LFO delay modulation
  appear in the description; claims not read (elements 5, 6, 7).
- [US4329902A](https://patents.google.com/patent/US4329902A/en), Beehler Mockabee Arant and
  Jagger, filed 1980-01-24, expiry 2000-01-24 per Google. Band-split vibrato; its sample-and-hold is
  a circuit element, not an LFO shape (element 2, loosely).
- [US4924744A](https://patents.google.com/patent/US4924744), Hudson Soft, filed 1988-08-09, expiry
  2008-08-09 per Google. Noise channel and LFO-controlled FM depth in a sound generator (element 2,
  loosely).

Seen and not listed (the claims or abstract name no element of the design, or the record is not a
live patent):

- US10262646B2 (Media Overkill), printed "Active", expiry 2038-01-09: claim 1 gist is audio-rate
  oscillators switched in sequence; its description says it may serve as an LFO source, and claim 4
  limits frequency below 20 Hz, but no random, sample-and-hold or delay element
  ([Google Patents](https://patents.google.com/patent/US10262646B2/en)). A human may prefer to list it.
- US9479864B2 (Yamaha), expiry 2034-01-15, printed "Active": effector routing and level between two
  effectors; no modulation element ([Google Patents](https://patents.google.com/patent/US9479864B2/en)).
- US7875789B2 (Yamaha), expiry 2028-07-17, printed "Active": tone-generator selection between an
  internal and a PC tone module; chorus mentioned only as an example effect
  ([Google Patents](https://patents.google.com/patent/US7875789B2/en)).
- US8704068B2 (Apple), expired 2024-11-24; US20100011941A1 (individual), expired 2022-10-17: tempo
  matching of music, no LFO or delay modulation ([Google Patents](https://patents.google.com/patent/US8704068B2/en),
  [Google Patents](https://patents.google.com/patent/US20100011941A1/en)). Cited by US10109265B2.
- US5981860A (Yamaha), ceased, expiry 2017-08-29: software sound source; a search summary says its
  delay device weights two neighbouring memory values by the fraction (two-point, not four-point);
  not read.
- US5703312A (Yamaha), expired 2015-09-12 per Google: per-channel effect with delay memory
  switching; flanger mention is from a search summary only.
- Casio family members of US10109265B2 (JP6575585B2, JP7375317B2, JP7427957B2), listed by Google as
  citing it; not opened.
- Product manuals (Korg, Yamaha, Kurzweil, Alesis, Eventide, Native Instruments, Strymon, Softube,
  Apple Logic) describe chorus, flanger, tempo-synced LFOs, random LFOs, stereo LFO offset and
  through-zero flanging; none gives a patent number.

## Claim analysis

Stage 2, 2026-11-13, by Claude. Claims are quoted from the Google Patents text of each patent (see
Search log), not OCR. The fetch tool returns quotes in fragments of under 125 characters; the
fragments are joined here without change. No granted PDF was opened or OCR'd.

### Row 1: US10109265B2 (Casio), priority 2016-09-27

Independent claims: 1 (apparatus), 9 (method), 11 (storage medium). Claim 7 (instrument) depends
on claim 1.

**Claim 1** (apparatus).

| Element (quoted) | Planned design |
|---|---|
| "An effect providing apparatus comprising: a processor, wherein the processor performs" | Met in the sense that the effect nodes run as software on a processor (design intro). |
| "first effect processing for providing inputted musical sound data with a first effect based on a parameter value that varies with time;" | Met: Chorus or Flanger varies delay time with the LFO (items 1, 5, 6, 7). |
| "second effect processing for providing, when provision of a second effect different from the first effect is specified while the first effect is being provided by the first effect processing, the inputted musical sound data with the second effect in place of the first effect;" | No such element. The design has no second effect that replaces the Chorus or Flanger on request; each node processes its input every sample. |
| "parameter update continuation processing for continuing variation in the parameter value while the second effect is being provided by the second effect processing; and" | No such element, as there is no second-effect period. The LFO phase advances every sample the node runs (item 1), but not "while the second effect is being provided". |
| "control processing for controlling such that, when the specification of the provision of the second effect is released, the first effect processing provides the inputted musical sound data with the first effect based on the parameter value continued to be varied by the parameter update continuation processing, and" | No such element: no release of a second effect and no resume. |
| "wherein the first effect processing does not provide the inputted musical sound data with the first effect while the second effect processing is providing the inputted musical sound data with the second effect." | No such element. |

Verdict, claim 1: **Does not read.** Missing: the "second effect processing" in place of the first
effect, the "parameter update continuation processing" during it, and the "control processing" that
resumes the first effect from the continued parameter.

**Claim 9** (method). "An effect providing method for an effect providing apparatus, comprising:
providing inputted musical sound data with a first effect based on a parameter value that varies
with time; providing, when provision of a second effect different from the first effect is
specified while the first effect is being provided, the inputted musical sound data with the second
effect in place of the first effect processing; continuing variation in the parameter value while
the second effect is being provided; and controlling such that, when the specification of the
provision of the second effect is released, the first effect is provided based on the parameter
value continued to be varied, wherein the first effect is not provided on the inputted musical sound
data while the second effect is being provided on the inputted musical sound data." Mapping as for
claim 1. Verdict: **Does not read**, same missing elements.

**Claim 11** (storage medium). "A non-transitory computer-readable storage medium having stored
thereon a program that is executable by a computer in an effect providing apparatus to perform
functions comprising:" the same four steps as claim 9, word for word as fetched. The design is
software, so the medium preamble could be met; the replacement, continuation and resume steps are
not. Verdict: **Does not read**, same missing elements.

Dependent claims name the design's features only under claim 1: claim 2 "tempo update processing
for updating a tempo of a musical sound being played back to a specified tempo, and wherein the
parameter update continuation processing continues to vary the parameter value based on the
specified tempo" (claims 8, 10, 12 likewise); claim 5 "a value in accordance with a phase of a low
frequency oscillator"; claim 6 "a value in accordance with an angular velocity of a low frequency
oscillator". They narrow claims that do not read, so cannot read.

**Patent verdict: Does not read.** The design was checked for any effect-replacement and resume
mechanism and has none. Prior art in this note before the 2016-09-27 priority bears on tempo-synced
timing in general (US5430243A, see [delay-patents.md](delay-patents.md)), not on the claimed
switching; none was compared to it.

### Row 2: US8060226B2 (Creative Technology), priority 2006-08-01

Independent claims: 1 (device), 11 (method), 21 (delay line), 29 (medium), 30 (means-plus-function
system). Claims 11, 29 and 30 recite claim 1's two steps in the same words.

**Claim 1** (device).

| Element (quoted) | Planned design |
|---|---|
| "A signal processing device to fractionally modulate a write pointer to provide one or more fractional delay lines, the signal processing device comprising:" | Not met. The design modulates the read position (item 5); the write pointer advances one whole frame per sample and is never modulated. |
| "circuitry configured to generate a fractional address for each of a plurality of sample periods by adding a delay value and a fractional offset value; and" | Partly. The design computes a fractional *read* position from base delay plus depth x LFO (item 5). It forms no fractional *write* address. |
| "processing circuitry configured to generate interpolated input sample values based on a fractional portion of the fractional address" | No such element. The input (plus flanger feedback, item 7) is written as is; interpolation (4-point Hermite) is applied to samples read out of the buffer, not to input samples before writing. |
| "and to perform a write operation to an address associated with an integer portion of the fractional address for each sample period using the interpolated input sample values." | No such element. Writes go to the whole-frame write pointer, with uninterpolated samples. |

Verdict, claim 1: **Does not read.** Missing: generation of "interpolated input sample values" and
the write of them at "an integer portion of the fractional address". Claims 11 ("A method of
modulating a write pointer ..."), 29 ("A computer-readable medium ... to modulate a write pointer
...") and 30 ("A system to modulate a write pointer ...") each recite "generating interpolated input
sample values based on a fractional portion of the fractional address and performing a write
operation to an address associated with an integer portion of the fractional address for each sample
period using the interpolated input sample values" (claim 29 "and to perform"); **Does not read**,
same missing elements.

**Claim 21** (delay line).

| Element (quoted) | Planned design |
|---|---|
| "A fractionally modulated digital delay line comprising: a delay line memory to provide two or more fractional delay lines," | Arguably met for Chorus with 2 to 4 voices: one circular buffer per channel serves several fractional read taps (items 5, 6). Not met for Flanger (one tap per channel, item 7). |
| "wherein a write pointer of the delay line memory is fractionally modulated by a dynamic changing of addresses associated with integer portions of fractional addresses, and" | No such element. The write pointer advances by exactly one frame per sample; it is derived from no fractional address and is not modulated. |
| "wherein each of the one or more fractional delay lines have differing delay lengths defined by different read pointers and the fractionally modulated write pointer." | Partly: Chorus voices have differing delays from different read positions, but measured against an unmodulated write pointer. |

Verdict, claim 21: **Does not read.** Missing: a write pointer "fractionally modulated by a dynamic
changing of addresses associated with integer portions of fractional addresses".

On interpolation and pointer modulation: the claims require interpolation of *input* samples on the
*write* side; dependent claims 2, 12 and 24 specify "a linear interpolation on the input sample
values using the fractional portion of the fractional address", and claims 3 to 5, 13 to 15, 23, 25,
26 handle write addresses skipped or duplicated as the "fractional offset value" rises or falls. No
claim recites Hermite or cubic interpolation, or interpolation of read-side samples. Claim 5's
"delay counter configured to generate the delay value and to increment the delay value for each
sample period" corresponds to the design's write pointer, but every independent claim also needs the
fractional offset and the write-side interpolation.

**Patent verdict: Does not read.** Prior art in this note before the 2006-08-01 priority, for the
read-side technique the design uses: US4569268A (1981, modulated read address), US5243658A (1990)
and US5559300A (1993, LFO-varied write-to-read lag), Laakso et al. 1996 and Dattorro 1997
(fractional and modulated delay reads), Catmull and Rom 1974 (the interpolating spline).

### Row 3: EP3105753B1 (Jobomusic AG), priority 2014-02-11

Independent claim: 1 only (claims 2 to 13 refer back; claim 8 refers to claims 1 to 4). Claims are
in English on the Google Patents page; the German text was not seen.

**Claim 1** (method).

| Element (quoted) | Planned design |
|---|---|
| "Method for the synthetic generation of a digital audio signal by using recurrently calculated amplitude values of a waveform" | Unclear. The smooth random LFO (item 2) computes amplitude values of a waveform every sample, but at 0.05 to 5 Hz it is a control waveform that sets delay time (item 5); it is never output as audio. The audio the node outputs is the delayed input, not this waveform. |
| "which is determined by a course which depends on period phase or frequency" | Met by the smooth random LFO: its value is a function of the LFO phase (items 1, 2). |
| "and is formed by approximation or interpolation between a number of control points which are formed by amplitude-period phase, magnitude-frequency or phase-frequency value pairs," | Met by the smooth random LFO: random values placed at the cycle's wrap points, joined by cosine or cubic Hermite interpolation across the cycle, are amplitude-period phase pairs. Not met by sine, triangle, saw, square or sample-and-hold, which are fixed functions or held values with no interpolated control points. Not met by the 4-point Hermite delay read, which interpolates stored samples in time, not a period-phase course. |
| "wherein the parameter values and/or other attributes of the control points can be changed by respectively associated control signals," | Unclear. The control points' values are replaced from the PRNG at each wrap (item 2). Whether a PRNG draw is an "associated control signal" (the claim's examples in the description were not read) decides it. |
| "and wherein the calculation of the amplitude values is based on the approximation or interpolation of the control points determined by the currently existing control signals." | Met if the previous element is: each LFO value is the interpolation between the current pair of random points. |

Verdict, claim 1: **Unclear.** Deciding elements: "synthetic generation of a digital audio signal"
(whether a sub-audio LFO used internally to modulate delay time is a "digital audio signal", and
whether the preamble limits the claim) and "control signals" (whether PRNG draws at each wrap are).
Dependent claim 4 names "polynomial-based interpolation, Bezier curves, b-splines, or NURBS"; cubic
Hermite is polynomial-based, so claim 4 would follow claim 1 for the Hermite-joined variant. Claim
11 ("the control points are temporally changed in accordance with a modulation function stored in a
memory unit (4)") is a closer reading of change at each wrap only if the PRNG is a stored modulation
function, which it is not as described.

National status, from the Google Patents legal events (text, not the EP Register): no opposition
filed (26N, effective 2018-11-02); opted out of the Unified Patent Court (2023-07-12); annual fees
paid in 2025 for ES, BE, IT and in 2026 for SE, GB, DE, FR, CH; lapsed in DK, IE, NL, NO, FI, AT and
most other states (PG25 events 2018 to 2023); HU recorded "invalid ab initio". Validations DK, ES
and NO (T3 publications) are listed among family members. The EP patent does not reach the US; the
US family member US9741329B2 (filed 2015-02-09, printed "Active", expiry 2035-02-09, small-entity
maintenance fees paid 2021-02-17 and 2025-02-06) has a sole independent claim 1 that is broader:
"A method for synthetic generation of a digital audio signal, the method comprising: providing, by a
synthesizer, a number of control points, each of which is given by an amplitude-period phase, a
magnitude-frequency, or a phase-frequency value pair, wherein the control points are changeable by
respective associated control signals; and on the basis of the control points, repeatedly
performing, by the synthesizer, the further steps of: defining a wave function by interpolating or
approximating between the control points; calculating amplitude values of the wave function; and
using the amplitude values for generating the digital audio signal." Its last step could be met
indirectly, because the LFO values set the delay that produces the output. It is not a candidate
row and is not given a verdict here; see "Open uncertainties".

**Patent verdict: Unclear.** Prior art in this note before the 2014-02-11 priority: Belcham,
musicdsp.org, 2009-06-30 (random levels linearly interpolated, meant for modulating delay lines);
Catmull and Rom 1974 (interpolating splines through control points); the Max `rand~` description
(undated, unverified). None was compared element by element to claim 1.

## Decision

2026-11-13, Chris Paro, through `flag-risk` (tag `patent`): the smooth random shape is dropped
from this work. Rows 3 and 4 are left unresolved and nothing built reads on them; the other LFO
shapes (sine, triangle, saw up and down, square, sample-and-hold), the stereo spread, tempo sync,
the Hermite delay read, the chorus and the flanger are built in-house. A design-around for a
smooth random shape needs its own claims check first.

## Search log

All searches 2026-11-13. The web search tool returns a prose summary, not raw result lists, so hit
counts are approximate (about 10 per query unless noted). Google Patents page fetches read dates,
status and abstracts; claims were read only as a gist where stated.

| Database | Query | Date | Hits | Result |
|---|---|---|---|---|
| Web (general) | 1. patent chorus effect digital delay line modulated by low frequency oscillator multiple voices | 2026-11-13 | 10 | US4144790; tutorials |
| Web (general) | 2. patent chorus effect apparatus digital delay memory read address modulated low frequency oscillator Yamaha OR Roland OR Korg OR Casio | 2026-11-13 | 10 | US4569268, US5243658, US5000074, JPS5814191A, JPS58108583A, JPS6332393B2 (JP not opened) |
| Web (general) | 3. patent flanger digital delay modulated feedback LFO delay line interpolation audio effect | 2026-11-13 | 10 | No patent; JOS and Basa papers |
| Web (general, extended) | 4. patent LFO synchronized to tempo musical note value period "low frequency oscillator" modulation effect audio plug-in beat sync chorus flanger phaser | 2026-11-13 | 9 | US10109265 |
| Web (general, extended) | 5. patent "low frequency oscillator" random waveform "sample and hold" interpolated smooth random control signal audio effect parameter modulation granted | 2026-11-13 | 9 | EP3105753A1, US10262646, US5668338A, US4924744 |
| Web (general) | 6. patent random low frequency oscillator sample and hold smooth random modulation chorus flanger audio effect | 2026-11-13 | 15 | musicdsp smooth random; US4329902 |
| Web (general, extended) | 7. patent stereo chorus left right channel LFO phase offset delay modulation spread audio effect claims | 2026-11-13 | 9 | No patent; manuals |
| Web (general) | 8. patent LFO phase accumulator audio effect plugin random LFO "decorrelated" OR "uncorrelated" left right channel modulation chorus patent application | 2026-11-13 | 10 | No patent; Korg manuals |
| Web (general, extended) | 9. patent modulation effect audio plug-in LFO waveform sine triangle square sawtooth random sample and hold stereo phase feedback delay "chorus" "flanger" US patent 2010..2024 | 2026-11-13 | 9 | US10957297 (flanger plug-in named only, claims not opened) |
| Web (general, extended) | 10. US patent audio effect LFO "tempo" synchronous modulation speed note length chorus flanger phaser electronic musical instrument effect apparatus Roland Yamaha Korg Casio 2010s | 2026-11-13 | 9 | US10109265 |
| Web (general) | 11. patent musical sound effect "LFO" frequency "synchronized with tempo" OR "in synchronization with the tempo" modulation delay chorus phaser Yamaha OR Roland OR Korg | 2026-11-13 | 10 | No patent; Korg and Yamaha manuals |
| Web (general) | 12. patent delay time modulation random waveform generated at each cycle random values interpolated modulation audio effect delay line chorus "random" LFO patent Eventide OR Lexicon OR Line 6 OR Roland | 2026-11-13 | 10 | US5559300, US4164884, US4384505, US5740716, WO2001039171A1 |
| Web (general, extended) | 13. patent fractional delay cubic Hermite interpolation modulated delay line audio effect flanger chorus claim | 2026-11-13 | 9 | US5742532, WO1998010355, JP2000509840A, US8060226 |
| Web (general, extended) | 14. patent variable delay line interpolation Lagrange OR Hermite OR "cubic" fractional read pointer modulated delay audio effect granted claims Creative OR Native Instruments OR Ableton OR Apple | 2026-11-13 | 10 | US8060226, EP0979463B1, US5781461; arXiv cs/0005022 |
| Web (general) | 15. patent multi-voice chorus voices LFO phases evenly spaced delay taps summed ensemble effect digital | 2026-11-13 | 10 | US4384505, US4144790; manuals |
| Web (general) | 16. patent flanger "negative feedback" OR "positive feedback" delay line modulated dry wet mix digital effects processor claim 1 flanging | 2026-11-13 | 10 | No patent; manuals |
| Web (general, extended) | 17. patent flanger effect feedback delay time sweep digital audio "flanger" claims 2008..2024 granted | 2026-11-13 | 9 | US7444194, US7865257, US5703312 |
| Web (general, extended) | 18. patent "modulation effect" OR "modulated delay" audio effect apparatus "LFO" "delay time" "feedback" digital signal processor musical instrument US2010 OR US2015 OR US2020 chorus vibrato flanger granted | 2026-11-13 | 10 | US4205579 only |
| Web (general) | 19. Eventide OR "Native Instruments" OR Ableton OR "Universal Audio" patent chorus flanger modulation delay line LFO claims | 2026-11-13 | 10 | No patent; manuals |
| Web (general) | 20. patent through-zero flanger OR "stereo flanger" OR "phase offset between left and right LFO" digital audio effect granted | 2026-11-13 | 20 | No patent; manuals (out of scope: through-zero) |
| Web (general) | 21. patent four-point cubic Hermite OR Catmull-Rom interpolation audio sample delay read pointer fractional position claim -graphics | 2026-11-13 | 10 | US5981860 (two-point); no Hermite patent |
| Web (general) | 22. Google Patents "chorus" effect modulated delay "low frequency oscillator" electronic musical instrument 1980s digital chorus patent Roland "Device for producing chorus effects" US4205579 | 2026-11-13 | 10 | US4205579, US4072079, US4080861, US4038898 |
| Web (general) | 23. Dattorro "Effect Design Part 2: Delay-Line Modulation and Chorus" JAES 1997 (and a follow-up on its content) | 2026-11-13 | 20 | Citation 45(10) 764 to 788 |
| Web (general) | 24. Niemitalo "Polynomial Interpolators for High-Quality Resampling of Oversampled Audio" 2001 (and a follow-up on yehar.com) | 2026-11-13 | 20 | mpg123 comment only |
| Web (general) | 25. Laakso Valimaki Karjalainen Laine "Splitting the Unit Delay" IEEE SPM 1996 DOI; Catmull Rom 1974 "A class of local interpolating splines" | 2026-11-13 | 20 | DOI and citation confirmed by indexes |
| Google Patents (page fetch by number) | US4569268A, US4144790, US5243658A, US5000074A, US10109265B2, US8060226B2, US7444194B2, US7865257B2, US8704068B2, US9479864B2, US7875789B2, US20100011941A1, US4384505, US4205579, US5559300A, US5740716, US5703312A, EP3105753A1, US10262646B2, US4924744, US4164884, US4080861, US4072079, US4038898A, US5981860A, US4329902A, EP1234302A1 | 2026-11-13 | 27 | Dates and status read; claims not read; cited-by lists read in part for US4569268A, US4144790, US5740716, EP1234302A1, US10109265B2 |
| Other pages fetched | musicdsp.org smooth random LFO; Wikipedia Dattorro industry scheme; CCRMA PASP Flanging | 2026-11-13 | 3 | Read |
| Google Patents (claims, stage 2) | US10109265B2 page: claims 1, 9, 11 and dependent claims 2 to 8 quoted in fragments | 2026-11-13 | 1 | Claim text for "Claim analysis" (text, not OCR) |
| Google Patents (claims, stage 2) | US8060226B2 page from character 200000: independent claims 1, 11, 21, 29, 30 and dependent claims 2 to 28 quoted in fragments | 2026-11-13 | 1 | Claim text (text, not OCR) |
| Google Patents (claims, stage 2) | EP3105753B1 page from character 100000: claims 1, 8, 11, 12 quoted, claims 2 to 13 listed; from character 200000: legal events | 2026-11-13 | 1 | Claim text (English, text, not OCR); national status |
| Google Patents (claims, stage 2) | US9741329B2 (US family member of row 3): dates, status, claim 1 quoted, legal events | 2026-11-13 | 1 | Claim text (text, not OCR) |
| Web (general, stage 2) | US10109265B2 Casio effect providing apparatus second effect in place of first effect parameter continued | 2026-11-13 | 10 | Justia and ipiq pages: embodiment has filter as first effect, flanger as second (search summary) |

Refused or failed: a Google Patents search-results page fetch (`patents.google.com/?q=...`)
returned only the page heading with no results. The Dattorro PDF (CCRMA), the Niemitalo PDF
(yehar.com, two URLs, one the blog page which returned only the domain name) and the DAFx-99 paper
(dafx.de, disch.pdf) came back as scanned or compressed binary that the fetch tool could not read.
The musicdsp.readthedocs.io page redirected; the musicdsp.org URL was read instead. Google Patents
pages for US8060226B2, US7444194B2, US7865257B2, US5559300A, US5000074A, US5243658A, US8704068B2,
US7875789B2, US20100011941A1, US5740716, US5703312A, EP3105753A1 and US5981860A were truncated at
100000 characters and the rest not read. Not tried: Espacenet, USPTO Patent Public Search and
Patent Center, FreePatentsOnline, any search by CPC class (for example G10H 1/12, G10H 1/16, G10H
2210/081 flanging, G10H 2210/251 chorus, G10H 2250/131, H03H 17/0009) or by assignee name in a
database. Citation and cited-by lists were not followed for most patents. Japanese patents that
surfaced (JPS5814191A, JPS58108583A, JPS6332393B2) were not opened. Stage 2: the first fetches of
US8060226B2 (characters 0 to 200000), EP3105753B1 (0 to 100000) and US9741329B2 (0 to 100000)
reached no claims, so the claims were read from later offsets; the fetch tool would not quote more
than 125 characters at once, so claims were taken in consecutive fragments. The EP Register and the
granted PDFs were not opened.

## Open uncertainties

- No patent-database search was run (see "Refused or failed"). "No candidate found" for elements
  1 to 3, 5, 6 and 7 is a statement about web-search coverage, not about the databases. Searches by
  CPC class and by assignee (Roland, Yamaha, Korg, Casio, Kawai, Line 6, Eventide, Lexicon, TC
  Electronic, Native Instruments, Ableton, Steinberg, Apple, Creative) are the main gap, and the
  Japanese 1980s modulation-effect patents that surfaced in search 2 were not opened.
- No patent with a priority date after about 2006 was found on LFO shapes, random or smooth-random
  LFOs, stereo LFO offset, multi-voice chorus or flanger feedback; this is the weakest part of the
  search because the web tool returns mostly manuals. Post-2006 candidates found are rows 1 to 3
  only.
- Element 1: no dated source for increment-and-wrap phase (an f64 phase accumulator); US4384505A
  (1980) is a table lookup.
- Element 2: no dated source for square or for smooth random shaped by cosine or cubic Hermite
  between points chosen at each wrap. Belcham (2009) and the Max `rand~` description are the
  nearest and are not patents; the Dattorro "low-pass noise" item is from Wikipedia's account, the
  paper was not read.
- Element 3: no source for a separate decorrelated random stream for the offset read.
- Row 1 (US10109265B2) does not read on the design as written. A human should re-check it if the
  editor ever lets one effect stand in for another on request (an A/B switch, or a bypass that
  substitutes a different effect) while the first effect's LFO keeps running, and resumes it from
  the advanced phase. Whether a plain node bypass that passes dry signal, with the LFO kept running,
  is a "second effect ... in place of the first effect" was not decided; the planned design names no
  bypass.
- Element 5: the content of Niemitalo 2001, Laakso et al. 1996 and Dattorro 1997 was not read; the
  4-point Hermite form is documented here only by Catmull and Rom's bibliography (1974) and
  secondary sources. Row 2 (US8060226B2) does not read because every independent claim needs a
  fractionally modulated write pointer, with interpolated input samples written (claims 1, 11, 29,
  30); the verdict holds only while the write pointer advances one whole frame per sample and the
  input is written uninterpolated. Moving the modulation or interpolation to the write side would
  need a re-check.
- Row 3 (EP3105753B1), Unclear, needs a human (or counsel) to decide: (a) whether a sub-audio LFO
  waveform used only to set delay time is a "digital audio signal" generated "synthetically"; (b)
  whether PRNG draws at each wrap are "control signals"; (c) which markets matter, since the EP
  patent is kept in force only in DE, GB, FR, ES, BE, IT, SE and CH per Google's legal events. The
  description was not read for how it uses "control signals". If the answer to (a) or (b) is in
  doubt, a fallback is to drop the smooth random shape or replace interpolation between random
  points with another smoothing (for example a one-pole low-pass on the sample-and-hold values),
  which a human would need to check against the claim.
- US9741329B2 (Jobomusic, US family member of row 3), printed "Active" to 2035-02-09, was found in
  stage 2 and is not in the Candidates table. Its claim 1 ends "using the amplitude values for
  generating the digital audio signal", which an LFO setting the delay of the output might meet
  indirectly; it also needs "a synthesizer" and "control signals". A human should decide whether to
  add it as a candidate row, mark its status, and have its claims analysed; it is the US exposure
  for the smooth random shape.
- Term end dates are as Google prints them; terminal disclaimers and adjustments not otherwise
  seen.
