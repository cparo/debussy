# Patent status of the generalised dynamics effect

Research note, 2026-11-11. This is not legal advice. It records what primary sources show, so a
reviewer can check it. Stage 1 listed the candidates; stage 2 (2026-11-12) read the independent
claims of the in-force ones against Planned design (see Claim analysis).

**Decision (Chris Paro, 2026-11-12, `flag-risk` tag `patent`):** weighted-peak mode is dropped;
no mode derives a gain from a block's peak, so row 1's "maximum amplitude value" element is
absent and the design as built does not read on any in-force candidate. Recorded in ADR-0100.
Element 5 later moved to normalised dB levels (ADR-0102); no verdict above depends on the curve's
form.

## Planned design

Software, portable scalar Rust, `f32`, one effect node in an audio editor (stereo in, stereo out),
processed on the real-time audio thread at 44.1 to 192 kHz. One configurable unit used, by its
settings, as a compressor, limiter, gate/expander, or overdrive/saturator.

1. **Preamp gain.** Input multiplied by a gain set in dB.
2. **Detector.** Per sample, the squared value (weighted-RMS mode) or the absolute value
   (weighted-peak mode). With **linked channels** (default on) left and right share one detector
   value (mean of squares, or maximum of absolutes); unlinked, each channel has its own.
3. **Block reduction.** Detector values are reduced over fixed blocks of about 0.5 to 1 ms (sum
   of squares, or maximum of absolutes), giving a control-rate sequence.
4. **Asymmetric weighting window (envelope).** The level is a weighted combination of block values
   over a window defined by attack time `A` (0 to 100 ms), attack exponent `a`, release time `R`
   (0 to 5000 ms), release exponent `r` (exponents in [0.1, 16]) and lookahead `L` (0 to 100 ms).
   With time `t'` relative to the current sample, positive into the future, the window's peak
   (weight 1) sits at `p = L - A`. Future side: `w = 1 - ((t' - p)/A)^a` for `t'` in `(p, p + A]`.
   Past side: `w = 1 - ((p - t')/R)^r` for `t'` in `[p - R, p)`. The window therefore spans
   `[L - A - R, L]`; the node's output is delayed by `L` (lookahead latency) so the window never
   reads past available input. A zero-length side contributes nothing; with both zero the level
   is the instantaneous absolute sample value (waveshaper mode).
   - RMS mode: level = sqrt(sum(w * blocksum) / sum(w * blocklength)), i.e. normalised by the
     weight sum; a full-scale square wave reads 1.0.
   - Peak mode: level = max over blocks of `w * blockmax`.
   - The window weights are a precomputed table at block resolution, recomputed when a time or
     exponent changes; evaluation is a direct dot product (or sliding maximum) at block rate.
5. **Transfer curve.** Output level `f(x) = tanh((s * x)^c)`, `s` = steepness (set in dB,
   converted to linear), `c` = curve exponent (>= 1). Gain = `f(x) / x`. With `c = 1` this is
   compression toward a ceiling; `c > 1` attenuates low levels (expansion/gating); with the
   instantaneous level it is a tanh-style waveshaper.
6. **Gain interpolation.** The block-rate gain is linearly interpolated per sample and multiplied
   into the (lookahead-delayed) signal.
7. **Asymmetric mode.** Optional second `s`, `c` (and clip ceiling) used for samples whose sign is
   negative; the envelope is shared. Adds even harmonics, like an overdriven valve stage.
8. **DC blocker.** One-pole high-pass at about 5 to 10 Hz after shaping, only when asymmetric mode
   is on.
9. **Makeup gain** (dB) after the transfer curve.
10. **Hard clip** at a ceiling in dB (default 0 dBFS, may be above or below), symmetric, or
    separate positive and negative ceilings in asymmetric mode. Last stage.
11. **Oversampling** option 1x/2x/4x/8x around stages 5 to 10 (polyphase FIR up-sampling, shaping
    and clip at the high rate, filtered down-sampling), to reduce aliasing.
12. **Displays** (UI only): a plot of the window weights against time with the current sample
    marked, and the static input-dB to output-dB curve including preamp, makeup and clip.

## Summary

Stage 2 (claim reading) done 2026-11-12 for the seven in-force candidates (rows 1, 2, 3, 4, 6, 7,
8); rows 5 and 9 lapsed and row 10 was revoked, so they were not read. Per-patent verdicts and the
claim text are under Claim analysis. The design facts relied on: attack/release times, exponents
and lookahead change only while playback is stopped; window weights are a precomputed
block-resolution table; gain is computed once per block (about 0.5 to 1 ms) and linearly
interpolated per sample.

