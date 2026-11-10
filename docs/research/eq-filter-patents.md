# Patent status of the TPT state-variable EQ and Butterworth filter set

Research note, 2026-11-09. This is not legal advice. It records what primary sources show, so a
reviewer can check it. Stage 1 listed the candidates; stage 2 (same day) read the independent claims
of the in-force ones against Planned design (see Claim analysis). Every date in the
Candidates table is as Google Patents prints it (Google states its status and expiry fields are
assumptions, not legal conclusions).

## Planned design

Software, portable scalar Rust, `f32`, in an audio editor's effects; per-sample processing on the
real-time audio thread.

1. **TPT two-pole state-variable filter** (zero-delay feedback, trapezoidal integrators), after
   Zavalishin ("The Art of VA Filter Design") and Simper (Cytomic "SvfLinearTrapOptimised2"). Per
   sample: `g = tan(pi*fc/fs)`, `k = 1/Q`, `a1 = 1/(1+g(g+k))`, `a2 = g*a1`, `a3 = g*a2`;
   `v3 = x - ic2eq`; `v1 = a1*ic1eq + a2*v3`; `v2 = ic2eq + a2*ic1eq + a3*v3`;
   `ic1eq = 2*v1 - ic1eq`; `ic2eq = 2*v2 - ic2eq`. Two state variables per instance.
2. **Output mixing** of the SVF's high-pass, band-pass and low-pass outputs
   (`out = m0*x + m1*v1 + m2*v2`, Simper form) to make low-pass, high-pass, bell (peaking), low
   shelf, high shelf and notch responses. Shelves and bells use `A = 10^(gain/40)`, with `g` and `k`
   scaled by `A` per Simper's paper. Identity setting (`m0=1, m1=m2=0`) bypasses.
3. **Bilinear transform with frequency prewarping** (the `tan` in `g`); cutoff clamped below about
   `0.49*fs`.
4. **TPT one-pole filter** (first-order low-pass/high-pass) for odd orders.
5. **Butterworth high-pass/low-pass, order 1 to 8**, as a cascade of two-pole SVF sections with pole
   Qs `1/(2*cos(theta_k))`, plus one one-pole section for odd orders.
6. **Parameter smoothing**: cutoff/frequency ramped linearly in log2(Hz), gain and Q ramped
   linearly; coefficients recomputed every sample during a ramp (relying on the TPT SVF staying
   stable under per-sample modulation). Mix coefficients `m0..m2` ramped linearly to new targets
   when a band's type changes or it is enabled/disabled.
7. **Changing a Butterworth filter's order**: linear crossfade over a fixed ramp time (about 10 to
   50 ms) from the old cascade's output to a second, preallocated cascade built for the new order,
   whose state starts at zero; both run during the fade.
8. **8-band parametric EQ**: eight fixed bands, each with enable, type (bell/low shelf/high
   shelf/notch), frequency, gain and Q, processed in series per channel, skipping disabled
   non-ramping bands.

## Summary

Stage 2 (claim reading) done 2026-11-09 for the ten in-force candidates; row 7 lapsed and was not
read. Per-patent verdicts and the claim text are under Claim analysis.

| Technique | Verdict | Confidence |
|---|---|---|
| 1. TPT two-pole SVF | No in-force candidate reads. Row 1 (US11202147B2) needs a resonant frequency modulated through zero into negative frequency; the design clamps `fc` to (0, 0.49 fs). | High |
| 2. Output mixing to LP/HP/bell/shelf/notch | No candidate reads. Row 4 (US9257953B2) needs shelves built from parallel, summed fourth-order low-pass and high-pass filters; the design mixes one second-order SVF's outputs. | High |
| 3. Bilinear transform with prewarp | No candidate reads. Row 3 (US7676048B2) uses it only inside a graphic-equalizer cascade with tied band frequencies and a shared bandwidth constant (see element 8). | High |
| 4. TPT one-pole | No candidate touches it. | High |
| 5. Butterworth order 1-8 cascade | No candidate reads. Row 5 (US9722560B2) concerns selectable-order *shelving* filters with order-dependent gain correction; row 4 needs parallel LP/HP shelves. | High |
| 6. Parameter smoothing | No in-force candidate addresses it. | Medium (no candidate was found by search; see Search log) |
| 7. Crossfade to a second cascade on order change | **Unclear, possible exposure.** Row 8 (US10311845B2, Casio) may read: crossfade from a partial filter in use to a preallocated "backup partial filter" set for the new characteristic. Row 6 (US9906197B2, Sony) turns on an amplifier wiring clause. Rows 9 and 10 do not read (frequency-domain only). Raise with `flag-risk` (tag `patent`) before building element 7 as designed. | Low |
| 8. 8-band parametric EQ | No candidate reads. Row 2 needs a slider that varies Q and gain together; row 3 needs tied band-edge/centre frequencies, a shared bandwidth constant or gain compensation for leakage; row 11 needs an inter-channel phase-difference parameter. | Medium (row 3 depends on a transfer function I could not read; see Open uncertainties) |

