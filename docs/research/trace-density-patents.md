# Patent status of the intensity-graded trace density display

Research note, 2026-11-08. This is not legal advice. It records what primary sources show, so a
reviewer can check it.

## Planned design

An "intensity-graded" (digital-phosphor-style) waveform display for the track lanes of an audio
editor, used at fine zooms of 2 to 63 audio frames per pixel column (`spc`). The elements, as they
will be built:

1. The sampled mono signal (f32 samples, clamped to +/-1) is treated as straight lines between
   consecutive samples (linear interpolation).
2. Per pixel column of `spc` frames (2 to 63), the column owns the `spc` segments from sample
   `i*spc` to sample `(i+1)*spc`. The next column's first sample closes the last segment.
3. Each segment is worth `1/spc` of the column's time. That time is split over the vertical pixel
   rows (amplitude bands, +1 at the top row to -1 at the bottom) that the segment's value range
   crosses, in proportion to the segment's overlap with each row. A flat segment puts all of it in
   one row. The result is a per-column "time-in-level" histogram that sums to 1.
4. Rows are the lane's pixel height.
5. Each column's histogram is normalised by that column's maximum, passed through a square-root
   curve, and used as the alpha (opacity) of a single fixed audio colour when filling the pixel.
   Vertical runs of equal quantised alpha (32 levels) are merged into one rectangle for drawing.
6. Coarser zooms (64 or more frames per column) keep a separate, unchanged min/max/RMS bar display
   from precomputed summaries. That display is not part of this check.

## Summary

