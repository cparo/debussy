# Patent status of the in-house dynamics processor (compressor, expander, limiter, gate)

Research note, 2026-11-12. This is not legal advice. It records what primary sources show, so a
reviewer can check it. Stage 1 listed candidates; a human filled **Status**; stage 2 (2026-11-12)
read the claims of the eight in-force candidates against the design (see Claim analysis). Every
date in the Candidates table is as Google Patents prints it (Google states its status and expiry
fields are assumptions, not legal conclusions). For issue #59.

## Planned design

Software, portable scalar Rust, in an audio editor's effects; per-sample processing on the
real-time audio thread, stereo channels L and R. One effect covers compressor, expander, limiter
and gate.

1. **Feed-forward topology.** The gain is computed from the input signal (not the output), then
   applied to a delayed copy of the input.
2. **Level detection, two modes.**
   a. Peak: `max(|L|, |R|)`. Stereo linking by the louder channel; one gain for both channels.
   b. RMS: `max(L^2, R^2)` smoothed as a power by the ballistics of element 3, then square-rooted.
3. **Level ballistics (on the detected level, not on the gain).** A one-pole smoother
   `y += a*(x - y)` using an attack coefficient while the input level is above the smoothed level
   and a release coefficient while it is below; `a = 1 - exp(-1/(tau*fs))` from attack and release
   times. A hold timer: release does not begin until `hold` ms after the last sample that raised
   the level. The gain derived from the smoothed level is not smoothed again.
4. **Static transfer curve in dB** on the smoothed level `x` (dB): threshold `T`, knee width `W`.
   - Below `T - W/2`: `y = T + (x - T)*ratio_below` (`ratio_below >= 1`, downward expansion; ratio
     100 acts as a gate); the attenuation `y - x` is clamped at `-range` dB.
   - Above `T + W/2`: `y = T + (x - T)/ratio_above` (compression).
   - Inside the knee: a quadratic (second-order polynomial) blend between the two lines (the
     standard soft knee of Giannoulis, Massberg & Reiss 2012, applied on both sides).
   - Optional ceiling `C`: `y <= C`, with its own quadratic soft knee of width `Wc`.
   - `gain = 10^((y - x + makeup)/20)`.
5. **Ceiling guarantee (lookahead brickwall limiter).** A separate detector path takes
   `max(|L|, |R|)`, runs a sliding-window maximum over the lookahead window (a monotonic deque,
   van Herk/Gil-Werman style or Lemire's streaming max), then a moving average (box filter, running
   sum) of the same window length, so the smoothed level is never below the true peak of the
   delayed signal. The ceiling gain from this path is combined with the main curve's gain by taking
   the minimum.
6. **Lookahead.** The audio path is delayed by `round(lookahead*fs)` samples (0 to 10 ms) so the
   gain leads the signal; the node reports that delay as its latency for plugin-delay
   compensation.
7. **Makeup gain** applied as a constant dB offset.

## Summary

**Decision (Chris Paro, 2026-11-12, `flag-risk` tag `patent`):** element 5 is left out; the ceiling
is a soft limit on the smoothed level. Recorded in ADR-0099; the brickwall path is issue #135.

Stage 2 done for the eight in-force candidates (rows 1, 2, 3, 4, 7, 8, 9, 10). Per patent: row 1
**Unclear**, row 4 **Unclear**, rows 2, 3, 7, 8, 9, 10 **Does not read**. Both Unclear rows bear on
element 5 with the lookahead of element 6; the elements whose meaning decides them are in Claim
analysis and Open uncertainties. Rows 5, 6 (fee-lapsed) and 11 (revoked) were not analysed.

| Technique | Verdict | Confidence |
|---|---|---|
| 1. Feed-forward topology | No in-force claim read covers it alone; it is one step inside rows 1 and 4, whose verdicts turn on other elements. | Medium |
| 2. Peak / RMS level detection, stereo link by max | No in-force claim reads on it alone. Row 4 dependent claim 3 (louder of two loudspeaker channels) matters only if row 4 claim 1 reads. RMS mode with lookahead is a second Unclear point for row 1 claim 55. | Medium |
| 3. Ballistics (one-pole attack/release on level, hold) | Does not read: row 7 (its timer runs from a threshold crossing and delays the gain change; the hold runs from the last level rise and delays release), row 10 (no transient-adaptive times), row 2 (no adaptive time constants). | Medium |
| 4. Static curve, knee, expansion, range, ceiling | No in-force candidate; none read. | Medium |
| 5. Sliding max plus box filter limiter, min with main gain | **Unclear** on row 1 (whether a sample-peak level is a "loudness level") and row 4 (whether a gain from the average of N window maxima is "a first gain value based on the threshold value and the first maximum amplitude value"). Does not read: rows 3, 9. | Low |
| 6. Lookahead delay, reported latency | Fixed lookahead: part of rows 1 and 4 (Unclear, as element 5). Does not read: rows 2, 3, 8, 9. No patent found on reporting latency to a host. | Medium |
| 7. Makeup gain | No candidate found. | High |

## Prior art

Items marked (unverified) were reported by a search summary or an index and I could not open the
primary text; see Open uncertainties. The AES e-library returned HTTP 403 to every fetch.

