# Patent status of the in-house Amplifier effect (drive, bias, waveshaper, makeup, DC high-pass, oversampling)

Research note, 2026-11-12. This is not legal advice. It records what primary sources show, so a
reviewer can check it. Stage 1 listed the candidates; a human filled in **Status**; stage 2
(2026-11-12) read the claims of the two in-force candidates, rows 4 and 6. Dates in the Candidates table are as Google Patents prints them (Google states that its
status and expiry fields are assumptions, not legal conclusions). For issue #60.

## Planned design

Software, portable scalar Rust, an "Amplifier" effect node in an audio editor; per-sample
processing on the real-time audio thread; any number of channels, each processed independently.
Signal path: `x -> drive -> + bias -> shaper f -> makeup -> DC high-pass -> out`.

1. **Pre-amp gain ("drive").** −12 to +36 dB, applied as a linear factor `drive`. A change of the
   setting is ramped smoothly in dB.
2. **DC bias after the pre-amp gain.** `u = drive*x + bias`, `bias` in −1..+1. Purpose: asymmetric
   clipping that produces even harmonics (tube-style asymmetry).
3. **Memoryless static waveshaper `f(u)`**, one of three chosen by the user:
   a. "warm": `(2/pi) * atan((pi/2) * u)`.
   b. "crunched": `tanh(u)`.
   c. "harsh": hard clip, `clamp(u, -1, 1)`.
   On a change of shape the output crossfades linearly from the old shape's output to the new
   shape's over a short ramp time.
4. **Makeup gain.** −36 to +12 dB, ramped in dB on change.
5. **DC-removing high-pass** after the shaper and makeup: fixed 4th-order Butterworth high-pass at
   20 Hz, a cascade of two biquads (RBJ-cookbook style), at the base sample rate.
6. **Optional 2x or 4x oversampling around the shaper only.** Upsample with a polyphase
   Kaiser-windowed-sinc resampler, shape at the high rate, downsample with the same kind of filter.
   The combined filter delay is padded to a whole number of base-rate frames and reported to the
   host as latency. The resampler itself was cleared in
   [resampler-patents.md](resampler-patents.md) /
   [ADR-0036](../adrs/0036-sample-rate-conversion-is-exact-rational-polyphase.md); only its use for
   oversampling a nonlinearity is new here. Gain and bias ramps advance per base frame and hold
   over the oversampled sub-samples.
7. **Display (UI, not audio).** A plot of the static transfer function
   `y = makeup * (f(drive*x + bias) - f(bias))` on linear axes ±1.413 (±3 dBFS): the curve
   recentred by subtracting `f(bias)` so it passes through the origin, matching what the DC
   high-pass does at silence.

Not in scope: tone stack, cabinet or speaker simulation, any physical circuit model.

## Summary

Six candidates had a printed term end in the future; a human found rows 4 (US8249275B1) and 6
(US12177646B2) in force and rows 1, 2, 3, 5 not in force (see Candidates). Stage 2 verdicts:
row 4 **Does not read**, row 6 **Does not read** (see Claim analysis). No candidate was found for
elements 3b, 3c, 5, 6 or 7 in the searches in the Search log; this rests on web searches only,
because no patent database query interface was used (see "Refused or failed").

| Technique | Verdict | Confidence |
|---|---|---|
| 1. Drive gain ramped in dB | Does not read: row 4 (no switching among gain settings averaged to the target), row 6 (no decoded bitstream or frame-level metadata gains). Row 5 not in force. Older art: US5289546A (expired). | Medium |
| 2. DC bias before the shaper, asymmetric clipping | Row 1 (asymmetric soft clipping, threshold comparison). Older art, expired: US4995084A, US5032796A. | Low |
| 3. Memoryless waveshaper (atan, tanh, hard clip) and linear crossfade between shapes | Rows 1, 3 touch soft clipping; no candidate found for the three formulas or the crossfade in searches 3, 7, 12, 13. | Low |
| 4. Makeup gain, ramped in dB | Does not read, rows 4 and 6, as for element 1 (same `Ramp`). | Medium |
| 5. 4th-order Butterworth DC high-pass (two RBJ biquads) | No candidate found in searches 8, 9 (DC removal after a nonlinearity). | Medium |
| 6. 2x/4x oversampling around a shaper, padded latency reported | No in-term candidate found in searches 1, 5, 11, 14, 16, 21; the oversampling-around-a-nonlinearity art is from 1997 to 1998 and expired (US5789689, US5929795A, US6337999B1, US6504935B1). No patent found on host latency reporting. | Medium |
| 7. Transfer-curve display recentred by `f(bias)` | No candidate found; no search targeted a display of a transfer curve. | Low |

## Prior art

Items marked (unverified) come from a search summary or an index, not from the publication itself.
The PDFs of Kahles et al., Parker et al. (DAFx-16), Pakarinen and Yeh, and the Maynooth saturation
paper could not be read as text by the fetch tool, so their contents are not established here.