## Prior art

Items marked (unverified) were reported by a search summary or an index and I could not open the
primary text; see Open uncertainties. PDFs could not be read in this environment (no PDF renderer;
the fetch tool returned compressed streams).

- **Element 1, 3, 4 (TPT, zero-delay feedback, bilinear/prewarp).** Zavalishin, "Preserving the
  LTI system topology in s- to z-plane transforms", Native Instruments, 2008 (unverified, year from
  a search summary; the PDF URL <https://www.native-instruments.com/fileadmin/redaktion_upload/pdf/KeepTopology.pdf>
  returned 404 to me). Zavalishin, "The Art of VA Filter Design": the author's thread on KVR starts
  22 May 2012 ([KVR thread](https://www.kvraudio.com/forum/viewtopic.php?p=4930795)); the thread
  title now reads "2.1.2", so the opening post has been edited and the 2012 text is not
  established. Revision 2.1.0 is hosted by NI at
  <https://www.native-instruments.com/fileadmin/ni_media/downloads/pdf/VAFilterDesign_2.1.0.pdf>
  and was dated October 2018 per a search summary (unverified; 404 to me).
- **Element 1, 2 (trapezoidal SVF and its output mixing).** Simper, Cytomic, "SvfLinearTrapOptimised2.pdf",
  listed under "Trapezoidal integrated filters" on <https://cytomic.com/technical-papers/>; the page
  gives no date and the PDF (<https://cytomic.com/files/dsp/SvfLinearTrapOptimised2.pdf>) could not
  be read, so the paper's date and its mixing table are not established here. Simper opened a KVR
  thread on zero-delay feedback and trapezoidal integration on 13 Nov 2013
  ([KVR](https://www.kvraudio.com/forum/viewtopic.php?t=396440)); the first page shows no SVF code.
  J. O. Smith, "Digital State-Variable Filters", <https://ccrma.stanford.edu/~jos/svf/svf.pdf>,
  attributes the trapezoidal SVF to Simper and tabulates notch (1, -k, 0) and bell (1, k(A^2-1), 0)
  mixes (unverified, from a search summary; undated).
- **Element 2 (shelf/peak/notch from filter outputs, older art).** US5282252A (Orban, AKG), published
  1994-01-25, first-order shelves and a second-order band-pass peak/dip
  ([Google Patents](https://patents.google.com/patent/US5282252A/en)). US5206913A (Lectrosonics),
  published 1993-04-27, state-variable band-elimination (notch) filters, per a search summary
  ([Google Patents](https://patents.google.com/patent/US5206913A/en)).
- **Element 5 (Butterworth as second-order sections plus one first-order section for odd order).**
  Textbook content; secondary pages only: <https://pages.hmc.edu/ruye/e84/lectures/ch6/node6.html>
  (undated), <https://www.mathworks.com/help/audio/ref/designvarslopefilter.html> (undated). The
  original Butterworth source and a dated one were not found.
- **Element 6 (smoothing).** Kalinichenko, "Smooth and Safe Parameter Interpolation of Biquadratic
  Filters in Audio Applications", DAFx-2006
  ([DAFx archive](https://dafx.de/paper-archive/details/cHbRQjkBpxQWUgk-ihIVjQ)); concerns biquad
  coefficient interpolation, not SVF ramps in log2(Hz). US5170369A (E-MU, Rossum), published
  1992-12-08, logarithmic filter sweeps and coefficient interpolation, per a search summary
  ([Google Patents](https://patents.google.com/patent/US5170369A/en)). No dated source found for
  per-sample recomputation of TPT SVF coefficients or for ramping mix coefficients `m0..m2`.
- **Element 7 (crossfade between filters).** US5781461A (Stanford; Jaffe, Smith), priority 1996-05-09,
  crossfader that lets a newly updated filter warm up before the fade reaches it, per a search
  summary ([Google Patents](https://patents.google.com/patent/US5781461A/en)). US6208687B1 (Central
  Research Laboratories), priority 1996-04-16, FIR filter switched by fading sections
  ([Google Patents](https://patents.google.com/patent/US6208687B1/en)). Both are patents, not
  published papers; both concern filters of a different kind from IIR cascades of different order.
  No dated source found for a zero-state second cascade of a new Butterworth order.
- **Element 8 (multi-band parametric EQ with bell/shelf/notch).** US5282252A above; US6317117B1
  (Goff) abstract names bell, notch, shelf and pass-band filters, priority 1998-09-23
  ([Google Patents](https://patents.google.com/patent/US6317117B1/en)). Commercial 8-band EQs
  (iZotope RX, Vegas Modern Equalizer) appear in search results but their documentation is undated
  and is not cited as prior art.

## Candidates

Patents that might cover an element of the design and might still be in force. Status is filled
in by a human from Google Patents or USPTO Patent Center.

| # | Patent | Assignee | Priority / filing | Est. term end | Element it may touch | Link | Status |
|---|---|---|---|---|---|---|---|
| 1 | US11202147B2, Audio filter with through-zero linearly variable resonant frequency | Rossum Electro-Music LLC | 2018-12-26 / 2019-12-20 | 2039-12-20 (20 yr from filing; term adjustment not seen) | 1 (SVF with trapezoidal integration), 3 | [GP](https://patents.google.com/patent/US11202147B2/en) | In force |
| 2 | US9543917B2, Software for manipulating equalization curves | Fabrice Gabriel Paumier (inventor; assignee not printed) | 2014-01-24 / 2015-01-26 | 2035-01-26 (Google; no adjustment shown) | 8 (parametric EQ bands with frequency, Q, gain), 2 (SVF named among IIR types) | [GP](https://patents.google.com/patent/US9543917B2/en) | In force |
| 3 | US7676048B2, Graphic equalizers | Texas Instruments Inc. | 2004-05-14 / 2005-05-11 | 2028-11-18 (Google adjusted) | 3 (bilinear transform peak/notch), 8 (cascade of peak/notch filters) | [GP](https://patents.google.com/patent/US7676048B2/en) | In force |
| 4 | US9257953B2, Digital equalizing filters with fixed phase response | Harman Becker Automotive Systems GmbH | 2011-12-16 / 2012-12-16 | 2033-11-15 (Google adjusted) | 2, 5 (shelves from parallel low-pass/high-pass built of cascaded second-order sections) | [GP](https://patents.google.com/patent/US9257953B2/en) | In force |
| 5 | US9722560B2, Filter with independently adjustable band gain and break point slopes | Meyer Sound Laboratories Inc | 2013-05-31 / 2014-05-31 | 2035-04-12 (Google adjusted) | 5, 8 (cascaded biquads, selectable order of shelving filters) | [GP](https://patents.google.com/patent/US9722560B2/en) | In force |
| 6 | US9906197B2, Signal processing apparatus and signal processing method | Sony Corp | 2011-12-06 / 2012-11-20 | 2034-05-20 (Google adjusted) | 7 (crossfade from one filter element to a replacement), 8 (equalizer) | [GP](https://patents.google.com/patent/US9906197B2/en) | In force |
| 7 | US10516376B2, Signal processing apparatus and signal processing method | Sony Corp | 2011-12-06 / 2018-02-05 | 2032-11-20 (Google; page also prints "Expired - Fee Related") | 7, 8 | [GP](https://patents.google.com/patent/US10516376B2/en) | Lapsed (fees) |
| 8 | US10311845B2, Filter characteristics changing device | Casio Computer Co., Ltd. | 2017-03-15 / 2018-02-20 | 2038-02-20 (Google) | 7 (crossfade between partial filters on a characteristic change, backup block prepared in advance) | [GP](https://patents.google.com/patent/US10311845B2/en) | In force |
| 9 | US10187741B2, Device and method for processing a signal in the frequency domain | Fraunhofer-Gesellschaft (current per Google: Brandenburg Labs GmbH) | 2014-03-14 / 2016-09-14 | 2035-03-11 (Google) | 7 (a search summary says the description discusses crossfading two filters; claims not seen) | [GP](https://patents.google.com/patent/US10187741B2/en) | In force |
| 10 | US10257640B2, Device and method for processing a signal in the frequency domain | Fraunhofer-Gesellschaft (current per Google: Brandenburg Labs GmbH) | 2014-03-14 / 2018-02-14 | 2035-03-11 (Google) | 7 (as row 9) | [GP](https://patents.google.com/patent/US10257640B2/en) | In force |
| 11 | US11038482B2, Parametric equalization for audio applications | Dirac Research AB | 2017-04-07 / 2017-04-07 | 2037-04-25 (Google adjusted) | 8 (title only; claim 1 concerns filter pairs across channels with inter-channel phase parameters) | [GP](https://patents.google.com/patent/US11038482B2/en) | In force |

Statuses checked by Chris Paro on 2026-11-09: every candidate is in force except US10516376B2,
which has lapsed for unpaid fees.

Status values: **In force**, **Lapsed (fees)**, **Expired (term)**, **Abandoned**, **Invalidated**,
**Not found**. Record who checked and when, under the table.

Expired by term (filing date + 20 years, plus any printed term adjustment, is in the past), listed
for completeness and needing no check:

- [US5781461A](https://patents.google.com/patent/US5781461A/en), Stanford, filed 1996-09-04, priority
  1996-05-09, expiry 2016-05-09 per Google. Crossfader between filters (element 7).
- [US6208687B1](https://patents.google.com/patent/US6208687B1/en), Central Research Laboratories
  (now Creative), filed 1997-04-04, expiry 2017-04-04 per Google. Fade between FIR filter sections
  (element 7).
- [US5170369A](https://patents.google.com/patent/US5170369A/en), E-MU Systems, filed 1990-08-29,
  expiry 2009-12-08 per Google. Logarithmic sweeps and coefficient interpolation (element 6).
- [US5282252A](https://patents.google.com/patent/US5282252A/en), AKG Acoustics, filed 1992-02-27,
  expiry 2012-02-27 per Google. Shelf and peak equalizer (elements 2, 8).
- [US5206913A](https://patents.google.com/patent/US5206913A/en), Lectrosonics, filed 1991-02-15,
  expiry 2011-02-15 per Google. State-variable notch filters (element 2).
- [US6317117B1](https://patents.google.com/patent/US6317117B1/en), individual (Goff), filed
  1999-02-16, expiry 2019-02-16 per Google. Control interface for bell, notch, shelf, pass-band
  filters (element 8).
- [WO2004054099A1](https://patents.google.com/patent/WO2004054099A1/en) and EP1570573B1, TC
  Electronic, filed 2002-12-09, so 20 years ended 2022-12-09 (Google's "ceased" date of 2005 is
  not relied on). Fully parametric equalizer with a symmetry parameter morphing shelf and bell
  (element 8). Its US application [US20060114979A1](https://patents.google.com/patent/US20060114979A1/en)
  is listed abandoned in 2010 with no US patent shown.
- [US20040146170A1](https://patents.google.com/patent/US20040146170A1/en), graphic equalizer with
  parametric function, filed 2003-04-02: abandoned application in 2008, no granted counterpart shown.

## Claim analysis

Claim text source: each patent's Google Patents page (`https://patents.google.com/patent/<number>/en`),
fetched 2026-11-09. Google serves this as machine-readable text, which I did not OCR. The fetch tool
relays page text through a summarizer and caps each quote at 125 characters, so long claims were
pulled in consecutive verbatim pieces; joins between pieces are marked " ... ". Equations that the
page's text extraction drops are marked [equation not in text]. A reviewer should check the decisive
elements against the granted PDF. Element numbers refer to Planned design. Row 7 (US10516376B2)
lapsed and was not read.

### Row 1: US11202147B2 (Rossum), priority 2018-12-26: **Does not read**

Independent claims 1, 15, 21. Claims 2-14 depend on 1 and 16-20 on 15.

- Claim 1: "receiving an input signal;": met (element 1 processes `x`). "receiving one or more control
  input signals; and": met (frequency and Q parameters, element 6). "providing a resonant filter for
  electronic music for processing the input signal,": met arguably (element 1 with Q). "the resonant
  filter being responsive to the one or more control input signals" ... "to produce a resonant
  frequency that varies through zero and is at least linearly controlled,": **not met**. Element 3
  clamps `fc` below about `0.49*fs` and `g = tan(pi*fc/fs)` is positive; the design has no zero or
  negative resonant frequency. "such that the resonant filter provides an output signal that has
  timbral modulation of the input signal.": not met (follows from the missing element).
- Claim 15: "a state variable filter configured to provide at least one of lowpass, bandpass, and
  highpass outputs,": met (element 1, `v1`, `v2`). "the state variable filter capable of having its
  resonant frequency linearly modulated" ... "through zero into negative frequency; and": **not met**
  (same clamp). "circuitry for configuring ... to produce a resonant frequency that varies through
  zero": not met.
- Claim 21: "linearly modulating a resonant frequency of the at least one resonant filter through
  zero frequency" ... "to enable negative frequency modulation": **not met**. "performing exponential
  times linear multiplication to determine the resonant frequency,": not met; element 6 ramps in
  log2(Hz) but has no linear (through-zero) term multiplied in.

Missing element: a resonant frequency that passes through zero into negative frequency. Keep the
positive clamp in element 3. Prior art in this note before 2018-12-26: Zavalishin 2008 (unverified)
and the 2012 KVR thread, Simper's 2013 KVR thread, US5206913A (1993); these show the SVF, not
through-zero modulation.

### Row 2: US9543917B2 (Paumier), priority 2014-01-24: **Does not read**

Independent claims 1 and 2 (the only claims).

- Claim 1: "parametric equalization hardware that is coupled to computer readable memory with
  software in the form of programming code": met arguably (element 8 runs as software on a computer).
  "configured to present a command interface to a user and control the equalization hardware" ...
  "to manipulate the frequency, the Q, and the gain of an audio signal;": met (element 8).
  "wherein the computer hardware and software are configured to visually present the frequency
  response curve" ... "of the audio equipment to the user via the command interface;": Planned design
  states no curve display. "wherein the computer software may be operationally configured to alter
  the shape of the resonance response curve" ... "and correspondingly equalize the audio signal;":
  met (element 8). "configured to display a character control slider" ... "on the command interface
  that, when manipulated, simultaneously varies the Q and gain of an equalization curve" ...
  "between two preset values defined by: (1) a high gain and narrow Q; and (2) low win and wide Q."
  (sic, "low win" as printed): **not met**; element 8 sets gain and Q as separate parameters.
- Claim 2: adds "wherein, the frequency response is presented as a normal bell curve," and the
  frequency/Q/gain curve controls; ends with the same "character control slider that, when
  manipulated, simultaneously varies the Q and gain of an equalization curve between two preset
  values defined by:" ... "(1) a high gain and narrow Q; and (2) low gain and wide Q.": **not met**.

Missing element: the character control slider that moves Q and gain together between two presets.
Prior art before 2014-01-24: US6317117B1 (priority 1998), US5282252A (1994).

### Row 3: US7676048B2 (Texas Instruments), priority 2004-05-14: **Does not read**

Independent claims 1 and 6. Claims 2-5 depend on 1, 7-8 on 6.

- Claim 1: "(a) a cascade of peak/notch filters F(1), F(2), . . . , F(N) where N is an integer greater
  than 1": met when two or more element-8 bands are bells or notches in series. "and each of said
  filters has a transfer function which is a bilinear transform of an analog transfer function of
  the form" [equation not in text] "with K a gain, α a positive constant, and ω0 an analog center
  frequency;": not established; element 2's bell is a prewarped bilinear transform of an analog
  peaking filter, but I could not read the claimed analog form. "(b) wherein said center frequency
  corresponding to F(j) is equal to a band-edge frequency" ... "corresponding to F(j+1) and/or
  F(j−1) for j=1, 2, . . . N, and wherein α is the same for all of said filters.": **not met**;
  element 8 bands take independent frequency and Q, with no tie between one band's centre and a
  neighbour's band edge and no shared bandwidth constant.
- Claim 6: "(a) receiving a plurality of desired center frequency filter gains for a cascade of
  equalization filters": met arguably (per-band gain). Transfer-function and band-edge clauses as in
  claim 1: not met. "(b) converting said desired center frequency filter gains into compensated
  center frequency filter gains for said cascade" ... "wherein said converting compensates for leak
  gains.": **not met**; element 8 applies each band's gain as set.

Missing elements: claim 1 (b), the tied band-edge/centre frequencies with one α for all filters;
claim 6 (b), the leak-gain compensation. Prior art before 2004-05-14: US5282252A (1994),
US6317117B1 (1998), WO2004054099A1 (filed 2002-12-09), US20040146170A1 (filed 2003-04-02).

### Row 4: US9257953B2 (Harman Becker), priority 2011-12-16: **Does not read**

Independent claims 1 and 7 (method, same elements). Claims 2-6 depend on 1, 8 on 7.

- "a first and a second shelving filter having a fixed first and a fixed second phase response," ...
  "each of which is determined by a respective cut-off frequency and Q factor": partly met; element 8
  can place a low shelf and a high shelf, each set by frequency and Q, but their phase changes with
  gain (element 2 scales `g` and `k` by `A`).
- "wherein the first and the second shelving filters are coupled in series": met (element 8, series
  bands).
- "and each shelving filter comprises (i) at least one fourth order low-pass filter" ... "having a
  cut-off frequency, the Q factor and a first broadband gain and (ii) at least one fourth order
  high-pass filter" ... "having the same cut- off frequency and the same Q factor as the low-pass and
  further having a second broadband gain;": **not met**; each element-2 shelf is one second-order SVF.
- "wherein the fourth order low-pass filter and the fourth order high-pass filter are connected in
  parallel," ... "such that both filters receive the same input signal and the corresponding filtered
  signals are summed": not met; element 2 sums outputs of one second-order section, and element 5's
  cascades are low-pass or high-pass only, never summed in parallel.
- "wherein each fourth order low-pass and high-pass filter is composed of a cascade" ... "of two second
  order low-pass or high-pass filters,": not met for shelves.

Missing element: shelves made of parallel, summed fourth-order low-pass and high-pass filters.
Prior art before 2011-12-16: US5282252A (1994); Zavalishin 2008 (unverified).

### Row 5: US9722560B2 (Meyer Sound), priority 2013-05-31: **Does not read**

Independent claims 1, 8, 10. Claims 2-7 depend on 1, 9 on 8, 11-13 on 10. The page text for claim 8
starts at "multiple band filter section"; its preamble was not in the text.

- Claim 1: "at least one band filter section comprised of an n-order high boost or cut shelving
  filter" ... "having a break point frequency, ω1, and an n-order low boost or cut shelving filter" ...
  "having a break point frequency, ω2, wherein ω1<ω2,": partly met; element 8 can set a high shelf and
  a low shelf, but each is second order only. "and wherein the order, n, of at least one of said
  shelving filters of the band filter section is user selectable": **not met**; element 5's selectable
  order applies to Butterworth low-pass and high-pass, not to shelves. "gain correction means for the
  n-order high shelving filter and n-order low shelving filter" ... "said correction means providing a
  correction factor dependent on the order of the shelving filters.": **not met**.
- Claim 8: same shelving, user-selectable order and gain-correction elements, plus "gain control means
  for adjusting the corrected-to base gain of each band filter section,": not met as claim 1.
- Claim 10: "constructing of n-order high boost or cut shelving filter", "the order, n, of at least
  one of said shelving filters is user selectable", "establishing a s-plane representation of a
  frequency-normalized n-order Butterworth-type filter," and "adding n complex zeros to the complex
  poles": **not met**; element 5 uses Butterworth poles for pass filters with no added zeros and no
  shelving.

Missing element: shelving filters with user-selectable order and order-dependent gain correction.
Prior art before 2013-05-31: US5282252A (1994); Butterworth textbook pages (undated).

### Row 6: US9906197B2 (Sony), priority 2011-12-06: **Unclear**

Independent claims 1 and 10. Claims 2-9 depend on them.

- Claim 1: "circuitry that comprises a first amplifier, a second amplifier, a first adder," ... "a
  plurality of signal processing filters and at least one crossfade signal processing filter,":
  met arguably in software (element 7: gains on the two cascade outputs, a sum, the old cascade's
  sections, the preallocated second cascade). "enhance or attenuate an input signal in a specific
  frequency band;": met (low-pass/high-pass). "replace at least one signal processing filter among
  the plurality of signal processing filters;": met (old cascade replaced by new). "to crossfade to
  the at least one crossfade signal processing filter having a new characteristic; and": met
  (element 7, new order). "replace the at least one signal processing filter by the at least one
  crossfade signal processing filter," ... "to process remaining signal processing filters of the
  plurality of signal processing filters,": unclear; element 7 replaces the whole cascade. **"wherein a
  first input terminal of the at least one crossfade signal processing filter is directly connected"
  ... "to a first input terminal of the first amplifier,"**: on its plain words not met; in element 7
  both gains sit on cascade *outputs*, and no gain takes the new cascade's input signal. "and an output
  terminal of the at least one crossfade signal processing filter is connected to the first adder" ...
  "via the second amplifier.": met (new cascade output times the fade-in gain, summed).
- Claim 10: same elements with "a plurality of IIR filters and at least one crossfade IIR filter:"
  (element 5 sections are IIR: met), "crossfading at least one IIR filter among the plurality of IIR
  filters to the at least one crossfade IIR filter having the new characteristic;": met, and the same
  first-amplifier wiring clause: plain words not met.

Deciding element: "a first input terminal of the at least one crossfade signal processing filter is
directly connected to a first input terminal of the first amplifier". Literally the design lacks it;
whether an output-side fade gain is equivalent depends on what the drawings show the first amplifier
doing, which I could not see. Prior art before 2011-12-06: US5781461A (priority 1996, crossfader
letting an updated filter warm up), US6208687B1 (1996), Kalinichenko DAFx-2006.

### Row 8: US10311845B2 (Casio), priority 2017-03-15: **Unclear (leans Reads for orders 3-8)**

Independent claims 1, 8 (method), 10 (storage medium), same elements. Claims 2-7 depend on 1, 9 on
8, 11 on 10.

- "a set of filters which includes a plurality of partial filters" ... "and which forms a
  characteristic by combining at least two filters" ... "selected from among the plurality of partial
  filters; and": met arguably; element 7 holds two preallocated cascades, each of second-order and
  one-pole sections, and the response is formed by cascading sections. For orders 1 and 2 the active
  cascade is one section, so "at least two" is not met there.
- "a processor which performs, as crossfading processing for a first filter and a second filter" ...
  "fade-out processing of gradually decreasing a degree of contribution of the first filter to the
  characteristic" ... "and fade-in processing of gradually increasing a degree of contribution of
  the second filter to the characteristic,": met for whole cascades (element 7 linear crossfade);
  whether fading a whole series cascade is fading "the first filter", a single partial filter, is the
  open question.
- "when an instruction is provided for changing the characteristic formed by the set of filters,":
  met (order change).
- "wherein the processor selects a partial filter used for characteristic formation before the
  instruction" ... "as the first filter,": met (the old cascade's sections).
- "sets a parameter to at least one backup partial filter among the plurality of partial filters,"
  ... "the parameter corresponding to the characteristic indicated by the instruction,": met (the
  second, preallocated cascade is built for the new order).
- "and selects the at least one backup partial filter as the second filter for the crossfading
  processing.": met.

Deciding element: "a degree of contribution of the first filter" with "the first filter" being one
partial filter, and "combining" of partial filters. If a series cascade of sections counts as
partial filters combined, and fading the cascade fades each section's contribution, every element is
met for orders 3 to 8. Prior art before 2017-03-15: US5781461A (1996), US6208687B1 (1996),
Kalinichenko DAFx-2006; Sony's US9906197B2 (filed 2012-11-20; its application publication was not
opened).

### Rows 9 and 10: US10187741B2 and US10257640B2 (Fraunhofer / Brandenburg Labs), priority 2014-03-14: **Does not read**

US10187741B2: claims 1, 22-27 and 28-44 do not refer to another claim (as relayed; claim 27's
opening fell between fetch windows). US10257640B2: independent claims 1, 9, 10; claim 8 is phrased
without a claim number but its terms rest on claim 1.

- Every independent claim of both opens its processing with "filter the signal which is present in a
  discrete frequency-domain representation by a filter with a filter characteristic" (claims 22 and
  25 of US10187741B2: "a frequency-domain representation"), and then "provide the filtered signal
  with a frequency-domain window function in order to acquire a windowed signal,". **Not met**:
  elements 1 to 7 are per-sample time-domain IIR filters; there is no spectral representation and no
  frequency-domain window.
- US10257640B2 claim 1 also recites "the frequency-domain window function approximates a fade-out
  function in the time domain" (one alternative): the design's crossfade (element 7) is a
  time-domain gain, not a frequency-domain window.

Missing element: filtering and windowing a signal in a discrete frequency-domain representation.
Prior art before 2014-03-14: US5781461A, US6208687B1.

### Row 11: US11038482B2 (Dirac), priority 2017-04-07: **Does not read**

Independent claims 1, 8, 16, 19, with the same core.

- "an audio filter system having a number, N≥2, of filter paths configured to enable processing of N
  audio channels," ... "one filter path per channel,": met (element 8 runs per channel).
- "a common set of filter design parameters for a pair of audio filters belonging to different filter
  paths,": met arguably if one band's settings drive both channels.
- "including phase difference information representing an inter-channel phase difference": **not
  met**; element 8's parameters are enable, type, frequency, gain and Q only.

Missing element: an inter-channel phase-difference parameter. Prior art before 2017-04-07:
US5282252A, US6317117B1, WO2004054099A1.

## Search log

All searches 2026-11-09. "Hits" counts results the tool returned (about 10 per query unless noted).
The search tool returns a prose summary, not raw result lists, so hit counts are approximate.

| Database | Query | Date | Hits | Result |
|---|---|---|---|---|
| Web (general) | patent zero-delay feedback trapezoidal integration state variable filter digital audio | 2026-11-09 | 9 | US11202147 (row 1); rest papers and forums |
| Web (general) | patent parametric equalizer state variable filter output mixing shelf bell notch coefficients smoothing per-sample | 2026-11-09 | 9 | US9543917, US7676048, WO2004054099, US5206913, US6317117, US20040146170 |
| Web (general) | patent crossfade between filter cascades when changing filter order digital filter parameter change click-free | 2026-11-09 | 9 | US10257640, EP0979463, others not relevant; no patent on order change |
| Web (general) | Zavalishin patent topology-preserving transform filter virtual analog Native Instruments | 2026-11-09 | 10 | No patent; NI papers |
| Web (general) | patents.google.com digital state variable filter audio bilinear transform prewarp cutoff modulation per-sample coefficient update stable | 2026-11-09 | 10 | No patent |
| Web (general) | patents.google.com smoothing filter coefficients logarithmic frequency interpolation parametric equalizer avoid zipper noise audio | 2026-11-09 | 10 | US5170369, US11038482, Magic Leap "Index scheming" (not pursued; shelving-cascade proportional EQ, status unchecked) |
| Web (general) | patents.google.com Butterworth high-pass low-pass variable order cascade second-order sections first-order section audio equalizer slope selectable | 2026-11-09 | 10 | No patent; docs only |
| Web (general) | patents.google.com crossfade between old and new filter output when filter structure changes parameter switching audio filter transition | 2026-11-09 | 10 | US5781461, US10187741, US10257640, US9906197 |
| Web (general) | patent ramp biquad filter coefficients over time interpolate gain Q frequency smoothly equalizer band enable disable bypass crossfade | 2026-11-09 | 10 | No patent; DAFx 2006 paper |
| Web (general) | patent digital equalizer shelving peaking filters implemented with state variable filter structure mixing lowpass bandpass highpass outputs gain Q | 2026-11-09 | 10 | US9257953, US5282252 |
| Web (general) | patent "one-pole" trapezoidal integrator topology preserving virtual analog filter Reaktor OR Cytomic OR "Native Instruments" claims | 2026-11-09 | 10 | No patent |
| Web (general) | Andrew Simper Cytomic inventor patent filter | 2026-11-09 | 15 | Irrelevant (cytometry); no Simper/Cytomic patent found |
| Web (restricted to patents.google.com, freepatentsonline.com, patents.justia.com) | patent "state variable filter" equalizer bell shelf notch digital audio workstation plugin "trapezoidal" | 2026-11-09 | 10 | US20060114979, US9543917, EP1570573, US7676048, US6317117, US5206913, US4453132 (1982-era active filter, dates not checked), US3755749, US9877107, US5046105 (older, dates not checked) |
| Web (general) | patent audio filter changing order or slope without clicks run two filters in parallel fade new filter state initialized zero | 2026-11-09 | 9 | US6208687, US9075697 (not relevant), US9722560 |
| Web (general) | patent eight band parametric equalizer digital audio editor bands series low shelf high shelf bell notch enable per band | 2026-11-09 | 10 | No patent; product docs |
| Google Patents (page fetch by number) | US11202147B2, US9543917B2, US9906197B2, US10187741B2, US10257640B2, US5781461A, US11038482B2, US7676048B2, US9257953B2, US5282252A, WO2004054099A1, US5170369A, US20060114979A1, US20040146170A1, US6122605A, US6317117B1, US9877107B2, US5206913A, US6208687B1, US9722560B2, US9379687B1, US10516376B2, US10311845B2, US20180268793A1, US7873686B2 | 2026-11-09 | 25 | Dates and status read. Not candidates: US6122605 (process-control filter, expired), US9877107 (speaker-protection EQ, fee-lapsed, not on any element), US9379687 (systolic FIR), US7873686 (half-band FIR), US20180268793A1 (application of row 8) |
| Google Patents (citations) | Cited-by list of US6208687B1 (13 patents) | 2026-11-09 | 13 | Checked US9379687, US10516376, US10311845, US20180268793, US7873686; US6600495B1, US6973144B1, EP1416345A1, US20050184785A1, WO2005072446A3, US20070050439A1, US20130142359A1, US10516376's family not opened |
| Google Patents (claims, stage 2) | Claims sections of US11202147B2, US9543917B2, US7676048B2, US9257953B2, US9722560B2 (first page window) | 2026-11-09 | 5 | Independent claims quoted under Claim analysis |
| Google Patents (claims, stage 2) | Claims of US9906197B2, US10311845B2, US10257640B2, US11038482B2 (fetch offset 100000); US10187741B2 (offsets 150000 and 200000) | 2026-11-09 | 5 | Independent claims quoted under Claim analysis; claim 27 of US10187741B2 straddles the two windows |

Refused or failed: Google Patents search UI (<https://patents.google.com/?q=...>) returned no results
(page needs JavaScript). Espacenet REST search returned HTTP 403. USPTO Patent Public Search
(<https://ppubs.uspto.gov/pubwebapp/external.html>) returned only its title (JavaScript app). Fetches
returning 404: NI `VAFilterDesign_2.1.0.pdf` (first try) and `VAFilterDesign_2.1.2.pdf`, NI
`KeepTopology.pdf`. Web Archive fetch refused by the tool. PDF reading failed (no `pdftoppm`);
Cytomic and Smith PDFs were not readable. Because the Google/USPTO/Espacenet query interfaces did
not work, no search was run with CPC classes (for example G10H, H03H17, H04S, G10L) or inventor
names directly in a patent database; every candidate came through a general web search or a
citation list.

## Open uncertainties

- Date and content of Simper's "SvfLinearTrapOptimised2" (element 1, 2, mix table) not established
  from the paper itself. Likewise Smith's paper date and the 2008 Zavalishin paper and the
  October 2018 date of book revision 2.1.0. A reviewer should open the PDFs.
- Whether the book's first (2012) release already held the TPT one-pole, SVF, Butterworth
  cascade and modulation-stability material (elements 1, 4, 5) is not established; the KVR opening
  post was edited and its original text is not visible.
- Element 6 (log2(Hz) ramps, per-sample coefficient recomputation, ramping of mix coefficients) and
  element 7 (zero-state preallocated cascade of a new order) have no prior art in this note beyond
  the loosely related items above.
- Element 3 (bilinear transform with prewarp) and element 5 (Butterworth poles) are textbook; no
  dated primary source is cited.
- **Element 7 needs a human decision (tag `patent`).** Row 8 (US10311845B2, Casio, to 2038-02-20)
  may read on the order-change crossfade to a preallocated second cascade; row 6 (US9906197B2, Sony,
  to 2034-05-20) turns on its first-amplifier wiring clause. Decide, through `flag-risk`, whether to
  get counsel's reading, rely on the older art (US5781461A, US6208687B1, both expired) against
  validity, or change element 7 so it holds no backup filter set to the new characteristic (for
  example, change order only while muted, or move existing sections' parameters instead of fading to
  a second cascade); any redesign should be read against both claims again.
- Claim quotes came through a summarizing fetch tool in pieces under 125 characters, not from the
  granted PDFs; the decisive elements (row 6 first-amplifier clause, row 8 claim 1, row 3 claim 1
  (b)) should be checked against the PDFs and, for row 6, the drawings.
- Row 3: the claimed analog transfer function was dropped by the page's text extraction. The
  verdict rests on element (b), which does not depend on it. If a later UI ties band frequencies
  together (for example, a graphic-EQ mode with fixed octave bands and one Q), read row 3 again.
- Row 2: if the UI adds a single control that moves Q and gain together, read row 2 again.
- Row 1: the verdict holds only while element 3 keeps `fc` strictly positive.
- Possible prior art for row 8 not opened: Sony's pre-grant publication of US9906197B2 (filed
  2012-11-20). US20130142359A1, in US6208687B1's cited-by list, may be it; not checked.
- Term end dates are twenty years from filing as printed by Google; term adjustments or terminal
  disclaimers were not seen on any page. Rows 1 and 8 may carry adjustments that move the end.
- Cited-by lists of the candidates (other than US6208687B1) were not walked, and no database
  search with classification codes could be run (see Refused or failed).
- Not checked: patents held by Native Instruments, Cytomic, FabFilter, iZotope, Waves, or other EQ
  vendors by assignee name.

## Outcome

Element 7 was not built. Chris Paro decided (2026-11-09, through `flag-risk`) that a filter's order
changes only while playback is stopped, applied at once with no crossfade; see
[ADR-0088](../adrs/0088-equalisers-are-built-on-a-trapezoidal-state.md).