- **Element 4 (soft-knee gain computer), element 1, 3 (compressor design).** Giannoulis, Massberg and
  Reiss, "Digital Dynamic Range Compressor Design - A Tutorial and Analysis", JAES vol. 60 no. 6,
  pp. 399-408, June 2012 ([AES e-library](https://www.aes.org/e-lib/browse.cfm?elib=16354);
  bibliographic details from a search summary, page not opened, so (unverified)). Its content
  (feed-forward versus feedback, quadratic knee, ballistics before or after the gain computer) is
  not established here from the paper itself.
- **Elements 1 to 4 (digital dynamics control, older art).** McNally, "Dynamic range control of
  digital audio signals", JAES vol. 32 no. 5, pp. 316-327, May 1984
  ([AES e-library](https://www.aes.org/e-lib/browse.cfm?elib=4505), not opened; the citation was read
  in a patent reference list, so (unverified)). What the paper describes beyond "dynamic and static
  controller characteristics" and smoothing of the gain-control signal is not established.
- **Elements 2 to 4, 7 (compressor, expander, limiter, gate textbook).** Zolzer (ed.), "DAFX - Digital
  Audio Effects", Wiley, 2002, Chapter 5 "Nonlinear Processing" (P. Dutilleux, U. Zolzer), with
  Limiter, Compressor and Expander, and Noise Gate sections
  ([DAFX book page](https://dafx.de/DAFX_Book_Page/chapter5.html)); the page is a table of contents
  and shows no mention of RMS, peak detection, lookahead or knee. Second edition, 2011, ISBN
  978-0-470-66599-2, Chapter 4, sections 4.2.1 Limiter, 4.2.2 Compressor and expander, 4.2.3 Noise
  gate ([contents on Safari](https://www.safaribooksonline.com/library/view/dafx-digital-audio/9780470665992/c04_level1_2.xhtml),
  page numbers from a search summary). The chapter texts were not read.
- **Element 3 (hold timer).** US5334947A (Aphex Systems), priority 1993-07-06, a noise gate whose
  hold timer starts when the input exceeds the threshold, so the gate closes only after the timeout
  ([Google Patents](https://patents.google.com/patent/US5334947)). A patent, not a paper; it
  places hold-timed gates before 1993. Product manuals found in search (Steinberg HALion Gate,
  QSC Q-SYS gate) are undated and not relied on.
- **Element 5 (sliding-window maximum algorithms).** M. van Herk, "A fast algorithm for local minimum
  and maximum filters on rectangular and octagonal kernels", Pattern Recognition Letters 13(7),
  517-521, 1 July 1992, DOI 10.1016/0167-8655(92)90069-c ([dblp](https://dblp.org/rec/journals/prl/Herk92);
  details from a search summary, paper not opened, (unverified)). J. Gil and M. Werman, "Computing 2-D
  min, median, and max filters", IEEE TPAMI 15(5), 504-507, May 1993 ([Hebrew University record](https://cris.huji.ac.il/en/publications/computing-2-d-min-median-and-max-filters/)).
  D. Lemire, "Streaming Maximum-Minimum Filter Using No More than Three Comparisons per Element",
  Nordic Journal of Computing 13(4), 328-339, 2006, arXiv v1 submitted 9 October 2006
  ([arXiv abs](https://arxiv.org/abs/cs/0610046)); the abstract does not mention a deque, so the
  deque form is not established from this source. The monotonic-queue form of sliding maximum
  appears in many undated tutorials; none is cited.
- **Element 5 (limiter built from sliding maximum plus averaging of the same length, lookahead delay).**
  - musicdsp.org "Lookahead Limiter", posted 2009-11-16: a circular buffer of the lookahead length,
    a hold algorithm for the window maximum, gain reduction derived from it, two cascaded
    circular-buffer averages ("dirac impulses to a triangle"), a release slew, and the gain applied
    to the delayed sample ([musicdsp.org](https://www.musicdsp.org/en/latest/Effects/274-lookahead-limiter.html)).
    This is the earliest dated source found for the structure. It takes the maximum and averages the
    gain reduction; the design averages the level. Whether the two are equivalent is for a reader.
  - G. Luff, "Designing a straightforward limiter", Signalsmith Audio, 2022-04-08: a moving minimum
    of gain (peak-hold turned upside-down), then "a (weighted) moving average no longer than our
    moving minimum", and the input delayed "by the same number of samples"; it says a box filter
    works and recommends cascaded box filters; it points to the Signalsmith DSP Library
    ([blog post](https://signalsmith-audio.co.uk/writing/2022/limiter/)). It says the peak-hold and
    smoothing algorithms were discussed in earlier posts of the author, which I did not open, so the
    first Signalsmith date is not established.
  - US6535846B1 (K S Waves), priority 1997-03-19: compressor-limiter with look-ahead delay of the
    audio path; claim 4 a linear-phase FIR smoother whose order equals the look-ahead delay
    ([Google Patents](https://patents.google.com/patent/US6535846)).
  - US5737434A (Orban), priority 1996-08-26: peak limiter with a 4 ms retriggerable sample-and-hold
    on the control signal, low-pass filtered, with the audio delayed by 4 ms
    ([Google Patents](https://patents.google.com/patent/US5737434)).
- **Elements 2a, 6 (stereo link by worst channel, lookahead).** US6757396B1 (Texas Instruments),
  priority 1998-11-16: compressor with RMS-style estimator and a minimum selector across channels,
  gain smoothed by attack and release filters; no delay line
  ([Google Patents](https://patents.google.com/patent/US6757396)).
  Hamalainen, 2002 DAFx paper on digital peak limiters, is named as background in US9661438B1; not
  opened ((unverified), [Google Patents](https://patents.google.com/patent/US9661438)).
- **Element 4 (soft knee, quadratic).** US5923767A (Sony), priority 1996-03-08: soft knee between two
  thresholds with a low-order (quadratic) polynomial, envelope in the log domain, abstract and
  search summary only ([Google Patents](https://patents.google.com/patent/US5923767)). MathWorks
  Audio Toolbox documents the quadratic soft-knee static curve, undated, not relied on
  ([MathWorks](https://au.mathworks.com/help/audio/ref/compressor.html)).
- **Element 4 (combined compressor, expander, limiter, gate curve).** US5524060A (EuPhonix),
  priority 1992-03-23, a user-defined input-level to gain transfer characteristic held in RAM, per a
  search summary describing combined expander and compressor portions
  ([Google Patents](https://patents.google.com/patent/US5524060)).
- **Element 3 (RMS detector with attack/release filter), element 2b.** US6757396B1 above (absolute
  value plus alpha filter, attack and decay coefficients).
- **Element 6 (reporting latency for plugin-delay compensation).** Steinberg's Cubase/Nuendo manual
  describes plugin delay compensation and lookahead dynamics processors
  ([Steinberg](https://archive.steinberg.help/nuendo/v12/en/cubase_nuendo/topics/audio_effects/audio_effects_plugin_delay_compensation_c.html),
  undated, via search summary). No dated primary source found.
- **Element 7 (makeup gain).** No dated source opened. Open uncertainty.

## Candidates

Patents that might cover an element of the design and might still be in force. Status is filled
in by a human from Google Patents or USPTO Patent Center.

| # | Patent | Assignee | Priority / filing | Est. term end | Element it may touch | Link | Status |
|---|---|---|---|---|---|---|---|
| 1 | US9559650B2, Loudness limiter | Music Group IP Ltd (current per Google: Music Tribe Innovation DK AS) | 2012-07-13 / 2013-07-15 | 2034-11-24 (Google adjusted) | 5, 6 (sliding-window level, maximum of last N levels in the description, delayed input, optional gain smoothing; independent claims need a gain from at least two level values and a delayed input, no maximum recited) | [GP](https://patents.google.com/patent/US9559650) | In force (checked by Chris Paro, 2026-11-12) |
| 2 | US9608588B2, Dynamic range control with large look-ahead | Apple Inc. | 2014-01-22 / 2014-06-10 | 2034-10-27 (Google) | 6, 3 (claim 1: gains applied "to a delayed version of the audio signal", loudness smoothed with adaptive time constants over a look-ahead period) | [GP](https://patents.google.com/patent/US9608588) | In force (checked by Chris Paro, 2026-11-12) |
| 3 | US9979369, Audio peak limiting | Dolby Laboratories Licensing Corp | 2016-10-21 / 2017-10-19 | 2037-10-19 (Google) | 5, 6 (per-block peak, minimum of instantaneous gains held over M blocks, first-order smoothing of gain, buffered delay) | [GP](https://patents.google.com/patent/US9979369) | In force (checked by Chris Paro, 2026-11-12) |
| 4 | US11394356, Block-based audio limiter | Amazon Technologies Inc | 2021-02-12 / 2021-02-12 | 2041-02-12 (Google) | 5, 6 (lookahead peak detection, delay, hold of gain, gain history minimum; claims not seen) | [GP](https://patents.google.com/patent/US11394356) | In force (checked by Chris Paro, 2026-11-12) |
| 5 | US9661438B1, Low latency limiter | Amazon Technologies Inc | 2015-03-26 / 2015-03-26 | 2035-03-26 (Google; page prints "Expired - Fee Related") | 2a (largest absolute value across left and right channels sets one gain), 6 (delayed output, gain ramped down over the delay) | [GP](https://patents.google.com/patent/US9661438) | Lapsed: fee-related expiry (checked by Chris Paro, 2026-11-12) |
| 6 | US7729673B2, Method and apparatus for multichannel signal limiting | Sony Ericsson Mobile Communications AB (current per Google: Sony Corp) | 2004-12-30 / 2005-03-03 | 2028-04-23 (Google adjusted; page prints "Expired - Fee Related") | 2a (one limiting factor from the worst-case channel applied to all channels), 6 (delayed channels before gain) | [GP](https://patents.google.com/patent/US7729673) | Lapsed: fee-related expiry (checked by Chris Paro, 2026-11-12) |
| 7 | US11252507B2, Dynamics processing method and dynamics processing device | Yamaha Corp | 2018-03-23 / 2020-09-21 | 2039-03-16 (Google adjusted) | 3 (timer after the level crosses the threshold; hold time and attack/release time constants in the description; claim 1 does not recite them) | [GP](https://patents.google.com/patent/US11252507B2/en) | In force (checked by Chris Paro, 2026-11-12) |
| 8 | US10636448B2, Audio buffering for processing with variable lookahead | Apple Inc. | 2018-09-17 / 2018-09-17 | 2038-09-17 (Google) | 6 (claims 6, 16: peak limiter processes using first and second lookahead; claim 1 a cross-fader between them) | [GP](https://patents.google.com/patent/US10636448B2/en) | In force (checked by Chris Paro, 2026-11-12) |
| 9 | US8983092B2, Waveform shaping system to prevent electrical and mechanical saturation in loud speakers | Conexant Systems LLC (current per Google: Synaptics Inc, Lakestar Semi Inc) | 2010-07-15 / 2011-07-14 | 2033-10-17 (Google adjusted) | 5, 6 (look-ahead buffer and analysis engine forming a dynamic range compressor or peak limiter; gain smoothed by low-pass or attack/release; claims not seen) | [GP](https://patents.google.com/patent/US8983092) | In force (checked by Chris Paro, 2026-11-12) |
| 10 | US11011180B2, Audio signal dynamic range compression | Guoguang Electric Co Ltd | 2018-06-29 / 2018-06-29 | 2038-06-29 (Google) | 3 (attack and release gain-response times as low-pass parameters, changed when a transient is detected; the design's times are fixed) | [GP](https://patents.google.com/patent/US11011180B2/en) | In force (checked by Chris Paro, 2026-11-12) |
| 11 | EP3588776B1, Audio signal dynamic range compression | Guoguang Electric Co Ltd | 2018-06-29 / 2018-11-14 | 2038-11-14 (Google; page prints "Revoked") | 3 (same family as row 10) | [GP](https://patents.google.com/patent/EP3588776B1/en) | Revoked (checked by Chris Paro, 2026-11-12) |

Status values: **In force**, **Lapsed (fees)**, **Expired (term)**, **Abandoned**, **Invalidated**,
**Not found**. Record who checked and when, under the table.

Rows 5, 6 and 11 are listed because their printed term end is in the future; Google prints
fee-lapsed or revoked status for them, which a human should confirm.

Expired by term (filing date + 20 years, plus any printed term adjustment, is in the past), listed
for completeness and needing no check:

- [US6535846B1](https://patents.google.com/patent/US6535846), K S Waves, filed 2000-08-07, priority
  1997-03-19, expiry 2021-02-12 per Google. Compressor-limiter and low-level expander with
  look-ahead and FIR smoothing as long as the delay (elements 3, 5, 6).
- [US6757396B1](https://patents.google.com/patent/US6757396), Texas Instruments, filed 1999-09-27,
  expiry 2019-09-27 per Google. Compressor with RMS estimator, minimum selector across channels,
  attack/release filter (elements 2, 3).
- [US5923767A](https://patents.google.com/patent/US5923767), Sony, filed 1997-02-19, expiry
  2017-02-19 per Google. Soft knee with a low-order polynomial (element 4).
- [US5737434A](https://patents.google.com/patent/US5737434), Orban, filed 1996-08-26, expiry
  2016-08-26 per Google. Peak limiter with delayed audio and held, low-pass-filtered control
  (elements 5, 6).
- [US5524060A](https://patents.google.com/patent/US5524060), EuPhonix, filed 1994-02-14, expiry
  2013-06-04 per Google. User-defined level-to-gain transfer curve (element 4).
- [US5334947A](https://patents.google.com/patent/US5334947), Aphex Systems, filed 1993-07-06, expiry
  2013-07-06 per Google. Noise gate hold timer (element 3).
- [US7013011B1](https://patents.google.com/patent/US7013011), Plantronics, filed 2001-12-28, expiry
  2024-04-29 per Google. Audio limiting with delayed input, smoothed attenuation (elements 5, 6).

## Claim analysis

Claims are quoted from the Google Patents HTML text of each patent, which the page does not mark as
OCR. The fetch tool only returns quotes of up to about 125 characters, so each claim came back as
consecutive verbatim pieces split at punctuation; they are joined here with single spaces in place
of the page's line breaks. Check the quotes against the patent PDFs before relying on them. "Met"
and "not met" below are a technical reading, not a legal construction.

### Row 1. US9559650B2, Loudness limiter (priority 2012-07-13)

Independent claims: 1, 30, 54, 55, 57. Claims 1, 30, 55 and 57 were read in full; claim 54 only by
its opening ("A non-transitory, computer-readable storage medium storing instructions to execute a
loudness limiting method") and the fetch tool's summary that it recites claim 1's steps.

Claim 1, read against element 5 with the delay of element 6:

- "A loudness limiting method for attenuating an input audio signal having an initial loudness
  level in order to constrain a resulting loudness level of an output audio signal by a
  pre-determined threshold loudness level" - element 5 attenuates to hold the sample peak under the
  ceiling `C`. Met only if a sample-peak ceiling is a "threshold loudness level" (deciding term).
- "receiving the input audio signal comprising a sequence of input signal values by a loudness
  signal processor" - met; per-sample input.
- "the loudness signal processor determining a time-varying level of said input audio signal by
  means of a finite-length sliding window, the time-varying level comprising time-varying level
  values each based on an individual time interval of a multiple of said input signal values" -
  met by element 5's sliding-window maximum of `max(|L|, |R|)`; each value covers the lookahead
  window of samples.
- "the loudness signal processor calculating a time-varying attenuation gain on the basis of said
  time-varying level of said input audio signal and said pre-determined threshold loudness level" -
  met by element 5's ceiling gain from the smoothed level and `C` (subject to "loudness").
- "the loudness signal processor time-delaying said input audio signal" - met by element 6.
- "the loudness signal processor applying said time-varying attenuation gain to said time-delayed
  input audio signal by applying an individual attenuation gain value to a corresponding individual
  input signal value resulting in said output audio signal having a constrained loudness level" -
  met by elements 1, 5, 6: per-sample gain applied to the delayed sample. Taking the minimum with
  the main curve's gain adds a step; it does not remove this one.
- "wherein said individual attenuation gain value is based on at least two of said time-varying
  level values for which said individual time interval comprises said corresponding individual
  input signal value" - met by element 5: the box filter averages one window's worth of window
  maxima, and the design's guarantee that the smoothed level is "never below the true peak of the
  delayed signal" holds only when each averaged window contains the delayed sample.
- "providing said output audio signal having the constrained loudness level from the loudness
  signal processor, such that an estimated loudness level of the output audio signal substantially
  never exceeds said pre-determined threshold loudness level." - element 5 keeps the output sample
  peak at or under `C`. Met only if a sample peak is an "estimated loudness level".

The main path (elements 2 to 4) does not meet claim 1: its level comes from a one-pole recursive
smoother, not "a finite-length sliding window", and each gain comes from one smoothed value.

Claim 30 is the apparatus form of claim 1: "a level detector arranged to determine a time-varying
level of said input audio signal by means of a finite-length sliding window", "a gain allotter
arranged to calculate a time-varying attenuation gain on the basis of said time-varying level of
said input audio signal and said pre-determined threshold loudness level", "a time-delay arranged
to delay the input audio signal", "an attenuator arranged to apply said time-varying attenuation
gain to the delayed input audio signal by applying an individual attenuation gain value to a
corresponding individual input signal value, resulting in said output audio signal, wherein said
individual attenuation gain value is based on at least two of said time-varying level values for
which said individual time interval comprises said corresponding individual input signal value."
Same mapping as claim 1; "loudness" appears in its preamble ("constrain a resulting loudness level
of an output audio signal by a pre-determined threshold loudness level") and in "said
pre-determined threshold loudness level".

Claim 55 drops the sliding window:

- "determining a time-varying level of said input audio signal by determining a plurality of
  time-varying level values each based on multiple of said input signal values, the time-varying
  level representing a loudness" - element 5's window maxima are each based on many samples; that
  they represent "a loudness" is the deciding question. The description says a loudness level "is
  based on a calculation typically as a means of estimating the perceived loudness level of the
  signal", calls an RMS detector "a (very) primitive loudness level estimator", and sets the
  invention against limiters "designed to limit signal transients or limit the maximum signal
  amplitude" (description quotes via the fetch tool).
- "calculating a time-varying attenuation gain of which an individual attenuation gain value for a
  corresponding individual input signal value is based on a non-linear function of at least two of
  said plurality of time-varying level values of said input audio signal and said pre-determined
  threshold loudness level, wherein all of said at least two of said plurality of time-varying
  level values are based on said corresponding individual input signal value" - element 5: the
  ceiling gain is a non-linear function (ratio to `C`, minimum) of the averaged maxima, all of
  whose windows contain the sample. Met.
- "time-delaying said input audio signal", "applying said time-varying attenuation gain to said
  time-delayed input audio signal by applying an individual attenuation gain value to said
  corresponding individual input signal value", "such that an estimated loudness level of the
  output audio signal substantially never exceeds said pre-determined threshold loudness level" -
  as claim 1.

Claim 55 against the main path in RMS mode (element 2b, which the patent's own usage treats as a
loudness estimate) with lookahead: the gain for a sample comes from one smoothed level value
("not smoothed again", element 3), so "based on a non-linear function of at least two of said
plurality of time-varying level values" is the deciding element; and with a finite ratio the output
level is not held under a threshold, so "substantially never exceeds" is met at most in limiter
settings.

Claim 57: "the calculated time-varying level comprising a time-varying predicted level of an output
signal" and "wherein said time-varying attenuation gain is based on a level of said input signal and
said time-varying predicted level of said output signal" - not met; the design predicts no output
level.

**Verdict: Unclear.** Deciding element: "loudness" in "pre-determined threshold loudness level" and
"estimated loudness level of the output audio signal" (claims 1, 30, 55) and "the time-varying level
representing a loudness" (claim 55). If a sample-peak level counts, claims 1, 30 and 55 read on
element 5 with element 6; if not, none does. Claim 57 does not read. Prior art in this note dated
before 2012-07-13: musicdsp.org lookahead limiter (2009-11-16: window maximum, averaged gain
reduction, delayed sample), US6535846B1 (1997, FIR smoother as long as the look-ahead delay),
US5737434A (1996), US7013011B1 (filed 2001), Giannoulis et al. (June 2012, unverified).

### Row 2. US9608588B2, Dynamic range control with large look-ahead (priority 2014-01-22)

Independent claims 1, 14, 15, read in full; 14 and 15 recite the same steps as 1 (14 adds "using a
characteristic unit to map the shaped loudness estimate, as its input, to the DRC gain values as its
output"). Claim 1:

- "A method of performing dynamic range control (DRC) with look-ahead on an audio signal" - met
  (elements 4, 6).
- "generating a loudness estimate for the audio signal" - element 2 level detection; met at most
  in RMS mode.
- "generating adaptive time constants by (i) pre-smoothing the loudness estimate, and (ii) analyzing
  the pre-smoothed loudness estimate during a predefined look-ahead time period by estimating
  amounts of change in the pre-smoothed loudness estimate over time intervals, respectively, in the
  predefined look-ahead time period, wherein estimating the amounts of change in the pre-smoothed
  loudness estimate comprises generating one or more decay slopes for the pre-smoothed loudness
  estimate and generating one or more rise slopes for the pre-smoothed loudness estimate" - not met.
  Element 3's coefficients are fixed from the user's attack and release times; nothing pre-smooths
  the level or measures rise and decay slopes over the lookahead. Element 5's box filter has a fixed
  length.
- "smoothing the loudness estimate using the adaptive time constants to produce a shaped loudness
  estimate" - not met (fixed constants).
- "generating DRC gain values to apply to a delayed version of the audio signal based on the shaped
  loudness estimate" and "applying the DRC gain values to the delayed audio signal." - met apart
  from "shaped" (elements 4, 6).

**Verdict: Does not read.** Missing: "generating adaptive time constants by (i) pre-smoothing ...
(ii) analyzing the pre-smoothed loudness estimate during a predefined look-ahead time period". Prior
art before 2014-01-22: US6535846B1, musicdsp 2009.

### Row 3. US9979369, Audio peak limiting (priority 2016-10-21)

Claim 1 read in full; claims 9 ("An audio peak limiter apparatus comprising: an interface for
receiving and outputting audio data; a memory system for saving audio") and 10 ("A non-transitory
computer readable medium comprising instructions suitable for configuring a logic system of an
audio peak limiter apparatus to") read by their openings only; the fetch tool reports both refer to
claim 1's method. Claim 1:

- "A method in an audio peak limiter apparatus, the apparatus being configured to calculate a
  smoothed sequence of gains for application to a sequence of blocks of samples of an audio signal,
  each of the blocks representing a respective temporal segment of the audio signal not greater
  than 5 milliseconds in duration" - the design computes a gain per sample; met only if one sample
  is a "block".
- "determining that applying a current gain of the sequence to a current block of samples would
  produce at least one scaled sample whose magnitude exceeds a predetermined limit" - element 5
  does not test a gain against the samples; it derives the gain from the smoothed level. Not met as
  stated.
- "calculating a candidate gain for replacing said current gain of the sequence, such that applying
  the candidate gain to the current block of samples would produce at least one scaled sample whose
  magnitude is substantially equal to the predetermined limit and no scaled sample whose magnitude
  exceeds the predetermined limit" - no candidate or replacement gain in the design. Not met.
- "determining that the candidate gain is greater than or equal to the final gain stored in
  connection with the immediately previous gain of the sequence" - not met; no stored final gains.
- "calculating a current final gain and storing it in connection with the current gain of the
  sequence, the current final gain being obtainable by dividing the gain of the sequence by a prior
  gain of the sequence and multiplying the result by the final gain stored in connection with said
  prior gain of the sequence." - not met; the design forms no such ratio.

**Verdict: Does not read.** Missing: the characterising steps, "determining that the candidate gain
is greater than or equal to the final gain stored ..." and "calculating a current final gain ...
obtainable by dividing the gain of the sequence by a prior gain of the sequence and multiplying the
result by the final gain stored in connection with said prior gain". Prior art before 2016-10-21:
musicdsp 2009, US5737434A, US6535846B1, US9559650B2.

### Row 4. US11394356, Block-based audio limiter (priority 2021-02-12)

Independent claims 1 (method) and 12 (system, the same steps as instructions to "at least one
processor"), read in full. Claim 1:

- "receiving first audio data" - met.
- "determining a first maximum amplitude value associated with a first plurality of audio samples
  within the first audio data, the first plurality of audio samples corresponding to a first time
  window" - met by element 5's sliding-window maximum of `max(|L|, |R|)` over the lookahead window.
- "determining that the first maximum amplitude value exceeds a threshold value" - the design does
  not state a comparison of a window maximum with `C`; the ceiling gain is formed from the
  box-filtered level. Met only if the ceiling-gain computation (for example `min(1, C / level)`)
  counts as this determination, and then it compares the average, not the maximum.
- "determining a first gain value based on the threshold value and the first maximum amplitude
  value" - each ceiling gain is a function of `C` and the average of one window's worth of window
  maxima, of which this maximum is one. Deciding element: whether "based on" covers a gain from an
  average of several maxima.
- "determining a first plurality of gain values using the first gain value" - the design forms one
  ceiling gain per sample, each from its own averaged level, and combines each with the main gain by
  minimum; it does not derive several gains from one gain. Met only if the per-sample gains, which
  share window maxima, count as "using the first gain value".
- "generating second audio data using the first plurality of gain values and the first plurality
  of audio samples." - met (elements 1, 6).

Dependent claim 3 ("determining a first amplitude value that is a highest value of first audio
samples ... corresponding to a first loudspeaker; determining a second amplitude value ...
corresponding to a second loudspeaker; determining that the first amplitude value exceeds the
second amplitude value") matches the `max(|L|, |R|)` link, if claim 1 reads. Claim 4 (ramp down from
a preceding gain, then hold) has no counterpart.

**Verdict: Unclear.** Deciding elements: "determining a first gain value based on the threshold value
and the first maximum amplitude value" together with "determining a first plurality of gain values
using the first gain value" (and "determining that the first maximum amplitude value exceeds a
threshold value"). A variant that forms a gain per window maximum and then averages gains (the
musicdsp structure) would meet these more plainly than the design, which averages levels. Prior art
before 2021-02-12: musicdsp 2009-11-16 (window maximum, gain reduction from it, averaging, delayed
sample), US5737434A (1996), US6535846B1 (1997), US7013011B1, US9559650B2 (2012). The Signalsmith
post (2022-04-08) is later.

### Row 7. US11252507B2, Dynamics processing method and device (priority 2018-03-23)

Independent claims 1 (method) and 7 (device with "a timing controller configured to" perform the
same determinations), read in full. Claim 1:

- "receiving an input audio signal" - met.
- "determining whether a level of the input audio signal exceeds a threshold value" - element 4
  places the smoothed level against `T`; met loosely.
- "determining, in a case where it is determined that the level of the input audio signal exceeds
  the threshold value, whether a first amount of time, that is not specified to a unique value by a
  first speed at which a gain applied to the input audio signal is changed, has elapsed from when the
  level of the input audio signal exceeded the threshold value" - not met. The design's only timer
  is element 3's hold, which starts at "the last sample that raised the level", with no reference to
  any threshold, and delays the start of release. The description makes the first speed the attack
  ("decrease at a first speed (rate), which is indicated by Attack"), the first time a delay
  ("delayed by a first amount of time indicated by Dtime") started when the level "crosses
  Threshold value".
- "changing, in a case where it is determined that the level of the input audio signal exceeds the
  threshold value and that the first amount of time has elapsed from when the level of the input
  audio signal exceeded the threshold value, the gain applied to the input audio signal at the first
  speed." - the design's attack begins at once; it applies no delay from a threshold crossing.

No dependent claim recites hold, timer, attack or release (claims 2 to 4 set how the first time is
chosen; 5 and 6 the direction of the gain change).

**Verdict: Does not read.** Missing: "whether a first amount of time ... has elapsed from when the
level of the input audio signal exceeded the threshold value". Prior art before 2018-03-23:
US5334947A (1993), a gate hold timer started when the input exceeds the threshold.

### Row 8. US10636448B2, Audio buffering for variable lookahead (priority 2018-09-17)

Independent claims 1, 11, 17, read in full. Claim 11:

- "receiving an audio signal into a buffer" - met (element 6 delay line).
- "processing the audio signal from the buffer in a first digital signal processing module, using a
  first lookahead" - met (one lookahead).
- "processing the audio signal from the buffer in a second digital signal processing module, using a
  second, greater lookahead" - not met; the design has one processing path with one lookahead.
- "cross-fading a processed audio signal, between output of the first digital signal processing
  module and output of the second digital signal processing module, depending on lookahead depth of
  data of the audio signal in the buffer." - not met; no cross-fader.

Claims 1 ("a second digital signal processing module ... using a second lookahead that is greater
than the first lookahead" and "a cross-fader") and 17 ("process the audio signal from the buffer
using a second, greater lookahead" and "cross-fade a processed audio signal") fail on the same
elements.

**Verdict: Does not read.** Missing: the second module with a greater lookahead and the cross-fade
between the two outputs. Prior art: none in this note bears on cross-fading between lookaheads.

### Row 9. US8983092B2, Waveform shaping system (priority 2010-07-15)

Independent claims 1, 10 (system) and 15 (method), read in full. Claim 15:

- "A method for limiting peaks in a first digital signal" - met (element 5).
- "converting a digital audio signal into a displacement signal using a displacement model" - not
  met; the design has no loudspeaker displacement model.
- "storing a plurality of samples of the displacement signal in a look-ahead buffer" - the design
  buffers audio samples, not a displacement signal. Not met.
- "determining whether a sample of the plurality of samples in a predetermined position in the
  look-ahead buffer has a value which exceeds a given threshold" and "if the sample exceeds the given
  threshold, applying a gain envelope function to the plurality of samples in the look-ahead buffer,
  where the gain envelope function is selected on the basis of the value of the plurality of samples
  in the look-ahead buffer and the given threshold" - the design applies one gain to the delayed
  output sample, not an envelope over the buffer contents. Not met as stated.
- "converting the displacement signal back into the digital audio signal using a model inverse." -
  not met.

Claim 1 recites "a displacement model operable to convert a digital audio signal into a displacement
signal" and "a model inverse operable to convert the displacement signal back into the digital audio
signal"; claim 10 adds "a root-mean-square estimation (RMS) module" and "a dynamic range compressor
(DRC) applying a gain to an audio signal" (which element 2b and 4 would meet) but keeps the
displacement model and model inverse.

**Verdict: Does not read.** Missing: the displacement model and model inverse (all three independent
claims). Prior art before 2010-07-15: US6535846B1, US5737434A, US7013011B1.

### Row 10. US11011180B2, Audio signal dynamic range compression (priority 2018-06-29)

Independent claims 1 (method) and 12 (system), read in full. Claim 1's last element was assembled
from two fetches of the page that meet at the fetch-tool offset ("... in response to a det" and
"ermination that the transient is occurring ..."). Claim 1:

- "(a) obtaining an input audio signal" - met.
- "(b) providing a time-varying gain signal based on the input audio signal and a desired output
  range" - met (elements 2 to 4).
- "(c) applying the time-varying gain signal to the input audio signal to provide an output audio
  signal" - met.
- "(i) determining and providing an indication of whether a sudden powerful burst of sound, referred
  to as a transient, is occurring in the input audio signal" - not met; no transient detector.
- "(ii) providing an attack gain-response time and a release gain-response time based on the
  indication of whether the transient is occurring in the input audio signal" - not met; the times
  are fixed.
- "(iii) lowpass filtering a signal that is based on the input audio signal, using the attack
  gain-response time and the release gain-response time as filtering parameters" - met by element 3.
- "wherein the attack gain-response time is decreased and the release gain-response time is
  increased in response to a determination that the transient is occurring in the input audio
  signal." - not met.

Claim 12 recites "an attack-detection module" and "wherein the gain-response-time generator shortens
the attack gain-response time and increases the release gain-response time in response to the
transient detection by the attack-detection module"; not met.

**Verdict: Does not read.** Missing: the transient indication and the attack and release times that
change with it. Prior art: US6757396B1 (1999) for fixed attack/release filtering of the level.

## Search log

All searches 2026-11-12. The search tool returns a prose summary, not raw result lists, so hit
counts are approximate (about 10 per query unless noted).

| Database | Query | Date | Hits | Result |
|---|---|---|---|---|
| Web (general) | Giannoulis Massberg Reiss Digital Dynamic Range Compressor Design A Tutorial and Analysis JAES 2012 vol 60 | 2026-11-12 | 9 | Bibliographic details only; AES page 403 |
| Web (general) | McNally "Dynamic range control of digital audio signals" JAES 1984 vol 32 lookahead | 2026-11-12 | 10 | Citation details; US9608588, US9559717 surfaced |
| Web (general) | Signalsmith Audio "Limiter" blog sliding maximum box filter lookahead | 2026-11-12 | 18 | Signalsmith post reached by direct fetch; musicdsp thread |
| Web (general) | Lemire "Streaming maximum-minimum filter ..." 2006 Nordic Journal of Computing | 2026-11-12 | 10 | arXiv cs/0610046 |
| Web (general) | Gil Werman "Computing 2-D min, median, and max filters" TPAMI 1993 van Herk ... 1992 | 2026-11-12 | 10 | Gil-Werman confirmed; van Herk not found in this query |
| Web (general) | van Herk "A fast algorithm for local minimum and maximum filters on rectangular and octagonal kernels" Pattern Recognition Letters 1992 | 2026-11-12 | 10 | dblp record, DOI |
| Web (general) | patent dynamic range compressor lookahead brickwall limiter sliding window maximum moving average smoothing gain | 2026-11-12 | 10 | US9559650, US9608588, US6535846, US6757396 |
| Web (general) | patents.google.com look-ahead limiter "maximum" over window "moving average" gain reduction attack filter delayed audio signal peak hold | 2026-11-12 | 10 | US9559650, US6535846, US11394356, US7729673 |
| Web (general) | patents.google.com audio dynamics processor compressor expander gate soft knee threshold ratio range hold time attack release downward expansion patent | 2026-11-12 | 10 | No patent; manuals |
| Web (general) | patents.google.com compressor limiter combined gain minimum of compressor gain and limiter gain lookahead delay latency reporting digital audio | 2026-11-12 | 10 | US6535846, US6757396, US5737434, US9661438 |
| Web (general) | patents.google.com limiter "moving maximum" OR "sliding maximum" OR "running maximum" filter gain lookahead audio "box filter" OR "moving average" ... | 2026-11-12 | 10 | US9979369, US9264836 (DTS loudness; not pursued, not on an element), US9559650 |
| Web (general) | patent audio compressor soft knee quadratic interpolation gain computer threshold knee width ratio level detector attack release smoothing before gain computer | 2026-11-12 | 10 | US5923767; EP3588776 |
| Web (general) | patent "look-ahead" limiter gain "minimum" over lookahead window then low-pass OR "moving average" filter ... (extended mode) | 2026-11-12 | 10 | US10636448, US6535846, US9661438; Signalsmith post |
| Web (general) | patent audio dynamics gate hold timer release does not begin until hold period expired after level exceeded threshold smoothed level attack release coefficients | 2026-11-12 | 10 | US11252507 |
| Web (general) | patent combined compressor expander limiter gate single dynamics processor transfer curve knee both sides below threshold expansion ratio range limit | 2026-11-12 | 10 | US5524060; US4224581, US4412189 named in an index, not opened |
| Web (general) | patents.google.com noise gate expander "hold time" "range" attenuation floor downward expander ratio digital audio processor gain computer patent active | 2026-11-12 | 10 | US5334947 |
| Web (general) | patents.google.com dynamic range compression RMS detector stereo link maximum of left and right power smoothing attack release patent | 2026-11-12 | 10 | US6757396, EP3588776 |
| Web (general) | Guoguang Electric Yuli You "Audio signal dynamic range compression" US patent granted | 2026-11-12 | 10 | US11011180B2; US10755722B2 (multiband, not opened) |
| Web (general) | patent audio limiter lookahead "sliding window" maximum deque "monotonic" OR "ring buffer of maxima" peak gain smoothing true peak ceiling | 2026-11-12 | 10 | US7013011, US8229124 (DTS, not opened), US8983092 |
| Web (general) | patent dynamics processor plugin reports latency to host lookahead delay compensation compressor limiter plugin delay compensation claim | 2026-11-12 | 10 | No patent; Steinberg manual |
| Web (general) | Zolzer DAFX Digital Audio Effects Wiley 2002 ISBN 0471490784 chapter 3 dynamic range control ... | 2026-11-12 | 10 | dafx.de chapter page, 2nd-edition contents |
| Web (general) | Signalsmith Audio blog "peak-hold" OR "Using a monotonic queue" ... | 2026-11-12 | 17 | No Signalsmith page; tutorials only |
| Web (general) | patent digital compressor limiter lookahead gain computed from delayed signal peak "attack" "hold" "release" brickwall oversampling true peak Waves OR iZotope OR FabFilter OR Native Instruments OR Ableton | 2026-11-12 | 10 | US9661438 only; no vendor patents |
| Web (general) | patents.google.com audio compressor "soft knee" "ratio" "threshold" downward expander gate "range" single gain curve ... 2020..2025 | 2026-11-12 | 10 | No patent; manuals |
| Google Patents (page fetch by number) | US9608588, US9559650, US6535846, US6757396, US7729673, US9661438, US5737434, US9979369, US11394356, US5923767, US9264836, US11252507B2, US10636448B2, EP3588776B1, US5524060, US7013011, US8983092, US11011180B2, US5334947 | 2026-11-12 | 19 | Dates and status read; US9559650 claims read at fetch offset 100000; US9264836 not a candidate |
| Google Patents (page fetch by number) | US9559650B2, US9608588B2 (the "B2/en" form) | 2026-11-12 | 2 | 404 for US9559650B2; US9608588B2 worked. Fetched by bare number instead |
| Google Patents, claims (stage 2) | US9559650 (bare number, offset 100000; also description for "loudness"), US11394356 (offset 100000), US8983092 (offset 100000), US9979369, US9608588B2/en, US11252507B2/en (claims and description), US10636448B2/en, US11011180B2/en (offsets 0 and 100000) | 2026-11-12 | 8 | Granted claims read from the HTML text (not marked OCR), in verbatim pieces of up to about 125 characters; rows 4 and 9 claims found past offset 100000 |
| FreePatentsOnline | 11394356.html | 2026-11-12 | 0 | HTTP 403; not needed, Google Patents gave the claims |

Refused or failed: AES e-library (`https://www.aes.org/e-lib/browse.cfm?elib=16354` and `=4505`,
`secure.aes.org`, `aes.org/publications/elibrary-page/?id=...`) returned HTTP 403. Google Patents
search UI (`https://patents.google.com/?q=...`) returned only the heading (JavaScript app).
Espacenet search returned HTTP 403. USPTO Patent Public Search was not tried (JavaScript app, failed
in an earlier note). `code.soundsoftware.ac.uk/publications/141` refused the connection. Google
Patents `US11394356B2/en` returned 404; the bare number worked. Because no patent database query
interface worked, no search was run by CPC class (for example H03G 9/00, H03G 7/00, G10L, H03G 3)
or by inventor or assignee name in a database; every candidate came through a general web search or
the bare-number page fetch.

## Open uncertainties

- Giannoulis et al. 2012 and McNally 1984: the AES pages could not be opened, so the dates and
  bibliographic details rest on search summaries and a patent reference list, and the content
  (feed-forward topology, quadratic soft knee, ballistics on level versus gain, lookahead) is not
  confirmed from the papers. Reiss's QMUL page and the Rochester (White, Dick) and Academia copies
  surfaced in search but were not opened.
- Zolzer DAFX chapter texts were not read; whether the 2002 edition already holds RMS detection,
  knee, hold and lookahead is not established.
- van Herk 1992 paper not opened; the deque form of sliding maximum has no dated primary source in
  this note (Lemire's abstract does not mention one; his code repository was not opened).
- Earliest Signalsmith post on peak-hold and smoothing (referred to as "earlier posts") not found;
  the musicdsp 2009-11-16 thread is the earliest dated source for the sliding-max plus averaging
  limiter, and it averages the gain reduction, not the level.
- Element 2b (RMS as smoothed power then square root, with the attack/release ballistics applied to
  the power) and element 3 (hold timer measured from the last sample that raised the level, with the
  gain not smoothed again) have no dated prior art other than the hold-timer gate patent US5334947A.
- Element 4 (quadratic soft knee applied on the expansion side, `range` clamp, optional ceiling with
  its own knee) has no dated source opened other than the Sony patent's description of a
  polynomial knee.
- Element 7 (makeup gain) has no source.
- **Row 1 (US9559650B2), for a human or counsel to decide:** whether a sample-peak level and a
  sample-peak ceiling are a "loudness level" / "threshold loudness level" / "level representing a
  loudness" in claims 1, 30 and 55. Every other element of those claims is met by element 5 with
  element 6. The description ties loudness to perceived loudness and contrasts limiters of "the
  maximum signal amplitude", which points away from peak, but this is a claim-construction question.
  Also for row 1: the main path in RMS mode with lookahead against claim 55, which turns on "based
  on a non-linear function of at least two of said plurality of time-varying level values" (the
  design gains each sample from one smoothed value). Claim 54 was not read in full.
- **Row 4 (US11394356), for a human or counsel to decide:** whether a per-sample gain computed from
  `C` and the box-filter average of window maxima is "a first gain value based on the threshold value
  and the first maximum amplitude value", whether per-sample gains sharing window maxima are "a first
  plurality of gain values using the first gain value", and whether the design meets "determining
  that the first maximum amplitude value exceeds a threshold value". Whether the musicdsp.org 2009-11-16 post
  anticipates claim 1 is also for a human; it predates the 2021-02-12 priority.
- Possible design-around for rows 1 and 4, if a human wants one: none is proposed here; the choice
  between accepting the risk, seeking an opinion, or changing element 5 is a `patent` flag-risk
  decision.
- Row 7 (US11252507B2): verdict Does not read rests on the design's hold being measured from the last
  level rise with no reference to the threshold, and delaying release, not the gain change. A case
  where the last rise is also the threshold crossing and the level then falls but stays over `T`
  makes the hold expiry coincide with "the first amount of time ... has elapsed from when the level
  ... exceeded the threshold value", followed by a gain change at the release speed. Claim 1 does not
  itself say the first speed is the attack. A human should confirm this coincidence does not matter.
- Row 8 (US10636448B2): reads only if the design changes its lookahead at run time by running two
  lookahead paths and cross-fading; element 6 does not say how a lookahead change is handled.
- Row 3 (US9979369): claims 9 and 10 read only by their openings; the fetch tool reports they
  refer to claim 1. Whether a one-sample "block" meets the 5 ms block element is moot, as the
  characterising steps are missing.
- All claim quotes came through a fetch tool that returns verbatim pieces of up to about 125
  characters; check them against the patent PDFs (linked from each Google Patents page) before
  relying on them. Row 10's claim 1 joins two fetches at a page offset. Rows 5, 6 and 11 print a
  lapsed or revoked status on Google; a human confirmed them as not in force.
- Term end dates are as Google prints them; terminal disclaimers and adjustments not otherwise
  seen. Row 4's 2041-02-12 is twenty years from filing with no adjustment shown.
- Not checked: other patents of Dolby, Apple, Amazon, Yamaha or Guoguang (for example US10755722B2,
  Guoguang multiband compression with overshoot suppression); DTS loudness patents US9264836B2 and
  US8229124 (gain smoothing in a look-ahead line, loudness control); US4224581 and US4412189
  (old compander/limiter patents named in an index, almost certainly expired); cited-by and cited
  lists of the candidates; patents held by Waves, iZotope, FabFilter, Native Instruments,
  Cytomic, Ableton or other dynamics-plugin vendors by assignee name.
- No patent found on reporting a lookahead delay as latency for plugin-delay compensation (search
  log, "plugin reports latency" query); the search tool does not search claims text.