- **Element 6 (oversample, shape, filter, decimate).**
  - US5789689A (Line 6 / Doidic et al.), priority and filing 1997-01-17: "linear interpolation is
    used with M=8 in its oversampling", the nonlinear function applied at the high rate, a
    symmetrical FIR with a sin(x)/x design and Hanning window, then "1 out of every 8 samples is
    selected" ([Google Patents](https://patents.google.com/patent/US5789689)). Its transfer
    functions include a hard-clipping curve (for fuzz) and a piecewise curve (element 3c).
  - US5929795A (Lucent), priority 1997-06-19: raises the sampling rate before a non-linear stage,
    low-pass filters, converts back down ([Google Patents](https://patents.google.com/patent/US5929795/en)).
  - US6337999B1 (Orban), priority 1998-12-18: clipper at a high oversampling rate; the clipper
    output is subtracted from its input, downsampled and added to a delayed unclipped copy
    ([Google Patents](https://patents.google.com/patent/US6337999B1/en)); a search summary says
    its description pads filter delay to a whole number of samples (unverified).
  - US6504935B1 (Jackson), priority 1998-08-19: up-converts the sample rate before a non-linear
    transfer function, down-converts afterward to suppress aliasing; also a DC-removal method
    ([Google Patents](https://patents.google.com/patent/US6504935B1/en); description via fetch
    summary, page truncated at 100000 characters).
  - Kahles, Esqueda, Valimaki, "Oversampling for Nonlinear Waveshaping: Choosing the Right Filters",
    JAES 67(6), 440-449, 2019-06-09, DOI 10.17743/jaes.2019.0012
    ([Crossref](https://api.crossref.org/works/10.17743/jaes.2019.0012)); content not read.
  - Parker, Zavalishin, Le Bivic, "Reducing the Aliasing of Nonlinear Waveshaping Using
    Continuous-Time Convolution", DAFx-16, Brno, 2016; the DAFx record says aliasing is
    traditionally addressed by oversampling
    ([DAFx archive](https://dafx.de/paper-archive/details/vem_XXF5qBbfiWOH2RVVAA)).
  - Latency reporting to a host: no dated primary source found. Search summaries say DAWs
    compensate for plugin latency when the plugin reports it
    ([MATLAB Answers thread](https://www.mathworks.com/matlabcentral/answers/507553-audio-plugin-oversampling-delay-behaviour),
    undated, not relied on).
- **Element 3 (memoryless waveshaper).**
  - US5789689A above: hard-clipping curve, tube-like curve, piecewise curve (1997).
  - US4868869A (Yield Securities / Clarity), priority 1988-01-07: each sample addresses a lookup
    table applying a nonlinear transformation; the fetch summary lists "multiple tables with
    crossfading" among optional features
    ([Google Patents](https://patents.google.com/patent/US4868869)). Whether the crossfade is
    linear or occurs when the table is switched is not established; I did not read the text.
  - US5841875A (Yamaha), priority 1991-10-30: input/output conversion table that adds harmonics to
    emulate vacuum-tube amplifiers ([Google Patents](https://patents.google.com/patent/US5841875A/en)).
  - Pakarinen and Yeh, "A Review of Digital Techniques for Modeling Vacuum-Tube Guitar
    Amplifiers", Computer Music Journal 33(2), 85-100, June 2009
    ([Crossref](https://api.crossref.org/works/10.1162/comj.2009.33.2.85)); PDF unreadable, so
    which waveshaper formulas it gives is not established.
  - The specific forms `(2/pi)*atan((pi/2)*u)` and `tanh(u)`: no dated source opened. Audio Weaver
    documents an arctangent shaper normalised as arctan(k*x)/arctan(k)
    ([DSP Concepts](https://documentation.dspconcepts.com/awe-designer/8.2.D.4/atan-wave-shaper)),
    undated, not relied on. The Maynooth paper
    ([PDF](https://mural.maynoothuniversity.ie/id/eprint/4099/1/EAApaper-JT-30-03.pdf)) and a
    Leeds paper on soft clipping
    ([PDF](https://eprints.whiterose.ac.uk/208893/1/Psychoacoustics%20of%20Soft%20Clipping.pdf))
    were reported by a search summary to cover tanh and atan; not read, undated here.
- **Element 2 (bias for asymmetric clipping and even harmonics).**
  - US4995084A (Pritchard), priority 1987-03-23: abstract names "an asymmetrical bias-shifting
    circuit with a limiter" generating even harmonics, in an analog or digital processor
    ([Google Patents](https://patents.google.com/patent/US4995084)); the digital form is from a
    search summary of the description (unverified).
  - US5032796A (St. Louis Music), priority 1989-12-19: a solid-state amplifier whose output is
    clipped asymmetrically ([Google Patents](https://patents.google.com/patent/US5032796A/)).
  - US8271109B2 (Gallo), priority 2007-03-06: asymmetrical soft-clipping transfer functions in a
    digital processor (figure descriptions per a search summary; see Candidates row 1).
  - Pakarinen and Yeh 2009 above (bias shifting; summary only).
- **Element 1, 4 (smooth gain changes).**
  - US5289546A (IBM), priority 1991-10-03: "smooth audio scaling" with logarithmic multiplier
    values ([Google Patents](https://patents.google.com/patent/US5289546A)); title and dates read,
    claims not.
  - Smoothing in the dB domain with linear gain output is also what Sony US5923767A does for a
    dynamics processor (see the dynamics note, [dynamics-patents.md](dynamics-patents.md)).
- **Element 5 (Butterworth, biquad cascade, RBJ form, DC blocking).**
  - S. Butterworth, "On the Theory of Filter Amplifiers", Experimental Wireless and the Wireless
    Engineer 7, 536-541, October 1930 (bibliographic details from
    [BibBase](https://bibbase.org/network/publication/butterworth-onthetheoryoffilteramplifiers-1930);
    scan at [worldradiohistory](https://worldradiohistory.com/UK/Experimental-Wireless/30s/Wireless-Engineer-1930-10.pdf),
    not opened). A search summary says it works through two-pole and four-pole filters and
    cascading them.
  - RBJ Audio EQ Cookbook: the W3C Working Group Note of 2021-06-08 adapts it and prints the
    high-pass biquad (b0 = (1 + cos w0)/2, b1 = -(1 + cos w0), a1 = -2 cos w0)
    ([W3C](https://www.w3.org/TR/audio-eq-cookbook/)). The musicdsp.org entry is dated
    2005-05-04 ([musicdsp](https://musicdsp.readthedocs.io/en/latest/Filters/197-rbj-audio-eq-cookbook.html));
    copies elsewhere cite a music-dsp mailing-list post of March 2001 (unverified, not opened).
  - A high-pass after an asymmetric shaper to recentre the signal: Source Audio Ultrawave manual
    (undated, via search summary, not relied on). US6504935B1 above has DC removal after distortion.
- **Element 3 crossfade on shape change, element 7 display.** No dated source found beyond
  US4868869A. Open uncertainty.

## Candidates

Patents that might cover an element of the design and might still be in force. Status is filled
in by a human from Google Patents or USPTO Patent Center.

| # | Patent | Assignee | Priority / filing | Est. term end | Element it may touch | Link | Status |
|---|---|---|---|---|---|---|---|
| 1 | US8271109B2, Method and apparatus for distortion of audio signals and emulation of vacuum tube amplifiers | Marc Nicholas Gallo (individual) | 2007-03-06 / 2007-03-06 | 2031-07-19 (Google adjusted; page prints "Expired - Fee Related") | 2, 3 (digital distortion processor; claim 1 recites a comparison of the input sample with a threshold and a linear function multiplying by a gain coefficient; description has symmetric and asymmetric soft clipping; claims not read in full) | [GP](https://patents.google.com/patent/US8271109) || Not in force (Google: Expired - Fee Related) (Chris Paro, 2026-11-12) |
| 2 | US8275477B2, same title | Marc Nicholas Gallo (individual) | 2009-08-10 / 2009-08-10 | 2030-09-25 (Google adjusted; page prints "Expired - Fee Related", maintenance-fee lapse effective 2024-09-25) | None of 1 to 7 by its one independent claim (implicit numerical integration solver for a tube emulator); listed because it shares the family and the tube-emulation purpose | [GP](https://patents.google.com/patent/US8275477B2/en) || Not in force (Google: fee lapse 2024-09-25) (Chris Paro, 2026-11-12) |
| 3 | US8645144B2, Audio signal shaping for playback by audio devices | Broadcom Corp (current per Google: Avago Technologies International Sales Pte Ltd) | 2008-04-24 / 2008-04-24 | 2031-11-25 (Google adjusted; page prints "Expired - Fee Related") | 3 (claim 1: compress to a compression limit, then soft clip to a lower soft-clipping limit; claim text seen only in part) | [GP](https://patents.google.com/patent/US8645144B2/en) || Not in force (Google: Expired - Fee Related) (Chris Paro, 2026-11-12) |
| 4 | US8249275B1, Modulated gain audio control and zipper noise suppression techniques using modulated gain | Cirrus Logic Inc | 2009-06-26 / 2009-06-26 | 2031-04-20 (Google adjusted; page prints "Active") | 1, 4 (gain changes made gradual by switching among gain values for set durations, PWM or delta-sigma; the design ramps gain in dB instead) | [GP](https://patents.google.com/patent/US8249275B1/en) || **In force** (Chris Paro, 2026-11-12) |
| 5 | US8258870B2, System and method for digital control of amplifier gain with reduced zipper noise | THAT Corp (current per Google: Middlesex Savings Bank) | 2009-08-14 / 2010-08-16 | 2030-08-16 (Google; page prints "Expired - Fee Related", maintenance-fee lapse effective 2024-09-04) | 1, 4 (gain increments clocked at non-uniform intervals; a digitally controlled analog gain circuit) | [GP](https://patents.google.com/patent/US8258870B2/en) || Not in force (Google: fee lapse 2024-09-04) (Chris Paro, 2026-11-12) |
| 6 | US12177646B2, Main-associated audio experience with efficient ducking gain application | Dolby International AB | 2020-05-26 / 2021-05-20 | 2041-12-04 (Google adjusted; page prints "Active") | 1, 4 (sub-frame gain ramps of a determined length between frame-level gains; claims read in stage 2, see Claim analysis) | [GP](https://patents.google.com/patent/US12177646B2/en) || **In force** (Chris Paro, 2026-11-12) |

Status values: **In force**, **Lapsed (fees)**, **Expired (term)**, **Abandoned**, **Invalidated**,
**Not found**. Record who checked and when, under the table.

Rows 1, 2, 3 and 5 are listed because their printed term end is in the future; Google prints a
fee-related expiry for them, which a human should confirm.

Expired by term (filing date + 20 years, plus any printed term adjustment, is in the past), listed
for completeness and needing no check:

- [US5789689A](https://patents.google.com/patent/US5789689), Line 6 (Yamaha Guitar Group now), filed
  1997-01-17, expiry 2017-01-17 per Google. Oversampling at M=8 before a nonlinear function,
  hard-clip and piecewise curves (elements 3, 6).
- [US5929795A](https://patents.google.com/patent/US5929795/en), Lucent, filed 1997-06-19, expiry
  2017-06-19 per Google. Upsample before a non-linear stage, low-pass, downsample (element 6).
- [US6337999B1](https://patents.google.com/patent/US6337999B1/en), Orban, filed 1998-12-18, expiry
  2018-12-18 per Google (also lapsed for fees 2014-01-08). Oversampled clipper (elements 3c, 6).
- [US6504935B1](https://patents.google.com/patent/US6504935B1/en), Jackson, filed 1998-08-19,
  expiry 2018-08-19 per Google. Oversampled distortion synthesis with DC removal (elements 3, 5, 6).
- [US4995084A](https://patents.google.com/patent/US4995084), Pritchard, filed 1988-11-01, expiry
  2008-02-19 per Google. Asymmetrical bias shifting for even harmonics (element 2).
- [US5636284](https://patents.google.com/patent/US5636284), Pritchard, filed 1994-02-25, expiry
  2014-06-03 per Google. Tube power-amplifier emulation including bias shifting (element 2).
- [US5032796A](https://patents.google.com/patent/US5032796A/), St. Louis Music, filed 1989-12-19,
  expiry 2009-12-19 per Google. Asymmetric clipping (element 2).
- [US5841875A](https://patents.google.com/patent/US5841875A/en), Yamaha, filed 1996-01-18, expiry
  2012-10-29 per Google. Table-based harmonics modification (element 3).
- [US4868869A](https://patents.google.com/patent/US4868869), Yield Securities, filed 1988-01-07,
  expiry 2008-01-07 per Google. Table-based waveshaping, multiple tables with crossfading
  (element 3).
- [US4991218A](https://patents.google.com/patent/US4991218), Yield Securities, filed 1989-08-24,
  expiry 2008-02-05 per Google. Continuation of US4868869A (element 3).
- [US5289546A](https://patents.google.com/patent/US5289546A), IBM, filed 1993-02-01, priority
  1991-10-03, expiry 2011-10-03 per Google. Smooth audio scaling (elements 1, 4).
- [EP1234302A1](https://patents.google.com/patent/EP1234302A1/en), STMicroelectronics Asia Pacific,
  filed 1999-11-24, expiry 2019-11-24 per Google. Aliasing in delay-modulating effects, not in
  waveshaping (listed because it surfaced for aliasing in effects; touches no element).

Seen and not listed (the claims or abstract name no element of the design):

- US9633812B1 (Senisi), term end 2036-09-30 as printed: analog triode emulator circuit, claim 1
  "An analog triode tube emulator circuit"; a physical circuit model, out of scope.
- GB2493382A (Blackstar): withdrawn; filters a signal before a solid-state power amplifier.
- WO2019222789A1 (Tsambos): printed status "Ceased", anticipated expiry 2020-11-23; tube cathode
  current estimation, a circuit model.
- IT202100000065A1 and IT202200024429A1 (Tone Spring): analog long-tailed-pair distortion and
  a floating-bridge amplifier; physical circuits.
- US9936282B2 (Cirrus Logic): oversampling in a DAC playback path; not read, nonlinearity not
  involved by its title.

## Claim analysis

Stage 2, 2026-11-12, for the rows a human marked **In force** (4 and 6). Claim text comes from the
Google Patents HTML full text, not OCR. The fetch tool returns text through a summarising model
and limits each quote to 125 characters, so every independent claim was obtained as consecutive
verbatim fragments of at most 120 characters each, joined here with no gaps. The granted PDFs were
not opened (see Open uncertainties).

Design facts used for elements 1 and 4 (from the brief for this stage, describing signal_graph's
`Ramp`): a parameter changes at most once per processing block; the value in dB then moves
linearly, one step per sample, to the new target over a fixed ramp time (tens of ms) and holds
there; each sample the dB value is converted to one linear gain and multiplied in. The design has
no set of discrete gain values that it switches among, no PWM or delta-sigma modulation of gain,
no decoded bitstream or frame-level metadata gains, no audio objects, and no ducking of a main
signal by an associated one.

### Row 4: US8249275B1 (Cirrus Logic), priority 2009-06-26

Independent claims: 1, 12, 23, 26 (27 claims; every other claim depends on one of these, per the
Google Patents claim list).

**Claim 1** (method).

| Element | Planned design |
|---|---|
| "A method of controlling amplitude of a signal, comprising:" | Met: drive and makeup (elements 1, 4) scale the signal's amplitude. |
| "receiving a gain control value to apply to the signal to generate a resultant output signal; and" | Met: the drive or makeup setting in dB, from the user. |
| "modulating a selectable gain of a gain stage by selecting between multiple gain settings according to durations corresponding to the multiple gain settings to produce a resulting average gain equal to the gain control value," | Not met. The design applies one gain per sample, taken from a monotonic ramp that ends at the target and holds it. It does not alternate among a set of gain settings, and the target is reached by applying it, not as a duration-weighted average of other settings. During a ramp the per-sample gains average to roughly the midpoint, not to the gain control value. |
| "wherein at least one of the durations and the multiple gain settings are determined from the gain control value." | Not met, as there are no such durations or settings. A ramp's step size is derived from the target, but it does not set durations of gain settings that average to that target. |

**Claim 12** (circuit).

| Element | Planned design |
|---|---|
| "A circuit for controlling the amplitude of a signal, the circuit comprising:" | Software on a general-purpose CPU; not a circuit as such. Treated as met for the analysis below, which does not turn on it. |
| "a gain control block for applying a selectable gain to the signal according to a selected gain setting; and" | Arguably met: the multiply by the per-sample linear gain. |
| "a control circuit for receiving a gain control value and setting the selected gain setting of the gain control block," | Arguably met: the `Ramp` takes the target and sets each sample's gain. |
| "wherein the control circuit includes a modulator for modulating the selected gain setting between multiple gain settings for corresponding durations, wherein at least one of the durations and the multiple gain settings are determined from the gain control value," | Not met: no modulator, and no switching between gain settings. |
| "whereby a resulting gain is applied to the signal equal to an average of the multiple gain settings weighted according to their corresponding durations." | Not met: the gain applied once the ramp has settled is the target itself, not a weighted average of other settings. |

**Claim 23** (digital signal processor) and **claim 26** (computer program product). Their bodies
are word for word the same; claim 23's preamble is "A digital signal processor comprising a
processor core for executing program instructions, a memory for storing the program instructions,
and wherein the program instructions comprise program instructions for controlling the amplitude
of a signal, the program instruction comprising program instructions for:", and claim 26's is "A
non-transitory computer program product comprising a computer-readable storage medium storing
program instructions for execution by a digital signal processor, the program instruction
comprising program instructions for:".

| Element | Planned design |
|---|---|
| Preamble (above) | Claim 23: whether a general-purpose CPU running the editor is "a digital signal processor" is not decided here; it does not decide the verdict. Claim 26: met if the design ships as a stored program. |
| "receiving a gain control value to apply to the signal to generate a resultant output signal;" | Met: the dB setting. |
| "determining durations for multiple selectable gain settings of a gain that is applied to the signal to generate the resultant output signal;" | Not met: the design determines no durations for gain settings. The fixed ramp time is the time to reach one target, not a duration assigned to each of several settings. |
| "selecting from among the multiple selectable gain settings and applying the selected gain setting to the signal; and" | Not met: there is no set of selectable gain settings; each sample's gain is computed from the ramp. |
| "modulating the selection of the gain setting while repeating the selecting, wherein the selection is modulated among the multiple gain settings values according to corresponding durations to produce a resulting average gain equal to the gain control value." | Not met: no modulation, and no averaging to the control value. |

**Verdict: Does not read.** Every independent claim requires modulating (switching) among
multiple gain settings for durations so that their weighted average equals the gain control
value (claim 1 third element, claim 12 modulator element, claims 23 and 26 last element); the
design has no such element. Prior art in this note that predates its 2009-06-26 priority:
US5289546A (IBM, priority 1991-10-03, smooth audio scaling with logarithmic multiplier values;
claims not read) and Sony US5923767A, smoothing in dB with linear gain output (see
[dynamics-patents.md](dynamics-patents.md)).

### Row 6: US12177646B2 (Dolby International), priority 2020-05-26

Independent claims: 1 only. Claims 2 to 17 depend on claim 1 (claim 6 through claim 2, claim 16
through claim 15), claim 18 depends on claim 17, and claims 19 (apparatus) and 20 (storage medium)
perform "the method recited in claim 1", so they need every element of claim 1. Claim 17's text
was seen only in part; its opening "The method as recited in claim 1" was seen.

**Claim 1** (method; preamble "A method comprising:").

| Element | Planned design |
|---|---|
| "decoding an audio bitstream into a set of one or more audio objects and audio metadata for the set of audio objects," | Not met: the Amplifier processes sample buffers in an editor's signal graph; it decodes no bitstream into audio objects and metadata. |
| "the set of one or more audio objects including a specific audio object, the audio metadata specifying a first set of frame-level gains that include a first gain and a second gain respectively for a first audio frame and a second audio frame in the audio bitstream;" | Not met: drive and makeup come from user parameters, not from metadata carrying per-frame gains of a bitstream. |
| "determining, based at least in part on the first and second gains for the first and second audio frames, whether sub-frame gains are to be generated for the specific audio object;" | Not met: the `Ramp` runs on every change of the setting; there is no decision, based on two frame-level gains, whether to generate sub-frame gains. If a block were called a frame and the old and new settings the two gains, a ramp starting only when the value changes could be argued to be such a decision; that argument still fails on the first two elements. |
| "in response to determining ... that sub-frame gains are to be generated for the specific audio object: determining a ramp length for a ramp used to generate the sub-frame gains for the specific audio object;" | Partly analogous: the design's ramp time is fixed, not determined per change. Not met as recited, since there is no audio object. |
| "using the ramp of the ramp length to generate a second set of gains, wherein the second set of gains includes the sub-frame gains for the specific audio object; and" | Per-sample ramped gains exist in the design, but not "for the specific audio object". |
| "causing a sound field represented by the set of audio objects, to which the second set of gains is applied, to be rendered by a set of audio speakers operating in a specific playback environment." | Not met: no object-based sound field and no rendering to a speaker layout; the effect writes processed samples to the graph. |

**Verdict: Does not read.** Missing elements: decoding an audio bitstream into audio objects
and metadata, and frame-level gains specified by that metadata (first two elements of claim 1);
also the rendering of an object-based sound field to a speaker set (last element). Prior art in
this note that predates its 2020-05-26 priority, for ramped gain changes in general: US5289546A
(1991), US8249275B1 (2009; its dependent claims mention transitions between gain control values
with linear or non-linear profiles, from a fetch summary), US8258870B2 (2009).

## Search log

All searches 2026-11-12. The search tool returns a prose summary, not raw result lists, so hit
counts are approximate (about 10 per query unless noted).

| Database | Query | Date | Hits | Result |
|---|---|---|---|---|
| Web (general) | 1. patent oversampling nonlinear waveshaper distortion audio effect aliasing upsample downsample polyphase | 2026-11-12 | 10 | US5789689 lead; papers; W3C list threads |
| Web (general) | 2. patent guitar amplifier simulation waveshaper DC bias asymmetric clipping even harmonics digital | 2026-11-12 | 10 | US4995084, US5133014, EP0307465 |
| Web (general) | 3. patent soft clipping arctangent hyperbolic tangent waveshaping distortion effect digital audio | 2026-11-12 | 10 | US8271109; papers |
| Web (general) | 4. Parker Esqueda Bilbao "Antiderivative antialiasing" waveshaping DAFx 2016 patent | 2026-11-12 | 10 | No patent; papers |
| Web (general) | 5. patent "oversampling" "non-linear" audio distortion effect plug-in upsampling filter latency compensation guitar amp modeling US patent granted | 2026-11-12 | 10 | US5929795, US6337999, US9936282 |
| Web (general) | 6. patent reducing aliasing nonlinear audio processing waveshaper antiderivative OR "continuous-time convolution" Native Instruments | 2026-11-12 | 10 | No patent; DAFx papers |
| Web (general) | 7. patent distortion effect crossfade between two nonlinear transfer functions when switching waveshaper type audio | 2026-11-12 | 10 | No patent on crossfade; US6504935 surfaced |
| Web (general) | 8. patent digital distortion effect bias offset added before nonlinear function then DC blocking high-pass filter output asymmetric transfer function | 2026-11-12 | 10 | DC-offset patents for receivers, not audio effects; none read |
| Web (general) | 9. patent distortion pedal modeling digital "asymmetric" "even harmonics" "DC offset" input signal added nonlinearity then "high-pass filter" remove DC audio effect processor | 2026-11-12 | 10 | US6504935, US8275477; manuals |
| Web (general, extended) | 11. patent oversampling nonlinear processing audio effect upsampler nonlinear function downsampler latency reported to host plugin delay compensation oversampled saturation | 2026-11-12 | 10 | US6337999; no patent on host latency reporting |
| Web (general) | 12. patent "waveshaper" "hard clipping" "soft clipping" selectable distortion types drive gain level digital audio effect processor claim | 2026-11-12 | 10 | No patent; software and manuals |
| Web (general) | 13. patent audio signal "soft clipper" "hyperbolic tangent" OR "arctangent" OR "atan" nonlinear transfer function gain before clipping makeup gain after limiting loudspeaker | 2026-11-12 | 10 | US4877981 (Ampex, analog soft-clip circuit, not opened); papers |
| Web (general, extended) | 14. patent anti-aliasing nonlinear audio effect "oversampling" saturation distortion plugin granted claims "upsampling" "waveshaping" 2018..2025 | 2026-11-12 | 10 | US9936282, EP1234302 |
| Web (general) | 15. patent audio gain change smoothing ramp in decibels zipper noise ... | 2026-11-12 | 10 | US5289546, US8258870, US8249275, US12177646 |
| Web (general) | 16. Roland OR Boss OR Yamaha OR Korg patent distortion effect digital "oversampling" nonlinear "waveshaper" aliasing foldover | 2026-11-12 | 10 | No manufacturer patent |
| Web (general) | 17. Line 6 OR Yamaha Guitar Group patent amplifier modeling "bias" waveshaper ... | 2026-11-12 | 10 | US5789689, US5841875 |
| Web (general) | 18. Fractal Audio Systems patent amplifier modeling distortion nonlinear bias oversampling Cliff Chase | 2026-11-12 | 10 | No patent number; interviews, forum |
| Web (general, extended) | 19. patent "vacuum tube" emulation digital waveshaper "bias" grid conduction asymmetrical clipping ... (vendor names) | 2026-11-12 | 10 | US5032796, US5636284, US20080218259A1 |
| Web (general) | 20. Julian Parker Native Instruments patent waveshaper aliasing OR "wave shaping" US patent application | 2026-11-12 | 10 | No patent; papers |
| Web (general) | 21. patent digital audio effect "static nonlinearity" OR "waveshaping" "oversampled" Kaiser polyphase ... Soundtoys OR Waves OR FabFilter OR "Plugin Alliance" OR Eventide | 2026-11-12 | 10 | US4868869, US4991218 only |
| Web (general) | 22. Butterworth "On the Theory of Filter Amplifiers" Wireless Engineer 1930 vol 7 pp 536-541 | 2026-11-12 | 10 | Citation confirmed by several sources |
| Web (general) | 23. Robert Bristow-Johnson "Cookbook formulae for audio EQ biquad coefficients" musicdsp.org original date | 2026-11-12 | 10 | musicdsp entry 2005-05-04; March 2001 list copies |
| Google Patents (page fetch by number) | US8271109, US5789689, US4995084, US8275477B2, US8645144B2, US6337999B1, US6504935B1, US5929795, US5032796A, US5636284, US9633812B1, GB2493382A, US8258870B2, US8249275B1, US12177646B2, US5289546A, EP1234302A1, US5841875A, US4868869, US4991218, WO2019222789A1, IT202100000065A1, IT202200024429A1 | 2026-11-12 | 23 | Dates and status read; claims read only in part (US8271109, US8275477, US8645144, US8258870, US8249275) |
| Google Patents (stage 2, claims) | [US8249275B1](https://patents.google.com/patent/US8249275B1/en): claims 1, 12, 23, 26 and the dependency list of claims 1 to 27 | 2026-11-12 | 1 | Full text of the four independent claims read, in verbatim fragments; HTML full text, not OCR |
| Google Patents (stage 2, claims) | [US12177646B2](https://patents.google.com/patent/US12177646B2/en), page text from character 100000 and from 200000 | 2026-11-12 | 1 | Claim 1 read in full, in verbatim fragments; claims 19, 20 read in full; dependencies of claims 2 to 18 read from their opening words; HTML full text, not OCR |
| Crossref API | 10.17743/jaes.2019.0012; 10.1162/comj.2009.33.2.85 | 2026-11-12 | 2 | Bibliographic data read |
| Crossref API | 10.1049/ep.1930.0032 (guessed DOI for Butterworth 1930) | 2026-11-12 | 0 | HTTP 404 |
| Other pages fetched | W3C Audio EQ Cookbook; DAFx archive record (Parker et al. 2016); Amp Books preamp page; musicdsp RBJ entry via search | 2026-11-12 | 4 | Read; Amp Books is a secondary source and not relied on |

The numbering has no search 10 (a duplicate of 8 was dropped, not run).

Refused or failed: the PDFs of Kahles et al. (Aalto), Parker et al. (DAFx-16), Pakarinen and Yeh
(effectrode.com) and the Maynooth saturation paper were returned as binary and could not be read
as text. Crossref returned HTTP 404 for the guessed Butterworth DOI. Google Patents pages for
US5789689, US4995084, US8271109, US8645144B2, US6504935B1, US12177646B2 and others were truncated
at 100000 characters and the remainder was not read (US5789689 and US4995084: citation lists not
seen). In stage 2 the rest of the US12177646B2 page was read with fetch offsets, and the claims
were found there. The fetch tool would not quote a claim longer than 125 characters in one piece;
claims were taken in fragments instead. Not tried: Google Patents search UI, Espacenet, USPTO Patent Public Search and Patent
Center, FreePatentsOnline, AES e-library. Because no patent database query interface was used, no
search was run by CPC class (for example G10H 1/00, G10H 3/18, G10H 2250/131, H03G 3/00, H03G 9/00,
H03F 1/32, H04S) or by assignee or inventor name in a database; every candidate came through a
general web search or a page fetch by number. Cited-by and citation lists of the candidates were
read only for US8271109 (Google lists 32 citations and 1 cited-by plus 6 family citing entries).

## Open uncertainties

- No patent-database search was run (see "Refused or failed"); absence of candidates for elements
  3b, 3c, 5, 6 and 7 is a statement about web-search coverage, not about the databases.
- Contents of Kahles et al. 2019, Parker et al. 2016, Pakarinen and Yeh 2009 and the Maynooth and
  Leeds saturation papers were not read; no dated primary source was opened for the specific
  `atan` and `tanh` forms of element 3a and 3b, or for hard clipping as a waveshaper other than
  in US5789689A (1997).
- Element 3 crossfade on a change of shape: only US4868869A's "multiple tables with crossfading"
  (from a fetch summary of the page) bears on it; its text was not read. No source found for a
  linear crossfade over a ramp time on shape change.
- Element 7 (display of the recentred curve): no source found, prior art or patent.
- Element 5: the 1930 Butterworth paper was not opened; the RBJ cookbook earliest date is
  unestablished (March 2001 mailing-list copies are secondary; the W3C note is 2021).
- Element 6: whether reporting oversampling filter delay, padded to whole base frames, to a host
  has any patent was searched only on the open web; the resampler itself is covered by
  resampler-patents.md, whose own open risks (US 8,405,532, US 8,253,610, US 8,369,973,
  US 8,217,812) are outside this note.
- Row 1 (US8271109B2): independent claims 1 and 12 were seen only as excerpts; whether the
  asymmetric curve in claim 1 is a threshold-based piecewise function that a bias plus atan, tanh
  or clamp would meet is for stage 2. Page truncated at 100000 characters; the last 6154 characters
  were not read. Family members not opened: US20080218259A1 (the application), and the
  citing patents of this family beyond those in Candidates.
- Rows 4 and 6 (stage 2): the claim text was relayed by the fetch tool's summarising model in
  fragments of at most 120 characters and joined here; it was not compared with the granted PDFs
  ([US8249275 PDF](https://patentimages.storage.googleapis.com/4a/62/18/ac66effce96da9/US8249275.pdf)).
  A human should check the quoted claims against the PDFs or USPTO Patent Center before relying
  on the verdicts. Not checked: certificates of correction, reexamination or other post-grant
  changes to the claims, and family members (continuations) of either patent with different
  claims. US12177646B2 claim 17 was seen only in part (it depends on claim 1, so it cannot read
  if claim 1 does not).
- Row 4: the verdict rests on reading "modulating ... between multiple gain settings ... to
  produce a resulting average gain equal to the gain control value" as requiring alternation
  among gain values whose duration-weighted average is the target. A human should decide whether
  any construction could treat a per-sample ramp as "multiple gain settings" with "durations"; on
  this reading the ramp's gains do not average to the target, so the element is still unmet.
- Row 6: the Amplifier does not read; a future feature that decodes object-based audio with
  per-frame gain metadata (for example ducking an associated stream) and ramps between those gains
  would need its own check against this patent.
- Row 5 (US8258870B2) was not analysed: a human found it not in force.
- Not opened: US5133014A (Pritchard continuation), EP0307465B1, WO1988007787A1, US4877981 (Ampex
  soft-clip circuit), US5636284's citations, US9936282B2, US20130034249A1; other Line 6, Yamaha
  Guitar Group, Fractal Audio, Native Instruments, Positive Grid, Neural DSP, IK Multimedia and
  Softube patents by assignee name; cited-by lists of US5789689, US4995084, US6504935B1 and the
  rows above; the Fractal "Gain Matching" application mentioned in a forum post (no number found).
- Term end dates are as Google prints them; terminal disclaimers and adjustments not otherwise
  seen.
