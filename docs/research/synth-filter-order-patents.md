# Patent status of the synth low-pass's selectable order

Research note, 2026-11-26. This is not legal advice. It records what primary sources show, so a
reviewer can check it. Stage 1 (evidence gathering) listed the candidates; stage 2 (same day) read
the independent claims of the three in-force ones against Planned design (see Claim analysis). Dates
in the Candidates table and the expired list are as Google Patents prints them (Google states its
status and expiry fields are assumptions, not legal conclusions).

## Planned design

Issue #206 gives the Synthesizer's per-voice low-pass (issue #173, cleared for one SVF in
ADR-0148 and `docs/research/synth-patents.md`) a selectable slope. Software, portable scalar Rust,
`f32`, per sample on the real-time audio thread. Each element as it will be built:

1. **Order choice.** A stepped (not automatable) parameter `filter order` with options `6 dB`,
   `12 dB`, `18 dB`, `24 dB` per octave; default `12 dB`.
2. **Stages.** Each voice holds two trapezoidal (TPT) state-variable filters (`dsp::Svf`, Simper's
   form, as in `docs/research/eq-filter-patents.md` element 1) and one TPT one-pole low-pass
   (`dsp::OnePole`, element 4 there). Per order, the signal passes through:
   - 6 dB: the one-pole only.
   - 12 dB: one SVF low-pass (today's filter).
   - 18 dB: one SVF low-pass, then the one-pole low-pass.
   - 24 dB: two SVF low-passes in series.
3. **Shared cutoff and resonance.** Every stage in use gets the same cutoff. Both SVFs in the
   24 dB order get the same Q (the user's `resonance`, 0.5–12): it is a resonant cascade of equal
   sections, not a Butterworth cascade with per-section pole Qs. The one-pole has no Q.
4. **Cutoff modulation.** Unchanged from ADR-0148: the cutoff is computed per sample in log2 Hz
   from the base cutoff, filter envelope (± octaves), velocity amount and LFO, clamped to 20 Hz …
   min(20 kHz, 0.49·fs); coefficients (`tan` prewarp) are recomputed every sample and shared by all
   stages in use.
5. **Changing order.** `filter order` is stopped-only (as the equalisers' order is, ADR-0088): it
   changes only while playback is stopped or paused, and takes effect at once from rest, with
   every stage reset to zero. No crossfade and no second, preallocated filter set.
6. **No gain compensation.** No level correction across orders or for resonance.
7. **Resonance inactive at 6 dB.** The `resonance` control is greyed out while `6 dB` is chosen;
   it keeps its value and has no effect.

(Elements 5 and 7 revised on 2026-11-26, during stage 1.)

## Summary

Stage 2 (claim reading) done 2026-11-26 for the three in-force candidates; none reads on the
design. Per-patent verdicts and the claim text are under Claim analysis. Searches (see Search log)
found no other in-force patent that touches the new elements 1, 2, 3 and 5; every other patent they
turned up on cascaded or selectable-pole filters has expired by term (list under Candidates). The
search tools were weak (prose summaries only; Google Patents, USPTO and Espacenet query interfaces
were unusable), so "none found" rests on web searches, not a classified database search; Confidence
is therefore not High for an element unless a read claim is all that bears on it.

| Element | Verdict | Confidence |
|---|---|---|
| 1. Order choice 6/12/18/24 dB | No in-force candidate reads. Row 2 (US9722560B2, Meyer Sound) claims a user-selectable order only for a band filter section of high and low boost-or-cut *shelving* filters, with order-dependent gain correction to a base band gain; the design's selectable order is a resonant low-pass with no shelf, no band gain and no correction. No other in-force candidate found (searches 1, 4, 5, 9, 10, 11, 14). | Medium (search coverage; the row 2 reading itself is High) |
| 2. Stages: SVF, one-pole, SVF + one-pole (3-pole), two SVFs (4-pole) | No in-force candidate reads (rows 1-3 checked; none claims a cascade of pass sections). Expired art for cascaded two-pole state-variable sections: US4934770A (Beltone) and US4563652A (Audio Precision). No patent found for an SVF followed by a one-pole to make three poles. | Medium |
| 3. Equal shared cutoff and Q across stages | No in-force candidate reads. Row 1 (US11202147B2, Rossum) needs a resonant frequency that varies through zero; every stage takes the same clamped, positive cutoff, so none does. Expired: US3924199A (ARP), US4934770A. | Medium |
| 4. Per-sample log2-Hz cutoff modulation, clamp, prewarp | Does not read on row 1 (re-confirmed for cascaded stages; see Claim analysis). Otherwise cleared by ADR-0088, ADR-0148, `docs/research/eq-filter-patents.md`, `docs/research/synth-patents.md`. | High |
| 5. Order change: stopped-only, at once from rest, every stage reset, no crossfade, no backup set | **Does not read** on row 3 (US10311845B2, Casio). Every independent claim needs crossfading processing ("gradually decreasing" one filter's contribution and "gradually increasing" a backup filter's); the design switches in one step at rest. Holds even if the second SVF a 24 dB choice adds is called a backup partial filter. No patent found on resetting stages or on limiting an order change to a stopped transport (searches 6, 12, 19). | High |
| 6. No gain compensation | Omission, nothing to match. Row 2's claims all need order-dependent gain correction; the design has none. | High |
| 7. Resonance control inactive (greyed out) at 6 dB | A user-interface state: the one-pole has no Q. No candidate claims it; no patent found (search 19). Korg Pa-series manuals (undated) grey out parameters that a routing does not use, so the practice is old in products but not cited as prior art. | Medium |

## Prior art

Items marked (unverified) came from a search summary or an index and I could not open the primary
text. PDFs could not be read in this environment (no PDF renderer); see Open uncertainties. Product
manuals (Apple Logic ES2, Applied Acoustics CV-2/CV-3, Surge OB-Xf, u-he Diva) show that 6/12/18/24
dB, 2-pole/4-pole and tap-per-pole modes are common in synthesizers, but their pages carry no date
and are not cited as prior art.

- **Elements 1, 3 (a four-pole resonant low-pass with feedback; slope set by pole count).** US3475623A
  (Robert A. Moog), priority and filing 1966-10-10, granted 1969-10-28, four-pole low-pass filter
  built from base-emitter diode resistances (title: electronic high-pass and low-pass filters); the
  "four-pole with feedback" description comes from a secondary page
  ([till.com](https://till.com/articles/moog/patents.html)), the patent's abstract on
  [Google Patents](https://patents.google.com/patent/US3475623A/en) mentions only the programmable
  filters. US3924199A (ARP, Pearlman), filed 1974-02-04, granted 1975-12-02: four low-pass sections in
  series with a common control and an adjustable feedback resistor setting resonance
  ([Google Patents](https://patents.google.com/patent/US3924199A/en)). US4011466A (ARP), filed
  1976-05-10, granted 1977-03-08: series-connected stages in a resonant filter
  ([Google Patents](https://patents.google.com/patent/US4011466A/en)). US4404529A (Rossum, Dow),
  filed 1980-12-30, granted 1983-09-13: four-stage low-pass with feedback, per a search summary
  ([Google Patents](https://patents.google.com/patent/US4404529A/en)).
- **Elements 2, 3 (two two-pole state-variable sections in series sharing one tuning).** US4563652A
  (Audio Precision, Hofer), filed 1985-04-12, granted 1986-01-07: two cascaded state variable filters
  of identical topology, both tuned to the same frequency (band-pass/reject use)
  ([Google Patents](https://patents.google.com/patent/US4563652A/en)). US4934770A (Beltone), filed
  1988-06-06, granted 1990-06-19: two 2-pole state variable filters in series, one control circuit
  keeping corner frequency changes from altering the Q, giving a four-pole high-pass or low-pass
  ([Google Patents](https://patents.google.com/patent/US4934770A/en); page text read to character
  100000 only). Both are hardware, not zero-delay-feedback software.
- **Element 2 (a modern digital reference for SVF pairs and ladder taps).** Lazzarini and Timoney,
  "Linear and Nonlinear Digital Filters: From the Analog and Beyond", Computer Music Journal 45(2),
  pp. 67-83, 2021, [DOI 10.1162/comj_a_00599](https://doi.org/10.1162/comj_a_00599)
  ([abstract](https://mural.maynoothuniversity.ie/id/eprint/19872/)). A search summary said the
  paper uses two SVFs in series for a ladder-like four-pole response; the abstract I opened does not
  say so (unverified). The date, 2021, is after the 2008-2013 sources named in the EQ note and
  adds nothing earlier. D'Angelo, Aalto doctoral thesis on the Moog ladder, 2014: a search summary
  says it notes that digital models need not keep four stages
  ([Aalto](https://aaltodoc.aalto.fi/server/api/core/bitstreams/7ebb6719-8c32-4e23-97ce-b6ebac589990/content),
  not opened).
- **Elements 2, 3 (TPT SVF and TPT one-pole, prewarp).** See `docs/research/eq-filter-patents.md`,
  "Prior art" (Zavalishin, Simper). The Zavalishin book revision 2.1.0 URL came up in a search
  (<https://www.native-instruments.com/fileadmin/ni_media/downloads/pdf/VAFilterDesign_2.1.0.pdf>)
  but returned 404 when fetched. Whether the book describes mixing stage outputs of a cascade for
  selectable slope is not established.
- **Element 1 (slope varied in an instrument filter).** US4114498A (Nippon Gakki), priority
  1975-10-23, filed 1976-10-21, granted 1978-09-19: a slope factor a(t) varied continuously through a
  tone, with a time-varying cutoff, applied to harmonic amplitudes
  ([Google Patents](https://patents.google.com/patent/US4114498A/en)). Not a stepped pole-count
  choice.
- **Element 1 (selectable pole count in a digital filter).** US4554858A (Nippon Gakki), priority
  1982-08-13, filed 1983-08-04, granted 1985-11-26: pole and zero filters connected in different
  orders or alone, and the number of operation stages changed
  ([Google Patents](https://patents.google.com/patent/US4554858A/en); text read to character 100000
  of 373979). EP0285316A2 (Tektronix), priority 1987-04-03: programmable filter of 1 to 8 stages,
  refused 1993-11-18, not a synthesizer
  ([Google Patents](https://patents.google.com/patent/EP0285316A2/en)).
- **Element 5 (reset of unused stages).** No dated source found. Related only in a general sense:
  US4809208 (programmable multistage digital filter with a reset signal) and US6167415 (recursive
  digital filter with reset, to avoid DC offset), both from search summaries and not opened.

## Candidates

Patents that might cover an element of the design and might still be in force. Status is filled
in by a human from Google Patents or USPTO Patent Center. All three rows come from the earlier
notes; the new searches added no row. Term end is as printed there.

| # | Patent | Assignee | Priority / filing | Est. term end | Element it may touch | Link | Status |
|---|---|---|---|---|---|---|---|
| 1 | US11202147B2, Audio filter with through-zero linearly variable resonant frequency (read in `docs/research/eq-filter-patents.md` row 1: does not read) | Rossum Electro-Music LLC | 2018-12-26 / 2019-12-20 | 2039-12-20 (20 yr from filing; term adjustment not seen) | 4 (per-sample modulated SVF cutoff; the clamp below 0.49·fs stays) | [GP](https://patents.google.com/patent/US11202147B2/en) | In force |
| 2 | US9722560B2, Filter with independently adjustable band gain and break point slopes (read there, row 5: does not read for EQ shelving) | Meyer Sound Laboratories Inc | 2013-05-31 / 2014-05-31 | 2035-04-12 (Google adjusted) | 1 (user-selectable order of a filter in 6 dB steps; this note's filter is a resonant low-pass with no order-dependent gain correction, element 6) | [GP](https://patents.google.com/patent/US9722560B2/en) | In force |
| 3 | US10311845B2, Filter characteristics changing device (read there, row 8: unclear, leans reads for crossfade to a backup cascade) | Casio Computer Co., Ltd. | 2017-03-15 / 2018-02-20 | 2038-02-20 (Google) | 5 (crossfade between partial filters, backup filter set up in advance; element 5 uses neither a fade nor a backup set) | [GP](https://patents.google.com/patent/US10311845B2/en) | In force |

Statuses checked by Chris Paro on 2026-11-26.

Status values: **In force**, **Lapsed (fees)**, **Expired (term)**, **Abandoned**, **Invalidated**,
**Not found**. Record who checked and when, under the table.

Seen and not listed as candidates: US7613311B2 (Cirrus Logic, filed 2004-12-15; Google adjusted
expiry 2027-10-31 but its legal events show it lapsed for fees in late 2021): a fixed fourth-order
Linkwitz-Riley crossover with a single coefficient, no selectable order or resonance
([Google Patents](https://patents.google.com/patent/US7613311B2/en)). US20010033196A1 (National
Instruments, filed 2001-04-20, abandoned): an analog state variable filter with programmable
resistors ([Google Patents](https://patents.google.com/patent/US20010033196A1/en)). US7558171 (an
optical disc device where a selection circuit picks a first- or second-order low-pass, per a search
summary): not opened, not a synthesizer; see Open uncertainties.

Expired by term (filing date + 20 years, plus any printed term adjustment, is in the past), listed
for completeness and needing no check. Dates are as Google prints them.

- [US3475623A](https://patents.google.com/patent/US3475623A/en), Moog, filed 1966-10-10, expiry
  1986-10-28. Four-pole ladder low-pass (elements 1, 3).
- [US3924199A](https://patents.google.com/patent/US3924199A/en), ARP Instruments, filed 1974-02-04,
  expiry 1992-12-02. Four cascaded low-pass sections, shared control, feedback resonance (elements 2, 3).
- US3974461 (Moog, Luce), granted 1976-08-10 per [till.com](https://till.com/articles/moog/patents.html);
  Google page not opened. Filing cannot be later than the grant, so 20 years from filing ended by
  1996-08-10. Cascaded filter stages with variable feedback (elements 2, 3).
- [US4011466A](https://patents.google.com/patent/US4011466A/en), ARP, filed 1976-05-10, expiry
  1994-03-08. Series-connected resonant stages (elements 2, 3).
- [US4404529A](https://patents.google.com/patent/US4404529A/en), Rossum and Dow, filed 1980-12-30,
  expiry 2000-12-30 (lapsed for fees 1995). Four-stage low-pass with feedback (elements 2, 3).
- [US4114498A](https://patents.google.com/patent/US4114498A/en), Nippon Gakki, filed 1976-10-21,
  expiry 1996-10-21. Time-varying filter slope (element 1).
- [US4489391A](https://patents.google.com/patent/US4489391A/en), Casio, filed 1982-02-17, expiry
  2002-02-17. Digital resonant low-pass; no selectable order claimed (element 3).
- [US4554858A](https://patents.google.com/patent/US4554858A/en), Nippon Gakki, filed 1983-08-04,
  expiry 2003-08-04. Digital filter with switchable pole/zero connection and stage count (element 1).
- [US4514704A](https://patents.google.com/patent/US4514704A/en), individual (Curtis), filed
  1983-08-26, expiry 2003-08-26. Cascadable low-pass stages (element 2).
- [US4563652A](https://patents.google.com/patent/US4563652A/en), Audio Precision, filed 1985-04-12,
  expiry 2005-04-12. Two cascaded identically tuned SVFs (elements 2, 3).
- [US4934770A](https://patents.google.com/patent/US4934770A/en), Beltone, filed 1988-06-06, expiry
  2007-06-19. Two cascaded 2-pole SVFs under one control, four-pole response (elements 2, 3).
- [US7206419B1](https://patents.google.com/patent/US7206419B1/en), Industrial Research Ltd, filed
  1998-11-20, expiry 2018-11-20. Band-split guitar preamp; SVFs only in dependent claims (element 2).
- [EP0285316A2](https://patents.google.com/patent/EP0285316A2/en), Tektronix, filed 1988-03-23;
  refused 1993-11-18, twenty years ended 2008-03-23. Programmable 1 to 8 digital filter stages (element 1).

## Claim analysis

Claim text source: each patent's Google Patents page (`https://patents.google.com/patent/<number>/en`),
fetched again 2026-11-26 for this note. Google serves this as machine-readable text, which I did not
OCR. The fetch tool relays page text through a summarizer and caps each quote at 125 characters, so
claims were pulled in consecutive verbatim pieces; joins between pieces are marked " ... " where
text is left out. A reviewer should check the decisive elements against the granted PDF. Element
numbers refer to Planned design (with elements 5 and 7 as revised).

### Row 1: US11202147B2 (Rossum), priority 2018-12-26: **Does not read**

Independent claims 1, 15, 21; read in full in `docs/research/eq-filter-patents.md` "Row 1". The
question here is only whether cascading stages under one cutoff changes that reading. It does not.

- Claim 1: "receiving an input signal;" and "receiving one or more control input signals; and": met
  (voice signal; cutoff, envelope, velocity, LFO and `resonance`, element 4). "providing a resonant
  filter for electronic music for processing the input signal,": met (each SVF with Q, elements 2
  and 3). "the resonant filter being responsive to the one or more control input signals to produce
  a resonant frequency" ... "that varies through zero and is at least linearly controlled,": **not
  met**. Element 4 clamps the cutoff to 20 Hz … min(20 kHz, 0.49·fs), and element 3 gives every stage
  in use that same cutoff, so each SVF's resonant frequency stays strictly positive. Cascading two
  equal SVFs, or an SVF and a one-pole, adds no stage with a frequency of its own; the one-pole has no
  resonance at all. "such that the resonant filter provides an output signal that has timbral
  modulation of the input signal.": not met (follows from the missing element).
- Claim 15: "a state variable filter configured to provide at least one of lowpass, bandpass, and
  highpass outputs,": met (element 2, low-pass output). "the state variable filter capable of having
  its resonant frequency" ... "linearly modulated through zero into negative frequency; and": **not
  met** (same clamp, applied to every stage). "circuitry for configuring the state variable filter
  such that" ... "to produce a resonant frequency" ... "that varies through zero": not met.
- Claim 21: "linearly modulating a resonant frequency of the at least one resonant filter through
  zero frequency" ... "to enable negative frequency modulation": **not met**. "performing exponential
  times linear multiplication to determine the resonant frequency,": not met; element 4 sums
  modulation in log2 Hz (exponential only) with no linear term multiplied in.

Missing element: a resonant frequency that passes through zero into negative frequency, in any stage.
Prior art in this note before 2018-12-26: US4563652A (1986) and US4934770A (1990), two cascaded SVFs
under one tuning; neither shows through-zero modulation.

### Row 2: US9722560B2 (Meyer Sound), priority 2013-05-31: **Does not read**

Independent claims 1, 8, 10. Claims 2-7 depend on 1, 9 on 8, 11-13 on 10. Claim 7 (dependent) names
"n=1 for a first order filter, n=2 for a second order filter, and n=3 for a third order filter" ...
"user selectable break point slopes of 6 dB/octave, 12 dB/octave, and 18 dB/octave"; the stepped
choice is the overlap with element 1, but it sits under claim 1's shelving and correction elements.

- Claim 1: "A filter for equalizing the frequency response of loudspeaker systems comprising:":
  preamble; the design is a synthesizer voice filter, not loudspeaker equalization (whether the
  preamble limits is not decided here; nothing turns on it). "at least one band filter section
  comprised of an n-order high boost or cut shelving filter" ... "having a break point frequency, ω1,
  and an n-order low boost or cut shelving filter" ... "having a break point frequency, ω2, wherein
  ω1<ω2,": **not met**. Elements 2 and 3 give low-pass stages only: no shelf, no boost or cut level,
  and no pair of break points bounding a band. "and wherein the order, n, of at least one of said
  shelving filters" ... "of the band filter section is user selectable for adjusting the slope of the
  shelving filter" ... "at the break point frequency of said shelving filter,": not met; element 1's
  selectable order is a low-pass slope, not a shelving filter's. "and wherein said high and low
  n-order shelving filters" ... "have substantially the same gain and produce a resultant band gain
  for the band filter section, and": not met; there is no band gain. "gain correction means for the
  n-order high shelving filter and n-order low shelving filter for correcting" ... "the resultant band
  gain for the band filter section to a base gain level, said correction means providing" ... "a
  correction factor dependent on the order of the shelving filters.": **not met**; element 6 has no
  level correction across orders or for resonance.
- Claim 8: "multiple band filter section, each of said band filter sections being comprised of an
  n-order high boost or cut" ... "shelving filter" ... "and an n-order low boost or cut shelving
  filter": not met (as claim 1). "gain correction means" ... "providing a correction factor dependent
  on the order of the shelving filters, and": **not met** (element 6). "gain control means for
  adjusting the corrected-to base gain of each band filter section,": not met; the filter has no gain
  control (the voice's level is not a band gain of the filter).
- Claim 10: "constructing of n-order high boost or cut shelving filter having a break point"
  ... "and an n-order low boost or cut shelving filter": not met. "choosing as a basic building block
  a first order (n=1) frequency-normalized low shelving filter": not met; the 6 dB order is a one-pole
  low-pass. "establishing a s-plane representation of a frequency-normalized n-order Butterworth-type
  filter," ... "adding n complex zeros to the complex poles": **not met**; element 3 cascades equal
  resonant sections (not Butterworth poles) and adds no zeros. "wherein the gain of said high and low
  n-order" ... "shelving filters are corrected so that they are substantially the same for any order
  n": not met (element 6).

Missing elements: the pair of boost-or-cut shelving filters forming a band (every claim), and the
order-dependent gain correction (claims 1, 8) or added Butterworth zeros (claim 10). Prior art in this
note before 2013-05-31 for a selectable filter order or stage count: US4554858A (1985), EP0285316A2
(priority 1987), US3475623A (1969); none shows shelving with gain correction.

### Row 3: US10311845B2 (Casio), priority 2017-03-15: **Does not read**

Independent claims 1 (device), 8 (method), 10 (storage medium), with the same elements. Claims 2-7
depend on 1, 9 on 8, 11 on 10 (claims 9 and 11 not re-read).

- Claim 1: "a set of filters which includes a plurality of partial filters and which forms a
  characteristic" ... "by combining at least two filters selected from among the plurality of partial
  filters; and": met for 18 dB and 24 dB (two stages in series, element 2); not met for 6 dB and
  12 dB, where one stage is used.
- "a processor which performs, as crossfading processing for a first filter and a second filter"
  ... "selected from among the plurality of partial filters, fade-out processing of gradually
  decreasing" ... "a degree of contribution of the first filter to the characteristic and fade-in
  processing" ... "of gradually increasing a degree of contribution of the second filter to the
  characteristic,": **not met**. Element 5 changes the order in one step while playback is stopped or
  paused, resets every stage to zero and starts from rest; no stage's contribution is lowered or
  raised gradually, and no two configurations ever run together. In a 12 → 24 dB change the first SVF
  stays in the path at full contribution, so nothing fades out either.
- "when an instruction is provided for changing the characteristic formed by the set of filters,":
  met (the user changes `filter order`).
- "wherein the processor selects a partial filter used for characteristic formation before the
  instruction" ... "as the first filter,": not met as a first filter *for crossfading*; there is no
  crossfading processing to select it for.
- "sets a parameter to at least one backup partial filter among the plurality of partial filters,"
  ... "the parameter corresponding to the characteristic indicated by the instruction, and selects":
  arguable, and this is the soft spot. Each voice holds both SVFs and the one-pole at all times
  (element 2); at 12 dB the second SVF is unused, and choosing 24 dB brings it in with the shared
  cutoff and Q. That stage could be called a backup partial filter, and dependent claim 2's "not used
  for the characteristic formation before the instruction and is" ... "associated with the
  characteristic indicated by the instruction." could be said to fit it. Against that, element 4
  recomputes its coefficients every sample from the same cutoff for every order; no parameter is set
  to it *for* the new characteristic. Either way, the next element is not met.
- "the at least one backup partial filter as the second filter for the crossfading processing.":
  **not met**; no crossfading processing.
- Claims 8 and 10: the same elements ("fade-out processing of gradually decreasing a degree" ... "of
  contribution of a first filter to the characteristic and fade-in processing of gradually increasing
  a degree", and "selects the at least one" ... "backup partial filter as the second filter for the
  crossfading processing."): **not met**, for the same reason.

Missing element: crossfading processing, a gradual fade-out of the old filter's contribution with a
gradual fade-in of the backup filter's. The claims' own wording ("gradually") excludes a one-step
switch, and claim 5 sets "processing of gradually changing each parameter" ... "without using the
crossfading processing" apart from crossfading, so the claims treat crossfading as a distinct act.
Treating the added stage as a backup partial filter does not change the verdict. Prior art: none in
this note predates 2017-03-15 on crossfading; `docs/research/eq-filter-patents.md` lists US5781461A
and US6208687B1 (priority 1996, both expired).

## Search log

All searches 2026-11-26. The search tool returns a prose summary, not raw result lists, so hit
counts are the number of links returned (10 unless noted).

| Database | Query | Date | Hits | Result |
|---|---|---|---|---|
| Web (general) | patent synthesizer filter selectable number of poles 12 dB 24 dB per octave switch low-pass digital | 2026-11-26 | 10 | No patent; product manuals (ES2, OB-Xf, Diva-style) |
| Web (general) | patent digital filter selectable slope cascade state variable filters resonance 4-pole low-pass musical instrument | 2026-11-26 | 10 | US3924199, US4114498, US4489391; Lazzarini and Timoney 2021 |
| Web (general) | patent ladder filter taps selectable pole count 6 dB 12 dB 18 dB 24 dB virtual analog model | 2026-11-26 | 16 | No patent; AAS CV-2/CV-3 and OB-Xf manuals, Aalto thesis |
| Web (general) | patents.google.com "filter" "selectable" "number of poles" synthesizer digital filter morph pole mixing Oberheim claims | 2026-11-26 | 10 | US10514883 (morphing filter, not opened), US5117726, EP0484137, US4267761; none on pole count |
| Web (general) | patents.google.com multimode filter electronic musical instrument "pole" mixing outputs of cascaded state variable filter stages to select slope | 2026-11-26 | 10 | US3924199, US3974461, US4218950, US4114498; none on slope selection |
| Web (general) | patent switching filter type or order during note reset filter state unused stages click avoidance synthesizer digital filter mode change | 2026-11-26 | 0 | **Refused** by the search tool (API error, usage-policy refusal); not retried verbatim. Query 12 covers the same ground |
| Web (general) | Roland OR Yamaha OR Korg patent "filter" "second-order" "fourth-order" switching low-pass filter order musical sound signal selectable cascade connection | 2026-11-26 | 10 | US4489391, US4554858, US4548119 (not opened), Justia class lists; none relevant and in force |
| Web (general) | patent virtual analog filter model Moog ladder digital nonlinear "tap" outputs different slopes zero-delay feedback granted | 2026-11-26 | 10 | No patent; papers and Csound docs |
| Web (general) | patent digital filter variable number of cascaded stages audio synthesizer "reset" state of unused filter stages when order changed | 2026-11-26 | 10 | EP0285316, US4809208, US3522546, US6167415 (the last three not opened); none on reset in a synth |
| Web (general) | Casio patent filter device low-pass "number of poles" OR "filter order" selectable electronic musical instrument cutoff resonance digital filter 2015..2022 (extended) | 2026-11-26 | 9 | US4489391, US4467440, US8067684 (not opened); no 2015-2022 patent found |
| Web (general) | patent "pole mixing" OR "pole-mixing" filter synthesizer variable slope Rossum OR Oberheim OR Moog OR Sequential claims | 2026-11-26 | 10 | No patent; Electric Druid, KVR and NI forum pages |
| Web (general) | "variable slope" filter patent audio low-pass cascaded biquad sections slope control dB per octave continuous granted US 2010s (extended) | 2026-11-26 | 10 | US9722560 (candidate 2); US4514704, US5937072 |
| Web (general) | patent Yamaha tone generator "filter" "two-pole" "four-pole" switch digital low-pass filter virtual analog synthesizer cascade of two biquad filters resonance common | 2026-11-26 | 10 | US4554858, US4548119, US4841828 (last two not opened); Yamaha EP higher-order filter by coefficient selection (not identified) |
| Web (general) | Native Instruments OR Arturia OR "Applied Acoustics" OR Waldorf patent filter model audio synthesizer multimode filter US patent | 2026-11-26 | 10 | No vendor patent found; US3475623, US3974461 |
| Web (restricted to patents.justia.com, patents.google.com, freepatentsonline.com) | filter order selectable low-pass synthesizer voice cutoff resonance "12 dB" "24 dB" digital signal processing | 2026-11-26 | 9 | US9722560, WO2014194306 (same family), US7558171, US7425863 (analog op-amp filters, not opened) |
| Web (same restriction) | ladder filter digital model musical instrument four one-pole stages feedback resonance selectable output tap patent | 2026-11-26 | 10 | US4404529, US4011466, US4797846 (not opened); RF ladder filters irrelevant |
| Web (same restriction) | state variable filter cascaded two stages same resonance 4-pole low-pass musical instrument filter patent | 2026-11-26 | 9 | US4563652, US4934770, US7206419, US7613311, US20010033196, US4514704 |
| Web (general) | patent analog or digital synthesizer filter switchable 2-pole 4-pole low-pass mode resonance Sequential OR Korg OR Roland OR Moog Music OR Behringer granted 2012..2024 (extended) | 2026-11-26 | 9 | No patent in the window; till.com Moog patents page, product pages |
| Web (general) | patent synthesizer filter parameter disabled or greyed out when filter mode has no resonance; filter order changed only while playback stopped state reset (19; run after the design revision) | 2026-11-26 | 16 | No patent; Apple MainStage and Korg manuals (greyed-out parameters by routing, undated) |
| Google Patents (page fetch by number) | US4114498A, US3924199A, US4489391A, US3475623A, US4554858A, EP0285316A2, US7206419B1, US7613311B2, US20010033196A1, US4404529A, US4563652A, US4934770A, US4011466A, US4514704A | 2026-11-26 | 14 | Dates and status read; none in force except as noted in Candidates |
| Web page fetch | Maynooth repository page for Lazzarini and Timoney 2021; till.com Moog patents page (updated 2006-09 per page) | 2026-11-26 | 2 | Citation data; secondary list of Moog patents |
| Google Patents (claims, stage 2) | Claims section of US9722560B2 (first page window): claims 1, 7, 8, 10 in verbatim pieces, dependency map | 2026-11-26 | 1 | Independent claims quoted under Claim analysis, row 2 |
| Google Patents (claims, stage 2) | Claims section of US10311845B2 (fetch offset 100000; the first window holds no claims): claims 1-8, 10 in verbatim pieces | 2026-11-26 | 1 | Independent claims and claims 2, 5 quoted under Claim analysis, row 3 |
| Google Patents (claims, stage 2) | Claims section of US11202147B2: claims 1, 15, 21 in verbatim pieces | 2026-11-26 | 1 | Quoted under Claim analysis, row 1 |

Refused or failed: one search was refused (row 6 above). The Google Patents search UI, the USPTO
Patent Public Search and Espacenet were not tried again this session; the earlier note found them
unusable (JavaScript or HTTP 403). NI `VAFilterDesign_2.1.0.pdf` returned 404. The
karmafx.net "Digital Filters For Music Synthesis" PDF (Kasper Nielsen) came back as compressed
streams and was not read. No search used CPC classes (for example G10H, H03H17) or inventor names
in a patent database; every candidate came through a general web search or an earlier note.

## Open uncertainties

- No classified or assignee-wide search could be run. Patents held by Korg, Roland, Yamaha,
  Sequential, Moog Music, Native Instruments, Arturia, u-he, Waldorf, Applied Acoustics and Rossum
  Electro-Music (other than US11202147B2) were not enumerated. Absence of a candidate for elements
  1 to 3 means only that these web searches returned none.
- US10514883 ("Multi-channel morphing digital audio filter", interpolating between fixed frequency
  responses), US7558171 (optical disc device with a selectable first- or second-order low-pass),
  US4548119 and US4841828 (Yamaha digital filters), US8067684 (Casio filter device) and US5117726
  surfaced in search summaries and were not opened; their terms and subject were not checked.
  US10514883 and US8067684 may still be in force.
- Whether Zavalishin's book, Simper's paper or the KVR threads describe the selectable-slope,
  SVF-plus-one-pole or equal-Q SVF cascade (elements 1 to 3) is not established; the PDFs could not
  be read. The tap-per-pole mixing used in Oberheim-style and ladder-tap filters has no dated primary
  source in this note.
- Date of the product manuals mentioned under Prior art (ES2, CV-2/CV-3, OB-Xf) is not shown; they
  are not cited as prior art.
- Element 5 (revised: stopped-only, reset from rest): the row 3 verdict rests on the absence of any
  gradual fade-out/fade-in. A stage a new order adds (for example, the second SVF moving from 12 dB
  to 24 dB) could be called a backup partial filter (claim 2's wording fits it); what stays missing
  is the crossfading processing. Read row 3 again if a fade or ramp of stage contribution, however
  short, is ever added (for example, to soften the step when a voice sounds while the transport is
  stopped, such as live input), or if the order becomes changeable during playback.
- Claim quotes came through a summarizing fetch tool in pieces under 125 characters, not from the
  granted PDFs; the decisive elements (row 3 "gradually decreasing" / "gradually increasing" in
  claims 1, 8, 10; row 2 claim 1's shelving and gain-correction clauses) should be checked against
  the PDFs. Row 3 claims 9 and 11 (dependent) were not re-read.
- Row 2: whether the claim 1 and 8 preamble ("for equalizing the frequency response of loudspeaker
  systems") limits the claims was not decided; the verdict does not depend on it.
- Term end dates are twenty years from filing as printed in the earlier notes; term adjustments or
  terminal disclaimers were not seen. Rows 1 and 3 may carry adjustments.
- Row 1: the verdict (from the earlier note) holds only while the cutoff stays strictly positive and
  below 0.49·fs for every stage.
- Row 2: if a later version adds order-dependent gain correction or lets the user choose the order
  of a shelving filter, read US9722560B2 again.

## Outcome

No in-force candidate reads. Chris Paro checked statuses on 2026-11-26 and decided the design as
planned, with the order stopped-only and `resonance` greyed out at 6 dB; see
[ADR-0168](../adrs/0168-the-synth-s-low-pass-has-a-stopped-only-order-of-6.md).