| Technique | Verdict | Confidence |
|---|---|---|
| 1. Preamp gain | Not assessed (trivial; no candidate sought) | n/a |
| 2. Detector, linked channels | No candidate reads. Rows 7 and 8 (DTS) need a selected dominant channel plus loudness approximation filters (row 7) or a same-phase sample count against a threshold (row 8); the design takes the mean of squares or maximum of absolutes of both channels. | High |
| 3. Block reduction | No candidate reads in RMS mode. Row 3 (US9979369B2) needs a substituted candidate gain and a final gain formed by a ratio of gains; row 2 (US11496099B2) needs backward then forward passes over blocks. **Peak mode: Unclear under row 1** (US11394356B1), see element 6. | Medium |
| 4. Asymmetric weighting window | No candidate reads. Row 2 needs a backward pass then a forward pass producing gain contours; the design evaluates one precomputed two-sided weight table directly per block. Row 6 needs a loudspeaker displacement model. | Medium |
| 5. tanh transfer curve | No candidate claims it. It bears on row 1 only through "threshold value" (see element 6). | Medium |
| 6. Gain interpolation, lookahead delay | **Unclear, possible exposure in weighted-peak mode.** Row 1 (US11394356B1, Amazon, to 2041) claims a block maximum compared with a threshold, a gain from threshold and maximum, and per-sample gains from it. The design has block maxima and interpolated per-sample gains but no explicit threshold test; whether the tanh curve's ceiling or steepness is a "threshold value" decides it. RMS mode and waveshaper mode do not read (no maximum over a window). Rows 3, 4, 6, 7, 8 do not read. Raise with `flag-risk` (tag `patent`). | Low |
| 7, 8. Asymmetric mode, DC blocker | No in-force candidate found; Pritchard and Line 6 art is expired | Medium |
| 9, 10. Makeup, hard clip | No candidate reads. Row 6 needs a displacement model and a look-ahead buffer of displacement samples; the clip in element 10 is a plain per-sample ceiling. | High |
| 11. Oversampling around shaping | No candidate reads. Row 2's independent claims recite no oversampling, and they fail on the backward/forward passes; Orban oversampled clippers expired | High |
| 12. Displays | No in-force candidate found; US5524060A (expired) shows transfer characteristics on an operator interface | Medium |

## Prior art

Items marked (unverified) rest on a search summary or an index and I could not open the primary
text. The AES E-Library returned HTTP 403, so no paper below was read from its publisher.