| Technique | Verdict | Confidence |
|---|---|---|
| Elements 1 to 3 (linear segments, per-column time-in-level histogram) | No in-force candidate reads. The nearest, US7589728B2, needs detecting and filling a discontinuity between adjacent columns' histograms, which the design does not do. Elements 1 to 3 have 1988 to 1994 prior art in expired patents (see Prior art). | Medium: literal reading of independent claims only, from six candidates found by web search |
| Element 4 (rows are the lane's pixel height) | No candidate claims it on its own. | Medium |
| Element 5 (normalise, square root, alpha of one colour, 32 levels, run merging) | No in-force candidate reads. US10565973B2 needs a polynomial of degree three or more applied to the audio signal; the square root on the histogram is neither. US8330758B2, US9672794B2 and US10983146B2 need random-threshold state updates, compressed counters in display memory, or persistence and decay. | Medium |
| Element 6 (min/max/RMS bars) | Not checked; outside this note. US7834780B2 claim 32 is relevant to it (see Open uncertainties). | n/a |

Claims were read for all six in-force candidates (see Claim analysis): none reads on elements 1 to
5. This is a literal claim reading, not a freedom-to-operate opinion. Whether it is enough for
in-house implementation under `docs/architecture/patents.md` "Patent policy" is for a human to
decide.

## Prior art

Dates below are from the Google Patents pages cited. They are patent filing or grant dates, which
are public descriptions of the element.

- Element 1 (straight lines between samples, intensity varying with position): US5440676A,
  Tektronix, filed 1988-01-29, granted 1995-08-08. Draws the digitised waveform as straight-line
  vectors, lighting pixels by distance from the vector path.
  <https://patents.google.com/patent/US5440676A/en>
- Elements 2, 3 and 5 (per-column histogram where each vector contributes a weight that depends on
  how many pixels it spans, mapped to display intensity): US5550963A, Tektronix, filed 1994-12-08,
  granted 1996-08-27. The abstract says "a histogram of hits is built for each compression
  interval, with each vector's contribution weighted by the vector's length", and claim 1 recites
  accumulators for one display column and a weight that is a function of the number of pixels in
  the range. <https://patents.google.com/patent/US5550963A/en>
- Element 5, brightness from how often a point is hit (hit count to brightness, not an audio
  display): US4940931A, Anritsu, priority 1988-06-24, granted 1990-07-10.
  <https://patents.google.com/patent/US4940931A/en>
- Element 5, single-colour population shading with saturation levels: US6151010A, LeCroy SA,
  filed 1996-05-24, granted 2000-11-21 (multiple acquisitions).
  <https://patents.google.com/patent/US6151010A/en>
- Digital phosphor as a product: Tektronix DPO introduced 1998, per the Tektronix timeline and a
  July 1998 Electronic Design article, both found by search (query in the log). I did not open
  either page, so the dates are second-hand.
  <https://www.electronicdesign.com/home/article/21200224/a-new-type-of-oscilloscope-offers-significant-advantages>
- Open-source renderer, audio-oriented: `scopeplot` by endolith. Its README says it connects
  adjacent points with linear interpolation, splits the signal into one chunk per pixel column and
  shows the per-column histogram as brightness. Visible commits run from 2015-07-16 to 2019-06-11
  (commit list page). Licence not seen. It also upsamples by FFT interpolation first, which the
  planned design does not. <https://github.com/endolith/scopeplot>,
  <https://github.com/endolith/scopeplot/commits/master>
- Audio min/max column display with per-column colouring: US6184898B1, Comparisonics, filed
  1998-03-26, granted 2001-02-06. <https://patents.google.com/patent/US6184898B1/en>
- Analog phosphor oscilloscope: no primary source was opened for it. Open uncertainty below.
- Square-root alpha curve and 32-level alpha quantisation with run merging: no prior-art source
  found. Open uncertainty below.

## Candidates

Patents that might cover an element of the design and might still be in force. Status is filled
in by a human from Google Patents or USPTO Patent Center. "Est. term end" is the figure Google
Patents prints on the linked page. Google calls it an assumption, not a legal conclusion, and it
may omit term adjustment, so a human should confirm it.

| # | Patent | Assignee | Priority / filing | Est. term end | Element it may touch | Link | Status |
|---|---|---|---|---|---|---|---|
| 1 | US7589728B2, Digital oscilloscope display and method for image quality improvement | LeCroy Corp (Teledyne LeCroy) | Filed 2004-09-15, granted 2009-09-15 | 2027-02-02 (adjusted, per Google) | 3, 5: per-column histogram of sample values from one acquisition, counts mapped to colour or grey level by a saturation control | <https://patents.google.com/patent/US7589728B2/en> | In force |
| 2 | US8330758B2, Statistically-based display processing | Teledyne LeCroy (page lists LeCroy Inc.) | Filed and priority 2008-05-08, granted 2012-12-11 | 2031-10-12 (adjusted, per Google) | 5: pixel brightness or colour state driven by waveform hits per pixel (random-threshold state update) | <https://patents.google.com/patent/US8330758B2/en> | In force |
| 3 | US9672794B2, Measuring device with a display memory having memory cells with a reduced number of bits (EP2990809B1 is the European counterpart, est. end 2034-10-22) | Rohde & Schwarz | Priority 2014-08-29, filed 2015-03-18, granted 2017-06-06 | 2035-05-23 (adjusted, per Google) | 3, 5: per-cell frequency counts of sampled values in amplitude/time bins, shown as pixel brightness or colour | <https://patents.google.com/patent/US9672794B2/en> | In force |
| 4 | US10983146B2 (published as US20180180647A1), Oscilloscope persistence for digital bus signals | Tektronix | Priority and filing 2016-12-28, granted 2021-04-20 | 2038-02-12 (adjusted, per Google) | 3, 5: stored hit frequencies per value shown on a display. It concerns digital bus values, so the touch is weak | <https://patents.google.com/patent/US20180180647A1/en> | In force |
| 5 | US7834780B2 (application US20070217694A1), Waveform compression and display | Tektronix | Priority and filing 2006-03-20, granted 2010-11-16 | 2029-08-17 (adjusted, per Google) | 5: pixel intensity differs between a peak-detected background trace and a filtered foreground trace. Touches the min/max display more than the density display | <https://patents.google.com/patent/US7834780B2/en> | In force |
| 6 | US10565973B2, Audio waveform display using mapping function | Home Box Office, Inc. | Provisional 2018-06-06, filed 2018-12-18, granted 2020-02-18 | 2038-12-18 (per Google) | 5: a polynomial (degree three or more) mapping applied to audio for waveform display in an editing system. Touches the amplitude-to-display curve, not the density | <https://patents.google.com/patent/US10565973B2/en> | In force |

Status values: **In force**, **Lapsed (fees)**, **Expired (term)**, **Abandoned**, **Invalidated**,
**Not found**. Record who checked and when, under the table.

Statuses checked by Chris Paro on 2026-11-08: all six in force.

Expired by term (filing date + 20 years, plus any printed term adjustment, is in the past), listed
for completeness and needing no check. Expiry dates are those Google Patents prints on each page.

- US5550963A, Tektronix, filed 1994-12-08 (term end 2014-12-08; Google also shows lapse for
  non-payment). Graded display of digitally compressed waveforms. Per-column histogram weighted by
  vector length. <https://patents.google.com/patent/US5550963A/en>
- US6222521B1, Tektronix, filed 1998-04-03 (2018-04-03). Variable intensity rasterizer.
  <https://patents.google.com/patent/US6222521B1/en>
- US6333732B1, Tektronix, filed 1998-06-05 (2018-06-05). Multi-function digital persistence decay.
  <https://patents.google.com/patent/US6333732B1/en>
- US6847905B2, Tektronix, filed 2003-02-12 (2023-03-23, adjusted). Per-time-slice histograms of
  amplitude counts. <https://patents.google.com/patent/US6847905B2/en>
- US7216046B2, Tektronix, filed 2003-03-19 (2024-02-19, adjusted; also lapsed for fees 2015).
  Variable persistence waveform database. <https://patents.google.com/patent/US7216046B2/en>
- US6965383B2, LeCroy, granted 2005-11-15 (2022-04-02, adjusted). Scaling persistence data with
  interpolation. Filing date not seen. <https://patents.google.com/patent/US6965383B2/en>
- US6151010A, LeCroy SA, filed 1996-05-24 (2016-05-24). Population shading with saturation.
  <https://patents.google.com/patent/US6151010A/en>
- US6344844B1, Agilent, filed 1998-08-21 (2018-08-21). Peak detect intensity by significance.
  <https://patents.google.com/patent/US6344844B1/en>
- US5254983A, Hewlett-Packard, filed 1991-02-05 (2011-02-05). Digitally synthesized gray scale.
  <https://patents.google.com/patent/US5254983A/en>
- US5440676A, Tektronix, filed 1988-01-29 (2012-08-08 per Google). Pixel intensity gradation for
  straight-line vectors. <https://patents.google.com/patent/US5440676A/en>
- US4940931A, Anritsu, filed 1989-06-20 (2009-06-20). Shading-tone display.
  <https://patents.google.com/patent/US4940931A/en>
- US6184898B1, Comparisonics, filed 1998-03-26 (2018-03-26). Audio min/max display with
  frequency-based colouring. <https://patents.google.com/patent/US6184898B1/en>

Not in force because the application was abandoned, so no term runs (Google's legal events list
failure to respond to an office action). Listed for completeness:

- US20080201092A1, Sound Evolution Pty Ltd, filed 2006-08-22, abandoned about 2011-02. Audio
  waveform with min/max per pixel plus shades of one colour for frequency-component intensity.
  <https://patents.google.com/patent/US20080201092A1/en>
- US20150097839A1, Tektronix, filed 2014-05-27, abandoned 2018. Stochastic rasterization of
  waveform trace displays. <https://patents.google.com/patent/US20150097839A1/en>

## Claim analysis

Claims read 2026-11-08 for all six candidates (all marked **In force**). This is a literal reading
of the independent claims against elements 1 to 5 of the Planned design. It does not cover the
doctrine of equivalents or whether a preamble limits a claim; both are listed under Open
uncertainties.

How the claim text was obtained: from the Google Patents HTML page of each patent (links in
Candidates), through a fetch tool that returns quotes of at most 125 characters. Each claim was
therefore received as consecutive verbatim clauses, rejoined below. Where a clause was cut short,
"[...]" marks the gap. The page does not say whether its text is OCR'd. The claim text read cleanly
for all six. The description of US9672794B2 shows OCR-like artefacts, so its claim text is marked
**possibly OCR** below. No claim was checked against the PDF.

The design facts the claims are tested against, all from Planned design: a desktop audio editor
draws stored audio files. Every redraw recomputes the per-column histogram from the stored samples.
There is no acquisition hardware, no trigger, no combining of several acquisitions or sweeps, no
persistence or decay, no random-number step, no hardware rasterizer, and no display memory of
per-pixel counters kept between frames. The histogram is a floating-point fraction of column time,
not an integer hit count.

### 1. US7589728B2 (LeCroy): Does not read

Independent claims: 1 (method), 6 (oscilloscope), 12 (oscilloscope, means-plus-function). Claims 6
and 12 recite the same processor steps as claim 1. All other claims (2 to 5, 7 to 11, 13 to 17)
depend on these.

Claim 1, element by element:

| Claim 1 element (verbatim) | Planned design |
|---|---|
| "A method performed by a processor for displaying an acquired signal on an oscilloscope, comprising the steps of:" | Not an oscilloscope; displays stored audio files. Whether a recorded audio file is an "acquired signal", and whether the preamble limits the claim, is a legal question. |
| "acquiring a data signal;" | Not done by the display. The editor may record audio elsewhere, but this display only reads stored samples. |
| "determining, by said processor, a plurality of samples of the acquired data signal representing samples at different times that are to be represented by each of a plurality of vertical pixel columns of a display;" | Met by element 2 (each column owns `spc` samples). |
| "generating, by said processor, a histogram of the values of the samples for each vertical pixel column;" | Met in substance by element 3, though the design's histogram is of time spent in each row along interpolated segments, not of sample values. |
| "determining, by said processor, a discontinuity in histogram values between histograms associated with adjacent pixel columns;" | **Not met.** The design never compares adjacent columns' histograms. Its column histograms are contiguous because each segment, including the one closed by the next column's first sample (element 2), spreads its time over every row it crosses (element 3). |
| "removing, by said processor, the discontinuity by filling in histogram bins with a value for the bins corresponding to the discontinuity; and" | **Not met.** No discontinuity is detected, so none is filled. |
| "modifying, by said processor, a display characteristic of information displayed in one or more of the vertical pixel columns based upon values in the histogram corresponding to each vertical pixel column." | Met by element 5 (alpha from the normalised histogram). |

Claims 6 and 12 add "an acquisition board for acquiring a data signal" and "acquisition means for
acquiring a data signal" respectively, inside "An oscilloscope, comprising:". The design has
neither. Their processor steps include the same discontinuity elements, quoted for claim 6:
"determining a discontinuity in histogram values between histograms associated with adjacent pixel
columns," and "removing the discontinuity by filling in histogram bins with a value for the bins
corresponding to the discontinuity, and". Not met, as for claim 1.

Verdict: **Does not read.** Missing in every independent claim: determining a discontinuity
between adjacent columns' histograms and filling it. Also missing: acquiring a data signal, and for
claims 6 and 12 the acquisition board or means and the oscilloscope. No dependent claim mentions
interpolation, multiple acquisitions, colour, intensity or saturation (per the fetch).

Prior art before its 2004-09-15 priority: US5550963A (filed 1994-12-08) builds a per-column
histogram in which each vector adds weight over the pixel range it spans. That is the design's way
of making column histograms contiguous (elements 2 and 3), not LeCroy's detect-and-fill way.
US5440676A (1988) and US4940931A (1988) also predate it.

### 2. US8330758B2 (LeCroy): Does not read

Independent claims: 1, 7, 15, 22 (all methods). Claim 1 was received only in part. Its middle
clauses were cut off by the fetch tool and the page section split.

Claim 1 (partial), element by element:

| Claim 1 element (verbatim) | Planned design |
|---|---|
| "A method for processing waveform information to update an image being displayed on a display device, the method comprising:" | Met in general terms (draws a waveform image). |
| "receiving sampled waveform data to be rendered as an image on a display device;" | Met (stored samples). |
| "associating at least a portion of the sampled waveform data with a pixel of the display device to form an image [...]" | Met by elements 2 and 3. |
| "receiving state information for the pixel, the received state information comprising a value corresponding to one of a finite [...]" | **Not met.** No pixel state is kept or received; each redraw starts from the samples. |
| "providing a transition probability vector (TPV) comprising a plurality of predetermined threshold values [...]" | **Not met.** No TPV or thresholds. |
| "comparing a substantially random value to the predetermined threshold value that corresponds to the received state infor[...]" | **Not met.** The design is deterministic; there is no random value. |
| "updating the pixel state based on a result of the comparison; and," | **Not met.** |
| "storing a control signal for controlling the pixel on the display device according to the updated pixel state;" | Not met (no pixel state). |
| "wherein the predetermined threshold values in the TPV are defined substantially so as to reduce the probability of a transition as pixel intensity approaches visual saturation." | **Not met.** |

Claim 7 has claim 1's first six steps and adds "fading the pixel intensity by:" "providing a fade
transition probability vector (FTPV) [...]" and "comparing a second substantially random value to
the predetermined threshold value in the FTPV that corresponds to state information for the
pixel; and". The design has no fading and no random value.

Claim 15 recites "storing pixel state information representing display information for at least a
portion of the sampled waveform;", "determining whether to update to the pixel state of the
identified pixel based on a comparison of a substantially randomized value to the selected
predetermined threshold value;" and "fading a pixel intensity based upon a comparison of a second
substantially random number to a second predetermined threshold value; and". None is in the
design. Its "storing sampled waveform data [...] wherein the sample waveform data has a sample
period that is substantially shorter than a period of time between screen refreshes on the display
device;" is met (audio sample periods are far shorter than a frame).

Claim 22 recites "triggering sweeps based on predetermined trigger conditions;", "for a plurality
of the sweeps triggered during the predetermined screen refresh period,", "(i) identifying a random
value;" and "(vi) decreasing the pixel's intensity if one or more predetermined promotion criteria
are met based on the comparison;". The design has no triggering, no sweeps, no random value and no
decay.

Verdict: **Does not read.** Missing in every independent claim: comparing a substantially random
value with a predetermined threshold for the pixel's stored state. Claims 7, 15 and 22 also need
fading or decreasing intensity, and claim 22 needs triggered sweeps.

### 3. US9672794B2 (Rohde & Schwarz): Does not read

Independent claim: 1 only (12 claims; 2 to 12 depend on earlier claims; no device claim). Claim text
is **possibly OCR**, see above.

| Claim 1 element (verbatim) | Planned design |
|---|---|
| "A method for reducing a number of bits used for a frequency value of a measuring signal stored in each memory cell of a display memory in a measuring device" | **Not met.** No display memory of per-cell counts, no bit reduction, not a measuring device. |
| "the method comprising the following steps:" | n/a |
| "determining the frequency value in each memory cell by assigning the frequency of sampled values in several measuring portions of a measured signal within an update cycle of the display to a corresponding memory cell; and" | **Not met.** The design does not combine several measuring portions (acquisitions) into one cell per update cycle. Each column's histogram comes from one stretch of one stored signal. |
| "displaying each pixel of the display with a brightness or a color corresponding to the frequency value in the corresponding memory cell after each update cycle," | Partly met by element 5 (alpha from the histogram), but there is no memory cell. |
| "wherein the determined frequency value is a sum of a first frequency value, which is determined in a number of first measuring portions of the measured signal within the update cycle," | **Not met.** |
| "and at least one compressed second frequency value, which is determined by compression of a corresponding uncompressed second frequency value with a compression factor" | **Not met.** No frequency value is compressed. |
| "and each frequency value stored in the memory cell for display in a corresponding pixel of the display is treated separately." | **Not met.** |

Verdict: **Does not read.** Missing: the frequency value as a sum of a first frequency value and a
compressed second frequency value, held in a reduced-bit display memory cell over several measuring
portions per update cycle.

Prior art before its 2014-08-29 priority, for per-cell counts shown as brightness: US5550963A
(1994), US6847905B2 (2003), US4940931A (1988).

### 4. US10983146B2 (Tektronix): Does not read

Independent claims: 1 (system), 10 (method), 15 (storage medium). Claims 16 to 19 depend on 15.

Claim 1, element by element:

| Claim 1 element (verbatim) | Planned design |
|---|---|
| "A test and measurement system comprising:" | **Not met.** An audio editor. |
| "an input port structured to connect to a digital bus conducting a digital signal from a device under test;" | **Not met.** No digital bus, no device under test. |
| "at least one acquisition circuit coupled to the input port and configured to sample the digital signal to obtain a plurality of digital values;" | **Not met.** No acquisition circuit. |
| "a display;" | Met. |
| "a memory configured to: store hit frequencies for each of the plurality of digital values for a plurality of time periods in hit lines" | **Not met.** No per-value hit lines; the design apportions continuous time over amplitude rows. |
| "and store hit frequencies for each transition between two of the plurality of digital values for the plurality of time periods in hit lines," | **Not met.** |
| "each hit line including a label indicating either a digital value or a transition between digital values, and" | **Not met.** |
| "store data indicating the digital value associated with each hit frequency for each time period; and" | **Not met.** |
| "at least one processor coupled to the input port, the display, and the memory," | Partly (a processor and display; no input port). |
| "the processor configured to determine a persistence of each the plurality of digital values over time," | **Not met.** No persistence. |
| "to determine a decay of each of the plurality of digital values over time," | **Not met.** No decay. |
| "and to cause the display to depict the digital values and the hit frequencies of the digital values by depicting the persistence of the digital values over time and by depicting the decay of the digital values over time." | **Not met.** |

Claim 10 is the method form ("receiving, at an input port, digital values over a digital bus
coupled to a device under test;", "determining a persistence [...]", "determining a decay [...]").
Claim 15 is the storage-medium form, with the hit lines "in a digital phosphor oscilloscope (DPO)
memory;" and display "on an oscilloscope display". Its opening words before "[...]ecuted by at
least one processor" were split across the page and not received verbatim.

Verdict: **Does not read.** Missing in every independent claim: digital values received over a
digital bus from a device under test, labelled hit lines for values and transitions, and
persistence and decay.

### 5. US7834780B2 (Tektronix): Does not read (elements 1 to 5)

Independent claims: 1, 17, 32, 37 (41 claims). Claim 2's dependency was not seen; the fetch of the
first part of the page did not report it as independent.

Claim 1: "A digital acquisition system that compresses sampled data prior to storage comprising:"
"means for digitizing an electrical signal at a high sample rate to produce a sampled data
stream;" "means for compressing the sampled data stream in parallel simultaneously with a plurality
of compression schemes," "at least one of the compression schemes being a lowpass filtering
scheme," "to produce a compressed sampled data stream for each compression scheme; and" "means for
storing the compressed sampled data streams as acquired sample data for each compression scheme."
Design: no digitizing (stored files), and the density display compresses nothing before storage.
**Not met** from the first means onward. Claim 17 is the method form with the same steps; same
result.

Claim 32, element by element:

| Claim 32 element (verbatim) | Planned design (elements 1 to 5) |
|---|---|
| "A method of displaying overlaid waveform images derived from a sampled electrical signal comprising the steps of:" | Partly; one image only, from a sampled signal. |
| "compressing the sampled electrical signal using a peak detect compression scheme to produce a background version of the sampled electrical signal;" | **Not met.** The density display does not peak-detect. |
| "compressing the sampled electrical signal using a decimation/lowpass filtering scheme to produce a foreground version of the sampled electrical signal;" | **Not met.** |
| "displaying the foreground version having a default intensity level; and" | **Not met.** No overlaid foreground trace. |
| "displaying the background version having a variable intensity level." | **Not met.** |

Claim 37: "An acquisition system comprising:" "means for acquiring data from an electrical signal;"
"means for waveform averaging the data over a plurality of acquisitions by the acquiring means to
produce an averaged waveform; and" "means for displaying the averaged waveform overlaid with a
waveform derived from the data by another means so as not to obscure information contained in any
of the waveforms." Design: no acquisition, no averaging over several acquisitions. **Not met.**

Verdict: **Does not read** on elements 1 to 5. Missing: digitizing and parallel compression
including a lowpass scheme (claims 1, 17); a peak-detect background overlaid with a
decimated/lowpass foreground at different intensities (claim 32); averaging over a plurality of
acquisitions (claim 37). Element 6 (min/max/RMS bars) was not checked; see Open uncertainties.

### 6. US10565973B2 (Home Box Office): Does not read

Independent claims: 1 (system), 10 (method), 18 (storage medium).

Claim 10, element by element:

| Claim 10 element (verbatim) | Planned design |
|---|---|
| "A computer-implemented method, comprising," | Met. |
| "selecting an audio signal from audio data;" | Met (a track's stored audio). |
| "mapping the audio signal to a reshaped audio waveform via a polynomial mapping function of at least degree three;" | **Not met.** The samples are only clamped to +/-1 (element 1) and mapped linearly to rows (element 3). The one curve in the design, the square root in element 5, is not a polynomial and is applied to the normalised histogram (opacity), not to the audio signal. |
| "outputting a visible representation of the reshaped audio waveform; and" | Not met as claimed (no reshaped waveform); the design does output a visible waveform. |
| "receiving editing instructions associated with the reshaped audio waveform." | The editor receives editing instructions, but not on a reshaped waveform. |

Claim 1 needs "an audio signal processing component that reshapes an audio signal via a
polynomial function of at least degree three into audio waveform data wherein the polynomial
function comprises predetermined coefficients;". Claim 18 needs "processing an audio signal [...]
into reshaped audio waveform data via a cubic polynomial mapping function, wherein the cubic
polynomial mapping function comprises predetermined coefficients;". Neither is in the design. Both
also recite an editing component or editing information, which the editor has.

Verdict: **Does not read.** Missing in every independent claim: reshaping the audio signal by a
polynomial of degree three or more (claim 18: cubic) before display.

## Search log

All searches were run on 2026-11-08 through a web search tool that returns summaries and links,
and then each patent page was opened on Google Patents. The tool is not a patent-database query
interface, so hit counts are not available. Fields marked "n/a" could not be recorded.

| Database | Query | Date | Hits | Result |
|---|---|---|---|---|
| Web search | `digital phosphor oscilloscope patent intensity graded waveform database Tektronix` | 2026-11-08 | n/a | Found US5550963A, US6222521B1, US6333732B1, US7216046B2, US20150097839A1, EP0947841/2 |
| Web search | `patent audio waveform display density histogram per pixel column intensity alpha audio editor` | 2026-11-08 | n/a | Found US20080201092A1, US6847905B2, US8330758B2, US10565973B2, scopeplot |
| Web search | `Keysight Agilent patent waveform display intensity grading "intensity" pixel density oscilloscope rasterization sample points connected lines` | 2026-11-08 | n/a | Found US5254983A, US5283596, US5440676A, US6344844B1, EP0947842; Keysight app note 5989-2003 |
| Web search | `Rohde Schwarz patent oscilloscope waveform rasterization color-graded density interpolation segments pixel column` | 2026-11-08 | n/a | Found US6249115B1, US10332287B2, EP2990809B8 |
| Web search (extended) | `patent display of audio waveform at high zoom level samples line segments intensity brightness proportional time spent pixel, audio editing software` | 2026-11-08 | n/a | Found US6184898B1, US7834780B2, US20070217694A1, US8890869B2, US8189797B1, US9076457B1, US20120269364A1, US7551744, US20040183818A1. No density display |
| Web search (extended) | `patent oscilloscope waveform display "interpolat" samples line segments histogram pixel "per column" persistence database digital phosphor after 2005 Keysight OR "Rohde & Schwarz" OR LeCroy OR Tektronix` | 2026-11-08 | n/a | Found US7589728B2, US6965383B2, US6151010A, US20180180647A1 |
| Web search | `Agilent OR Keysight patent "intensity graded" OR "intensity grading" waveform display density per pixel oscilloscope memory granted 2008..2020` | 2026-11-08 | n/a | No Keysight/Agilent patent found after 2002 |
| Web search | `Tektronix DPO digital phosphor oscilloscope introduced 1998 TDS 500D 700D announcement history` | 2026-11-08 | n/a | 1998 introduction (second-hand) |
| Google Patents (page fetch) | US5550963A, US6222521B1, US6847905B2, US8330758B2, US20080201092A1, US10565973B2, US7216046B2, US20150097839A1, US6333732B1, US10332287B2, EP2990809B1, US9672794B2, US7589728B2, US6965383B2, US20180180647A1, US6184898B1, US7834780B2, US4940931A, US5254983A, US5440676A, US6151010A, US6344844B1 | 2026-11-08 | 22 | Dates, assignees and term ends in the tables above. Pages are long and were read only in part (first 100,000 characters) where a fetch note says so |
| GitHub (page fetch) | endolith/scopeplot, and its commit list | 2026-11-08 | 2 | README and commit dates above |
| Google Patents (claims fetch) | US7589728B2, claims section of <https://patents.google.com/patent/US7589728B2/en> | 2026-11-08 | 1 | Independent claims 1, 6, 12 received in full as verbatim clauses; dependent 11 seen; 17 claims |
| Google Patents (claims fetch) | US8330758B2, <https://patents.google.com/patent/US8330758B2/en>, both halves (offset 0 and 100,000) | 2026-11-08 | 1 | Independent claims 1, 7, 15, 22. Claim 7, 15, 22 received in full; claim 1's middle clauses cut short at 125 characters and at the page split |
| Google Patents (claims fetch) | US9672794B2, <https://patents.google.com/patent/US9672794B2/en>, second half (offset 100,000) | 2026-11-08 | 1 | 12 claims, only claim 1 independent, received in full. Description shows OCR-like artefacts |
| Google Patents (claims fetch) | US10983146B2, <https://patents.google.com/patent/US10983146B2/en>, both halves | 2026-11-08 | 1 | Independent claims 1, 10 in full; claim 15 in full except its opening words, split across the page halves |
| Google Patents (claims fetch) | US7834780B2, <https://patents.google.com/patent/US7834780B2/en>, both halves | 2026-11-08 | 1 | 41 claims; independent 1, 17, 32, 37 in full. Claim 2's dependency not seen |
| Google Patents (claims fetch) | US10565973B2, <https://patents.google.com/patent/US10565973B2/en>, first 100,000 characters | 2026-11-08 | 1 | Independent claims 1, 10, 18 in full. The final 6,023 characters of the page were not read |

Refused or failed: none. The fetch tool limits each quote to 125 characters, so claims came back as
consecutive verbatim clauses; none was checked against the patent PDF. Not searched: USPTO Patent Center, Espacenet, and the "cited by" lists of
the candidates (the fetches did not return them).

## Open uncertainties

- Not searched in USPTO or Espacenet directly, and no forward-citation lists were read. The
  "no Keysight patent after 2002" result rests on two search-tool queries only. Rohde & Schwarz
  and Keysight families beyond those found may exist.
- Candidate term ends are Google's estimates. Candidate 1 may run out around 2027-02-02, and the
  human status check should confirm the date and maintenance fees.
- US10332287B2 (Rohde & Schwarz, colouring a displayed signal by a measured parameter) was seen
  and left out, because its abstract and first claim concern signal parameters, not density. Its
  claims were read only through a summary.
- Claim stage: the independent claims were read from Google Patents HTML text through a fetch
  tool, as verbatim clauses of at most 125 characters, and not checked against the PDFs. Gaps: the
  middle of US8330758B2 claim 1, the opening words of US10983146B2 claim 15, claim 2's dependency in
  US7834780B2, and the last 6,023 characters of the US10565973B2 page. Whether the text is OCR'd
  is not stated; US9672794B2's is possibly OCR. A human may want to confirm the claims from the
  USPTO or EPO publications.
- Doctrine of equivalents (a human must decide, with counsel if needed): US7589728B2 removes gaps
  between adjacent columns' histograms by detecting and filling them; the design avoids such gaps
  by apportioning each interpolated segment, including the one closing into the next column, over
  every row it crosses. The result is similar, the way differs, and US5550963A (filed 1994,
  expired) used the design's way before LeCroy's 2004 priority. The literal verdict is Does not
  read. Its term also ends around 2027-02-02 by Google's figure.
  Resolved 2026-11-08 by Chris Paro, who read the patent's description: its fill applies only to
  a sample-gap condition (samples too sparse to join adjacent columns) that cannot arise at the
  zooms the trace draws.
- Preambles: whether "on an oscilloscope" (US7589728B2 claim 1) or "in a measuring device"
  (US9672794B2) limits the claim was not decided. The verdicts do not rest on them; each patent
  also lacks a body element.
- Element 6, outside this note: US7834780B2 claim 32 overlays a peak-detect "background version"
  at "a variable intensity level" with a "decimation/lowpass filtering" "foreground version" at "a
  default intensity level". The coarse min/max/RMS bar display draws min/max (peak) bars with RMS
  bars over them. Whether RMS is a "decimation/lowpass filtering scheme" and whether the bars have
  different intensities decides whether claim 32 reads on element 6. A separate check is needed.
- Only independent claims were analysed. A dependent claim cannot read where its independent
  claim does not, so this is complete for literal infringement, but continuations, divisionals
  and foreign family members of the six (other than EP2990809B1, not read) were not searched.
- Audacity, other open-source audio editors and the analog phosphor oscilloscope literature were
  not searched, so there is no dated prior art for an audio-editor density display or for analog
  phosphor. The earliest density-by-histogram sources found are the patents above.
- No source found for the square-root curve on the normalised histogram or for 32-level alpha
  quantisation with vertical run merging. That may only mean there is nothing to cite.
- Tektronix DPO 1998 dates come from search-result descriptions and were not opened.
- Per-segment time apportioned over rows by overlap (element 3) was not seen verbatim in any
  source. The nearest are US5550963A (vector length weight over a pixel range) and scopeplot (each
  line segment contributes to one or two pixels).