- **Elements 2, 3, 4, 6 (level detector, attack/release smoothing, gain from a look-ahead delayed
  signal).** Giannoulis, Massberg, Reiss, "Digital Dynamic Range Compressor Design: A Tutorial and
  Analysis", JAES vol. 60 no. 6, pp. 399-408, June 2012 (volume, pages and month from a search
  summary, unverified; [AES E-Library](https://www.aes.org/e-lib/browse.cfm?elib=16354) returned 403).
  Zoelzer (ed.), "DAFX: Digital Audio Effects", Wiley, 2002, nonlinear processing chapter with a
  limiter built on a delayed input x(n-D1) (unverified; from a lab report citing it, no primary text
  opened; the [second-edition contents](https://www.safaribooksonline.com/library/view/dafx-digital-audio/9780470665992/c04_level1_2.xhtml)
  list "4.2.1 Limiter").
- **Element 4, 6 (look-ahead delay with smoothed gain), dated patents.** US5737434A (Orban),
  priority 1996-08-26, delayed audio path with a look-ahead clipper and peak limiter
  ([Google Patents](https://patents.google.com/patent/US5737434A/en)). US6535846B1 (K S Waves),
  priority 1997-03-19, compressor-limiter and expander with look-ahead delay and FIR smoothing
  ([Google Patents](https://patents.google.com/patent/US6535846B1/en)). US7013011B1 (Plantronics),
  priority 2001-12-28, attenuation factor smoothed and applied to a delayed input
  ([Google Patents](https://patents.google.com/patent/US7013011B1/en)). All three are patents,
  not papers.
- **Element 2 (linked stereo detection).** US6757396B1 (Texas Instruments), priority 1998-11-16,
  RMS of left and right, lower gain kept and applied to both channels
  ([Google Patents](https://patents.google.com/patent/US6757396B1/en)). Maximum-of-channels and
  summed-power linking are described in a [KVR thread](https://www.kvraudio.com/forum/viewtopic.php?p=6524597)
  (undated in the result; forum post, not cited as prior art).
- **Element 4 (asymmetric two-sided window with attack and release exponents).** No dated source
  found. US7532730B2 (priority 2000-04-28, program-dependent release) and US5444788A (feedback and
  feedforward sidechain) are related to release shaping only
  ([Google Patents](https://patents.google.com/patent/US7532730B2/en)); I did not open US5444788A.
- **Element 5 (tanh transfer curve, waveshaper).** Mikelson, "Hyperbolic Tangent Distortion", The
  Csound Journal, issue "winter 1999" per the URL, page prints no date
  ([article](https://csoundjournal.com/ezine/winter1999/processing/index.html)); it uses tanh for
  valve-style distortion and an offset giving a DC bias that "must be removed". Vicanek,
  "Waveshaper Harmonics", compares tanh, algebraic and arctan shapers (2021 per a search summary;
  title page unreadable, [PDF](https://www.vicanek.de/articles/WaveshaperHarmonix.pdf)). Neither
  shows tanh(x^c) as a gate/expander curve with gain f(x)/x.
- **Elements 7, 8 (asymmetric shaping, even harmonics, DC removal).** US4995084A (Pritchard),
  priority 1987-03-23, asymmetrical bias shifting and a limiter introducing even harmonics
  ([Google Patents](https://patents.google.com/patent/US4995084A/en)). US5636284A (Pritchard),
  priority 1987-03-23, solid-state tube power-amplifier emulation
  ([Google Patents](https://patents.google.com/patent/US5636284A/en)). Analogue circuits; the
  digital DC blocker after an asymmetric shaper has only the Mikelson article above.
- **Elements 10, 11 (hard clip, oversampled clipping and shaping).** US6337999B1 (Orban),
  priority 1998-12-18, oversample, clip, difference downsampled and added back
  ([Google Patents](https://patents.google.com/patent/US6337999B1/en)). US5789689A (Line 6),
  priority 1997-01-17, DSP guitar amplifier avoiding foldover noise
  ([Google Patents](https://patents.google.com/patent/US5789689A/en)); that it uses 8x oversampling
  comes from a [blog](https://www.ampbooks.com/mobile/dsp/preamp/) (unverified).
- **Element 12 (transfer curve display).** US5524060A (EuPhonix), priority 1992-03-23, operator
  interface storing and displaying transfer characteristics
  ([Google Patents](https://patents.google.com/patent/US5524060A/en)). No dated source for a plot
  of the envelope window weights.

## Candidates

Patents that might cover an element of the design and might still be in force. Status is filled
in by a human from Google Patents or USPTO Patent Center. Dates and expiry are as Google Patents
prints them; term end is the printed expiry where there is one, else filing date plus 20 years
(marked "computed"). No term adjustment was seen on any page.

| # | Patent | Assignee | Priority / filing | Est. term end | Element it may touch | Link | Status |
|---|---|---|---|---|---|---|---|
| 1 | US11394356B1, Block-based audio limiter | Amazon Technologies Inc | 2021-02-12 / 2021-02-12 | 2041-02-12 (computed) | 4, 6 (look-ahead delay line, attack/hold/release gain per block) | [GP](https://patents.google.com/patent/US11394356B1/en) | In force (checked by Chris Paro, 2026-11-12) |
| 2 | US11496099B2, Systems and methods for limiter functions | Mimi Hearing Technologies GmbH | 2020-07-28 / 2020-07-28 | 2041-05-12 (Google) | 3, 6, 11 (backward/forward gain passes over blocks, block-wise oversampling) | [GP](https://patents.google.com/patent/US11496099B2/en) | In force (checked by Chris Paro, 2026-11-12) |
| 3 | US9979369B2, Audio peak limiting | Dolby Laboratories Licensing Corp | 2016-10-21 / 2017-10-19 | 2037-10-19 (Google) | 3, 6 (smoothed gains over sample blocks, candidate gain substitution) | [GP](https://patents.google.com/patent/US9979369B2/en) | In force (checked by Chris Paro, 2026-11-12) |
| 4 | US9225310B1, Audio limiter system and method | iZotope Inc (current per Google: Native Instruments USA Inc) | 2012-11-08 / 2013-11-08 | 2034-06-27 (Google) | 4, 6 (limiter attack/release shaping; psychoacoustic limit) | [GP](https://patents.google.com/patent/US9225310B1/en) | In force (checked by Chris Paro, 2026-11-12) |
| 5 | US9661438B1, Low latency limiter | Amazon Technologies Inc | 2015-03-26 / 2015-03-26 | 2035-03-26 (Google; page also prints "Expired - Fee Related") | 4, 6 (delayed audio, gain lowered gradually, linear interpolation of gain curves) | [GP](https://patents.google.com/patent/US9661438B1/en) | Lapsed (checked by Chris Paro, 2026-11-12) |
| 6 | US8983092B2, Waveform shaping system to prevent electrical and mechanical saturation in loud speakers | Conexant Systems LLC (current per Google: Synaptics Inc / Lakestar Semi Inc) | 2010-07-15 / 2011-07-14 | 2033-10-17 (Google adjusted) | 4, 6, 10 (look-ahead buffer, gain envelope, peak reduction) | [GP](https://patents.google.com/patent/US8983092B2/en) | In force (checked by Chris Paro, 2026-11-12) |
| 7 | US8315398B2, System for adjusting perceived loudness of audio signals | DTS Inc (original DTS LLC) | 2007-12-21 / 2008-12-19 | 2030-11-01 (Google adjusted) | 6 (look-ahead line index sets decay; smoothed gain) | [GP](https://patents.google.com/patent/US8315398B2/en) | In force (checked by Chris Paro, 2026-11-12) |
| 8 | US9264836B2, System for adjusting perceived loudness of audio signals | DTS Inc (original DTS LLC) | 2007-12-21 / 2012-06-18 | 2030-06-30 (Google) | 6 (as row 7; same family) | [GP](https://patents.google.com/patent/US9264836B2/en) | In force (checked by Chris Paro, 2026-11-12) |
| 9 | US9667214B2, Dynamic compressor with "release" feature | Institut fuer Rundfunktechnik | 2013-04-11 / 2014-04-10 | 2034-04-10 (Google; page prints "Expired - Fee Related", lapsed 2025-05-30) | 3, 4 (two envelope detectors, one with release, combined to drive gain) | [GP](https://patents.google.com/patent/US9667214B2/en) | Lapsed (checked by Chris Paro, 2026-11-12) |
| 10 | EP3588776B1, Audio signal dynamic range compression | Guoguang Electric Co Ltd | 2018-06-29 / 2018-11-14 | 2038-11-14 (Google; page prints "Revoked") | 3, 4 (level detector with separate attack and release poles) | [GP](https://patents.google.com/patent/EP3588776B1/en) | Revoked (checked by Chris Paro, 2026-11-12) |

Status values: **In force**, **Lapsed (fees)**, **Expired (term)**, **Abandoned**, **Invalidated**,
**Not found**. Record who checked and when, under the table. Statuses checked by Chris Paro on
2026-11-12 (see the Status column): rows 1-4 and 6-8 in force, rows 5 and 9 lapsed, row 10 revoked.

Expired by term (filing date + 20 years, plus any printed term adjustment, is in the past), listed
for completeness and needing no check:

- [US5737434A](https://patents.google.com/patent/US5737434A/en), Orban, filed 1996-08-26 (computed
  expiry 2016-08-26). Multi-band compressor with look-ahead clipper (elements 4, 6, 10).
- [US6535846B1](https://patents.google.com/patent/US6535846B1/en), K S Waves, expiry 2021-02-12 per
  Google. Compressor-limiter and expander with look-ahead (elements 4, 6).
- [US6757396B1](https://patents.google.com/patent/US6757396B1/en), Texas Instruments, filed
  1999-09-27 (computed expiry 2019-09-27). Stereo RMS compressor, lower gain applied to both
  channels (elements 2, 6).
- [US7013011B1](https://patents.google.com/patent/US7013011B1/en), Plantronics, expiry 2024-04-29
  per Google. Limiter with smoothed attenuation on a delayed signal (elements 4, 6).
- [US7532730B2](https://patents.google.com/patent/US7532730B2/en), individual, expiry 2022-04-04
  per Google. Program-dependent release (element 4).
- [US6337999B1](https://patents.google.com/patent/US6337999B1/en), Orban, filed 1998-12-18
  (computed expiry 2018-12-18). Oversampled differential clipper (elements 10, 11).
- [US6937912B1](https://patents.google.com/patent/US6937912B1/en), Orban, expiry 2020-04-05 per
  Google. Anti-aliased clipping with band-limited steps (elements 10, 11).
- [US6504935B1](https://patents.google.com/patent/US6504935B1/en), individual (Jackson), expiry
  2018-08-19 per Google. Modelling and synthesis of harmonic distortion with aliasing suppression
  (elements 5, 7, 11).
- [US4995084A](https://patents.google.com/patent/US4995084A/en), individual (Pritchard), expiry
  2008-02-19 per Google. Tube emulation with even harmonics (element 7).
- [US5636284A](https://patents.google.com/patent/US5636284A/en), individual (Pritchard), expiry
  2014-06-03 per Google. Tube power amplifier emulation (element 7).
- [US5789689A](https://patents.google.com/patent/US5789689A/en), Line 6 (current per Google:
  Yamaha Guitar Group), filed 1997-01-17, expiry 2017-01-17 per Google. Tube-modelling digital
  guitar amplifier (elements 5, 7, 11).
- [US5524060A](https://patents.google.com/patent/US5524060A/en), EuPhonix (current per Google:
  Avid Technology), filed 1994-02-14 (computed expiry 2014-02-14). Transfer characteristic
  interface and display (element 12).

## Claim analysis

Claim text source: each patent's Google Patents page (`https://patents.google.com/patent/<number>/en`),
fetched 2026-11-12. Google serves this as machine-readable text, which I did not OCR. The fetch tool
relays page text through a summarizer and caps each quote at 125 characters, so claims were pulled
in consecutive verbatim pieces; joins between pieces are marked " ... ". For row 4 the tool refused
to give claims 8 and 15 in full and gave a paraphrase, so only the pieces quoted below are verbatim.
A reviewer should check the decisive elements against the granted PDF. Element numbers refer to
Planned design. Rows 5, 9 (lapsed) and 10 (revoked) were not read.

### Row 1: US11394356B1 (Amazon), priority 2021-02-12: **Unclear (weighted-peak mode); Does not read (RMS and waveshaper modes)**

Independent claims 1 (method) and 12 (system, same steps). Claims 2-11 depend on 1, 13-20 on 12.

- "receiving first audio data;": met (element 1 input).
- "determining a first maximum amplitude value associated with a first plurality of audio samples
  within the first audio data," ... "the first plurality of audio samples corresponding to a first
  time window;": met in weighted-peak mode (elements 2, 3, 4: block maximum of absolutes, then the
  maximum of `w * blockmax` over the window). **Not met** in RMS mode (level is a normalised
  weighted root of sums of squares, not a maximum) or waveshaper mode (one sample, not a plurality).
- "determining that the first maximum amplitude value exceeds a threshold value;": **unclear**. The
  design has no comparison of the level with a threshold; element 5 maps every level through
  `tanh((s*x)^c)` unconditionally. Dependent claim 9 sets a below-threshold block's gain "equal to a
  value of one", which shows the claimed threshold is a level above which gain reduction starts.
- "determining a first gain value based on the threshold value and the first maximum amplitude
  value;": **unclear**. The design's gain is `f(x)/x`, set by the level `x`, the steepness `s` and
  exponent `c`. For large `s*x`, `tanh` approaches 1 and the gain approaches `1/x`, which is the
  description's example formula (threshold divided by peak, per the page's description) with a
  threshold of 1.0. Whether `s` (the knee at about `x = 1/s`) or the curve's unit ceiling is "the
  threshold value" decides this element and the one above. The claim gives no formula.
- "determining a first plurality of gain values using the first gain value; and": met (element 6,
  per-sample linear interpolation from the block gain).
- "generating second audio data using the first plurality of gain values and the first plurality of
  audio samples.": met (element 6, gain multiplied into the delayed signal).

Deciding element: "determining that the first maximum amplitude value exceeds a threshold value" and
"based on the threshold value". Literally the design has no threshold test; the doctrine of
equivalents could treat the tanh curve's ceiling as one, since its gain tends to ceiling/peak.
Watch the implementation: a guard such as `if x > eps { f(x)/x } else { s }` is a literal
comparison of the maximum with a threshold, though the gain would not be based on `eps`. Prior art
in this note before 2021-02-12: US5737434A (1996, look-ahead peak limiter), US6535846B1 (1997,
look-ahead compressor-limiter with FIR smoothing), US7013011B1 (2001, smoothed attenuation on a
delayed input), US9661438B1 (2015, same assignee, delayed audio and interpolated gain curves),
Giannoulis et al. 2012 and DAFX 2002 (both unverified).

### Row 2: US11496099B2 (Mimi Hearing Technologies), priority 2020-07-28: **Does not read**

Independent claims 1 (method), 16 (apparatus), 18 (medium). Claims 2-15 depend on 1 (7 via 6, 10
via 9), 17 on 16.

- Claim 1: "A method for determining dynamic gain values to be applied on a digital input signal,"
  ... "the digital input signal arranged in signal blocks, the dynamic gain values for attenuating
  input signal values" ... "exceeding a clipping threshold,": met arguably (element 3 blocks;
  elements 5 and 10 attenuate high levels). "passing backwards over a next signal block and a
  current signal block to produce a preliminary gain contour" ... "from the digital input signal,
  wherein the preliminary gain contour is produced by applying in reverse" ... "an instant gain
  ascent and a smooth gain decay to the digital input signal; and": **not met**. Element 4
  evaluates one level per block as a direct dot product or weighted maximum against a precomputed
  table; nothing is run in reverse time order and no intermediate gain contour is produced. The
  polynomial weight shape `1 - (d/A)^a` cannot be produced by a one-pass recursion either (only an
  exponential weight has that form). "passing forwards over the current signal block to produce a
  final gain contour for the current signal block" ... "based on the preliminary gain contour,
  wherein the final gain contour is produced by applying forwards" ... "the instant gain ascent and
  the smooth gain decay to the preliminary gain contour.": **not met**; the past side of the
  window is applied to block levels in the same evaluation, not to a gain contour.
- Claim 16: "a backwards processor for passing backwards over a next signal block and a current
  signal block to produce" ... "a preliminary gain contour from the digital input signal; and" "a
  forwards processor for passing forwards over the current signal block to produce a final gain
  contour" ... "for the current signal block based on the preliminary gain contour,": **not met**
  (as claim 1). "wherein the backwards processor and the forwards processor produce gain contours
  by applying an instant gain ascent" ... "and a smooth gain decay.": not met.
- Claim 18: same steps as claim 1 ("pass backwards over a next signal block ...", "pass forwards
  over the current signal block ..."): **not met**.

Missing element: a backward pass producing a preliminary gain contour followed by a forward pass
over it. Keep element 4 as a direct evaluation of the weight table; an optimisation into reverse and
forward recursive passes would need reading again. Prior art before 2020-07-28: US5737434A,
US6535846B1, US7013011B1, US9661438B1 (2015).

### Row 3: US9979369B2 (Dolby), priority 2016-10-21: **Does not read**

Independent claim 1 only. Claims 2-4, 7, 9, 10 depend on 1 (9 and 10 claim an apparatus and a
medium "in accordance with claim 1"); 5 on 4, 6 on 5, 8 on 7.

- "A method in an audio peak limiter apparatus, the apparatus being configured to calculate a
  smoothed sequence of gains" ... "for application to a sequence of blocks of samples of an audio
  signal, each of the blocks representing" ... "a respective temporal segment of the audio signal
  not greater than 5 milliseconds in duration,": met (elements 3, 6: one gain per 0.5 to 1 ms
  block, interpolated).
- "determining that applying a current gain of the sequence to a current block of samples would
  produce" ... "at least one scaled sample whose magnitude exceeds a predetermined limit; and":
  **not met**. The design never tests a block gain against the samples it scales; overshoot past
  the ceiling is cut by the per-sample hard clip (element 10).
- "calculating a candidate gain for replacing said current gain of the sequence, such that applying
  the" ... "candidate gain to the current block of samples would produce at least one scaled sample
  whose magnitude" ... "is substantially equal to the predetermined limit and no scaled sample
  whose magnitude exceeds the predetermined limit,": **not met**; no replacement gain.
- "determining that the candidate gain is greater than or equal to the final gain stored in
  connection" ... "with the immediately previous gain of the sequence; and": not met.
- "calculating a current final gain and storing it in connection with the current gain of the
  sequence, the current" ... "final gain being obtainable by dividing the gain of the sequence by a
  prior gain of the sequence and multiplying" ... "the result by the final gain stored in connection
  with said prior gain of the sequence.": **not met**; the design forms each gain as `f(x)/x` from
  the windowed level, with no ratio of earlier gains.

Missing elements: the candidate gain that brings a block's peak to the limit, and the final gain
formed from a ratio of sequence gains. Prior art before 2016-10-21: US5737434A, US6535846B1,
US7013011B1, US9661438B1, Giannoulis et al. 2012 (unverified).

### Row 4: US9225310B1 (iZotope / Native Instruments), priority 2012-11-08: **Does not read**

Independent claims 1 (method), 8 (medium), 15 (limiter). Claims 2-7 depend on 1, 9-14 on 8, 16 on
15.

- Claim 1: "A method performed by a processor for limiting an audio signal, comprising:": met
  (limiter settings). "applying a psychoacoustic model to a portion of the audio signal" ... "to
  produce masking thresholds for a plurality of critical frequency bands": **not met**; the design
  has no psychoacoustic model, no band split and no masking threshold. "generating a plurality of
  different amplitude envelopes for the portion of the audio signal": **not met**; element 4 forms
  one level per block. "for each of the plurality of amplitude envelopes:" "applying the amplitude
  envelope to the audio signal portion to produce a limited audio signal portion;" "producing a
  distortion signal by comparing the audio signal portion to the limited audio signal portion;"
  "calculating distortion signal power for the plurality of critical frequency bands; and"
  "comparing the plurality of distortion signal power for critical frequency bands" "against the
  plurality of masking thresholds to produce a distortion score; and": not met. "selecting one of
  the limited audio signal portions based its corresponding distortion score." (sic): not met.
- Claim 8: preamble "A non-transitory computer-readable storage medium having instructions, that
  when executed by a processor,"; the rest was given only as a paraphrase stating it mirrors claim
  1. Not met on the same elements, subject to checking the text.
- Claim 15: "An audio limiter, comprising:" "a psychoacoustic model component, configured to analyze
  a portion of an audio signal, and produce masking thresholds": **not met**. The remaining
  components (envelope generation, distortion scoring, selection) were given only as a paraphrase.

Missing element: a psychoacoustic model producing masking thresholds, and selection among several
trial envelopes by distortion score. Prior art before 2012-11-08: US5737434A, US6535846B1,
US7013011B1.

### Row 6: US8983092B2 (Conexant / Synaptics), priority 2010-07-15: **Does not read**

Independent claims 1, 10 (systems), 15 (method). Claims 2-9 depend on 1 (6 via 5), 11-14 on 10,
16-20 on 15 (17 via 16).

- Claim 1: "an audio driver including:": met arguably. "a displacement model operable to convert a
  digital audio signal into a displacement signal;": **not met**; the design has no loudspeaker
  model. "a look-ahead buffer operable to hold a plurality of samples of the displacement signal;":
  not met (element 4's lookahead holds audio, not displacement). "an analysis module operable to
  determine whether a sample of the plurality of samples in the look-ahead buffer has a value which
  exceeds a threshold value": not met as claimed (no displacement samples; no threshold test). "a
  model inverse operable to convert the displacement signal back into the digital audio signal.":
  **not met**.
- Claim 10: "a root-mean-square estimation (RMS) module;" "a dynamic range compressor (DRC) applying
  a gain to an audio signal" "the gain of the DRC is determined as a function of an RMS estimation
  from the RMS module and a RMS power threshold;": met arguably in RMS mode (elements 4, 5).
  "a displacement model operable to convert the audio signal output by the DRC into a displacement
  signal;", the look-ahead buffer of displacement samples and "a model inverse operable to convert
  the displacement signal back into the audio signal.": **not met**.
- Claim 15: "converting a digital audio signal into a displacement signal using a displacement
  model:": **not met**. "storing a plurality of samples of the displacement signal in a look-ahead
  buffer;", "if the sample exceeds the given threshold, applying a gain envelope function to the
  plurality of samples" "where the gain envelope function is selected on the basis of the value of
  the plurality of samples", "converting the displacement signal back into the digital audio signal
  using a model inverse.": not met.

Missing element: the displacement model and its inverse, with look-ahead analysis in the
displacement domain. Prior art before 2010-07-15: US5737434A, US6535846B1, US7013011B1, DAFX 2002
(unverified).

### Row 7: US8315398B2 (DTS), priority 2007-12-21: **Does not read**

Independent claim 1 only. Claims 2-7 depend on 1 (6 via 5).

- "A method of adjusting a loudness of an audio signal from a multi-media device": met arguably
  (gain change). "receiving an electronic audio signal comprising two or more channels of audio;":
  met (stereo). "selecting at least one of the channels of audio": met arguably. "determining a
  dominant channel of the two or more audio channels and selecting the dominant channel": **not
  met**; element 2 linked mode combines both channels (mean of squares, or per-sample maximum of
  absolutes) and unlinked mode keeps both; no channel is chosen as dominant for the analysis.
  "computing an energy value from each channel": met in linked RMS mode. "estimating a loudness of
  the dominant channel with one or more processors to produce an estimated loudness": not met.
  "processing the dominant channel with a plurality of approximation filters": **not met**; no
  filter bank. "weighting outputs of the approximation filters by an inverted loudness weighting
  curve": **not met**. "computing at least one gain based at least in part on the estimated
  loudness", "calculating a gain for a portion of the audio signal based at least partly on the
  estimated loudness": met arguably (gain from level). "smoothing the gain over a plurality of
  samples of the audio signal portion": met (element 6).

Missing element: loudness estimation of a dominant channel through approximation filters weighted
by an inverted loudness curve. Prior art before 2007-12-21: US6757396B1 (1998, stereo RMS, lower
gain on both channels), US5737434A, US6535846B1, US7013011B1.

### Row 8: US9264836B2 (DTS), priority 2007-12-21: **Does not read**

Independent claims 1 (method) and 10 (system). Claims 2-6 depend on 1 (7-9 chain from 6); 11-14
and 18 on 10 (15-17 via 14).

- Claim 1: "receiving an electronic audio signal comprising two or more channels of audio": met.
  "analyzing a phase between a portion of each of the two or more audio channels": **not met**; the
  design never compares channel phase. "comparing the number of samples that have the same phase to
  a threshold number": **not met**. "selecting a channel of the two or more audio channels",
  "determining a dominant channel of the two or more audio channels, and selecting the dominant
  channel": not met (as row 7). "using one or more processors to process the selected channel to
  determine a loudness of a portion of the audio signal", "computing at least one gain based at
  least in part on the determined loudness", "applying the at least one gain to the electronic
  audio signal": met arguably. "bypassing said steps of using, computing, and applying": not met;
  the design has no phase-conditioned bypass.
- Claim 10: "a phase analysis module configured to" "compare the number of samples that have the
  same phase to a threshold number", "enable the samples for loudness processing in response to a
  determination that the samples correspond", "bypass the loudness analysis module and gain control
  module": **not met**. The loudness and gain modules are met arguably.

Missing element: the phase analysis counting same-phase samples against a threshold, which enables
or bypasses loudness processing. Prior art before 2007-12-21: as row 7.

## Search log

All searches 2026-11-11. The search tool returns a prose summary of about 10 results, not a raw
list, so hit counts are approximate. Searches were run without any project detail.

| Database | Query | Date | Hits | Result |
|---|---|---|---|---|
| Web (general) | patent look-ahead limiter delay audio signal gain computed from future samples attack window | 2026-11-11 | 10 | US11394356, US9661438, US11496099, US8983092, DTS family (rows 1, 2, 5-8) |
| Web (general) | patent dynamic range compressor level detector asymmetric weighting window attack release lookahead digital audio | 2026-11-11 | 10 | EP3588776, US9667214, US7532730, US6757396, US6535846, US5444788; no asymmetric-window patent |
| Web (general) | patent tanh transfer function compressor waveshaper audio saturation gain computed hyperbolic tangent | 2026-11-11 | 10 | No patent; code and articles |
| Web (general) | patent asymmetric saturation even harmonics DC blocking filter digital audio distortion tube emulation oversampling | 2026-11-11 | 10 | US4995084, US5133014 (not opened), US5636284, US6504935 |
| Web (general) | patent compressor limiter oversampling upsample clipping downsample aliasing reduction audio dynamics | 2026-11-11 | 10 | US11496099, US9225310, US6337999, US6937912 (via Orban listing); US4459557 not opened |
| Web (general) | patent display of compressor transfer curve and envelope window weights user interface audio dynamics | 2026-11-11 | 10 | No patent; manuals |
| Web (general) | patent soft clipper oversampled audio waveform shaping tanh gain control block-rate envelope interpolated gain per sample | 2026-11-11 | 10 | No patent; FFmpeg asoftclip source |
| Web (general) | patent stereo linked compressor sidechain detector combined left right channels maximum or mean square digital audio limiter | 2026-11-11 | 10 | No patent; manuals and forums |
| Web (general) | Giannoulis Massberg Reiss "Digital Dynamic Range Compressor Design A Tutorial and Analysis" JAES 2012 | 2026-11-11 | 10 | AES entry id 16354 (403 to me); EP3588776 cites related work |
| Web (general) | patent audio limiter lookahead gain smoothing window convolution attack release shaped gain curve exponent | 2026-11-11 | 10 | US7013011, US9979369, US9225310, US10701485 and US10797666 (loudspeaker limiters, not opened) |
| Web (general) | patent digital waveshaping asymmetric positive negative half different transfer curve guitar distortion DC offset high-pass filter oversampling | 2026-11-11 | 10 | US5789689 (via blog), US6504935 |
| Web (general) | Zoelzer DAFX Digital Audio Effects dynamic range control look-ahead limiter 2002 book | 2026-11-11 | 10 | DAFX contents; no primary text |
| Web (general) | patent dynamics processor compressor expander gate transfer function nonlinear curve exponent steepness single unit limiter saturator | 2026-11-11 | 10 | US5524060; no single-curve multi-mode patent |
| Web (general) | patent peak limiter lookahead oversampled true peak sinc interpolation gain reduction smoothing digital audio mastering | 2026-11-11 | 10 | US5737434, US6535846; no true-peak patent |
| Web (general) | patent saturation audio effect soft clipping nonlinear transfer polyphase oversampling downsampling plugin harmonic distortion adjustable curve exponent | 2026-11-11 | 10 | US6337999, WO2000036743A1 (same family) |
| Google Patents (page fetch by number) | US11394356B1, US9661438B1, US11496099B2, US8983092B2, US8315398B2, US9264836B2, US9667214B2, EP3588776B1, US9225310B1, US9979369B2, US7013011B1, US7532730B2, US6757396B1, US6535846B1, US6337999B1, US6937912B1, US6504935B1, US4995084A, US5636284A, US5789689A, US5524060A, US5737434A | 2026-11-11 | 22 | Dates and status read; rows 1-10 and the expired list |
| Google Patents (claims, stage 2) | Claims of US9979369B2, US9225310B1 (first page window); US11394356B1, US11496099B2, US8983092B2, US8315398B2, US9264836B2 (fetch offset 100000) | 2026-11-12 | 7 | Independent claims quoted under Claim analysis; offset 150000 of US8315398B2 and US9264836B2 held only citation tables |
| Google Patents (description, stage 2) | US11394356B1 description (threshold and gain formula) and dependent claims 2-11 | 2026-11-12 | 1 | Threshold compared with block peak; gain "by dividing the threshold value by the maximum amplitude value", not limited thereto |

Refused or failed: Google Patents pages for US11394356B2 and US8229124B2 returned 404 (US11394356B1
worked; the DTS sibling US8229124 was not opened). Stage 2: Justia's page for US9225310 returned
HTTP 403, and the fetch tool declined to give claims 8 and 15 of US9225310B1 verbatim in full. AES
E-Library page returned HTTP 403. Vicanek PDF
gave no readable front matter. The Google Patents, USPTO and Espacenet search interfaces were not
queried (they did not work in the earlier EQ search, see `docs/research/eq-filter-patents.md`), so
no search used CPC classes (for example G10L21/0316, H03G7, H03G9, G10H) or assignee names
(Waves, FabFilter, iZotope, Softube, Universal Audio, Dolby, Apple) directly; every candidate came
through general web search.

## Open uncertainties

- **Weighted-peak mode needs a human decision (tag `patent`).** Row 1 (US11394356B1, Amazon, to
  2041-02-12 computed) may read on peak mode: block maximum over a time window, gain from it,
  per-sample gains interpolated from that gain. It turns on "determining that the first maximum
  amplitude value exceeds a threshold value" and a gain "based on the threshold value": the design
  has no threshold test, but its tanh gain tends to ceiling/peak at high levels, close to the
  description's threshold/peak example. Decide, through `flag-risk`, whether to get counsel's
  reading; rely on the older art (US5737434A, US6535846B1, US7013011B1, all expired; US9661438B1,
  lapsed) against validity; or design around. Design-arounds, each to be read against claim 1
  again: (a) keep the gain formula branch-free, with no comparison of the level against any
  threshold (including a divide-by-zero guard; use a form of `f(x)/x` that is finite at 0); (b)
  derive the gain from the RMS level only, keeping peak detection out of the gain path; (c) drop
  weighted-peak mode. Note (a) leaves the equivalence question open; (b) and (c) remove the
  "maximum amplitude value" element.
- Row 2: the verdict holds while element 4 is a direct evaluation of the weight table. If it is
  later optimised into recursive reverse-then-forward passes (for example, exponential weights
  computed by recursion over blocks), read row 2 again.
- Row 3: the verdict holds while no step replaces a block gain with one that brings that block's
  peak exactly to the ceiling. A "true peak" or "no overshoot" option of that kind would need
  reading again.
- Row 4: claims 8 and 15 were relayed as paraphrase beyond the quoted pieces; claim 1 was quoted
  in full in pieces. The verdict rests on the psychoacoustic-model element, which claim 15 quotes
  verbatim; a reviewer should check claim 8 against the PDF.
- Claim quotes came through a summarizing fetch tool in pieces under 125 characters, not from the
  granted PDFs; the decisive elements (row 1 claim 1 threshold clauses) should be checked against
  the PDF.
- Element 4 (two-sided weighting window with separate attack and release exponents, level as a
  normalised weighted sum or weighted maximum): no prior-art source and no candidate found. This is
  the least-covered element; classification-code and assignee searches would test it.
- Element 5 (`tanh((s*x)^c)`, gain f(x)/x): no patent found, and no dated source shows the `c > 1`
  gating form. Mikelson's article is undated.
- Element 3 (block reduction at about 0.5 to 1 ms with per-sample linear gain interpolation): rows
  2 and 3 do not read and row 1 is Unclear (see above); row 5's claims were not read (lapsed).
- Element 12 (window-weight plot): no source found either way.
- The AES paper, DAFX 2002 text and Vicanek paper were not read from the publisher; dates come from
  search summaries.
- Family members and cited-by lists were not walked (for example US5133014A, US4459557, US10701485B2,
  US10797666B2, DTS US8229124B2, any US counterpart of EP3588776B1).
- Term ends are Google's printed or computed (filing plus 20 years); adjustments and terminal
  disclaimers were not seen. Row 1's end (2041-02-12) is computed and may carry an adjustment.
- Not searched by assignee name: Waves, FabFilter, iZotope, Softube, Universal Audio, Apple, Sonnox.
