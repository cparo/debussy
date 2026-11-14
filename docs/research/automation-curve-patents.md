# Patent status of the in-house automation curves (cubic Bézier segments, function-of-time validity rule, bisection evaluation, control-rate sub-block ramps, PDC-compensated evaluation, slider override, curve-port UI)

Research note, 2026-11-13. This is not legal advice. It records what primary sources show, so a
reviewer can check it. Stage 1 listed the candidates; a human filled in **Status**; stage 2
(2026-11-14) read the independent claims of the two in-force candidates (see "Claim analysis").
Dates in the Candidates table are as Google Patents prints them (Google states that its status and
expiry fields are assumptions, not legal conclusions).

## Planned design

A DAW (offline and real-time audio engine). Automation curves drive node parameters.

1. **Curve data.** A curve is a sequence of anchor points (timeline frame: integer; value:
   normalised 0 to 1), each with an in-handle and an out-handle stored as (delta frame, delta value)
   offsets from the anchor. Consecutive anchors form cubic Bézier segments: P0 = anchor i, P1 =
   anchor i + out-handle, P2 = anchor i+1 + in-handle, P3 = anchor i+1.
2. **Function-of-time validity rule.** For each segment, with x control values x0..x3, let
   a = x1 - x0, b = x2 - x1, c = x3 - x2. x'(t) >= 0 on [0,1] iff a >= 0, c >= 0, and (b >= 0 or
   b^2 <= ac). An edit (for example dragging a handle) that fails the rule is refused; the point
   stays at its last valid position.
3. **Evaluation.** Before the first anchor hold the first value, after the last hold the last; an
   empty curve is 0.5. Inside a segment, a fixed-iteration-count f64 bisection on x(t) = frame finds
   t, then value = y(t) clamped to 0 to 1. A cursor caches the current segment for O(1) sequential
   reads.
4. **Mapping.** The normalised value maps to the parameter's range: linear parameters linearly
   min to max, logarithmic parameters across their logarithm. Stepped or discrete and stopped-only
   parameters cannot be automated.
5. **Control rate.** The engine evaluates each curve at timeline frames that are multiples of 32
   (the block is split at those grid points so output is independent of audio buffer size), and the
   parameter ramps linearly to each new target across the sub-block. Evaluation time is the timeline
   frame minus the target node's arrival (plugin-delay-compensation) latency. Evaluation also runs
   while stopped, at the playhead.
6. **Override.** A parameter connected to a curve ignores its slider; the slider greys out and
   follows the curve's value at the playhead. Disconnecting returns the parameter to its stored
   value over a 10 ms ramp.
7. **UI.** Curves are lanes in the track pane; parameters are assigned to numbered "curve ports" on
   a node via a button next to the control; lanes are wired to ports on a patch board. A new point
   is made on click with horizontal handles at 1/3 of the gap to each neighbour; dragging a handle
   keeps the opposite handle collinear unless a modifier is held.

## Summary

Three candidates have a printed term end in the future (rows 1 to 3 below). A human found rows 1
and 2 in force and row 3 lapsed. The independent claims of rows 1 and 2 were read: neither reads on
the design (see "Claim analysis"). Every patent found that is closest to the design (the
monotonic-in-time Bézier editor, US6154221A, and its relatives) has expired by term. This rests on web searches and Google Patents page fetches
only: the one patent-database search attempted returned irrelevant results and Espacenet refused
the request (see "Refused or failed"). "No candidate found" means the searches in the Search log
found none, not that none exists.

| Technique | Verdict | Confidence |
|---|---|---|
| 1. Bézier anchors with in/out handles as offsets | Row 1 (US7496416B2, in force) **does not read**: its only independent claim needs "breaking the value of the control point" into a step, which the design has no step for (see the coincident-frame question under "Open uncertainties"). Basic art expired (US6154221A). | Medium |
| 2. Function-of-time validity rule, refuse edit | No in-term candidate found (searches 2, 6). The closest patent, US6154221A (monotonic check after a handle drag, repairs rather than refuses), is expired. | Low |
| 3. Bisection evaluation, hold ends, cursor | No candidate found (searches 1, 2, 5, 6). | Low |
| 4. Linear / logarithmic mapping of 0 to 1 | Row 3 (US9991861B2) only loosely (piecewise linear or logarithmic curve sections for loudness). Apple US7805685B2 (logarithmic gain-control mapping) expired 2024. | Low |
| 5. Control-rate sub-block ramps, PDC-compensated, evaluated while stopped | No candidate found (searches 3, 7, 11, 12). | Low |
| 6. Override by connected curve, slider follows, 10 ms return ramp | No patent found (searches 4, 13); behaviour documented in manuals only. | Low |
| 7. Lanes, curve ports, patch board, 1/3 handles, collinear handles | Row 1 **does not read** (collinear handles appear only in dependent claims 18 to 22, which need claim 1's value break). Row 2 (US9952739B2, in force) **does not read**: all three independent claims need a control surface linked by network to media applications on external platforms, which the design has no such element for. No patent found for numbered ports or a patch board (search 8). | Medium for rows 1 and 2; Low otherwise |

## Prior art

Items marked (unverified) come from a search summary or an index, not from the publication itself.

- **Element 1 (cubic Bézier segments, handles).**
  - Bernstein polynomials 1912, de Casteljau at Citroën 1959 (first applied to computer-aided
    design; algorithm not published until the 1980s, citing de Casteljau 1986, *Mathématiques et
    CAO, Tome 2*), P. Bézier at Renault, publicised in the 1960s, per
    [Wikipedia, Bézier curve](https://en.wikipedia.org/wiki/B%C3%A9zier_curve). Wikipedia itself
    gives no citation for Bézier's 1960s work; a search summary says his papers appeared 1966 to
    1968, and de Casteljau's note was filed with INPI in February 1959 and kept confidential
    ([arXiv 2408.13125, "A tour d'horizon of de Casteljau's work"](https://arxiv.org/pdf/2408.13125),
    paper not opened, unverified). The primary papers were not opened.
  - A parametric curve of an animated variable over time, with tangent handles at keys:
    [US6154221A](https://patents.google.com/patent/US6154221A/en) (Microsoft, later Avid; inventor
    Gangnet), priority and filing 1997-04-04, granted 2000-11-28. Also published as
    [EP0869457A2](https://patents.google.com/patent/EP0869457A2/en), priority 1997-04-04.
  - Handle break and independent handles in animation tools: Autodesk Lustre
    ([help page](https://download.autodesk.com/us/systemdocs/help/2011/lustre/files/WSc4e151a45a3b785a2ff00bd211df92972a0-7fef.htm))
    says moving one handle moves the other in the opposite direction by default and breaking lets
    each be adjusted separately; Apple Motion
    ([guide](https://support.apple.com/guide/motion/motn1474726d/mac)) says handles are locked
    together by default. Both undated (current help pages).
  - Adobe After Effects: "all interpolation methods used by After Effects are based on the Bezier
    interpolation method, which provides direction handles"
    ([Adobe help](https://helpx.adobe.com/en/after-effects/using/keyframe-interpolation.html)),
    undated; no dated source for the graph editor found (a forum comment says After Effects 7.0,
    January 2006, unverified).
  - Bézier curves in a DAW envelope: Steinberg forum thread on Bézier curves in automation
    ([thread](https://forums.steinberg.net/t/new-bezier-curves-midi/107223)), undated, by search
    summary only. WO2013170368A1 (Scratchvox), priority 2012-05-18: Bézier paths encoding pitch and
    volume in a music tool ([Google Patents](https://patents.google.com/patent/WO2013170368A1/en)).
- **Element 2 (function-of-time rule).**
  - US6154221A above: its abstract says the method "verifies whether the up-dated parametric
    function curve is monotonic" and, if not, modifies the handles repeatedly until it is (1997-04-04,
    [Google Patents](https://patents.google.com/patent/US6154221A/en)). It repairs; the design refuses
    the edit.
  - CSS `cubic-bezier()`: the x coordinates of P1 and P2 are restricted to [0, 1], "the definition is
    invalid" otherwise (CSS Timing Functions Level 1 working draft 2017-02-21,
    [W3C](https://www.w3.org/TR/2017/WD-css-timing-1-20170221); 2021 candidate draft
    [W3C](https://www.w3.org/TR/2021/CRD-css-easing-1-20210401/)). This is a sufficient, not the
    exact, condition.
  - F. N. Fritsch and R. E. Carlson, "Monotone Piecewise Cubic Interpolation", *SIAM J. Numer.
    Anal.* 17(2), 238 to 246, April 1980, doi 10.1137/0717021 (confirmed through
    [Wikipedia, Monotone cubic interpolation](https://en.wikipedia.com/wiki/Monotone_cubic_interpolation)
    and [OSTI](https://www.osti.gov/biblio/5404930) listings; paper not opened, unverified). It
    concerns monotone Hermite interpolants of monotone data, not the control-point test of the design.
  - The exact condition (a >= 0, c >= 0, b >= 0 or b^2 <= ac): no dated source found. Open
    uncertainty.
- **Element 3 (bisection evaluation, holds, cursor).** No dated source found for solving x(t) =
  frame by bisection, for hold-before-first/after-last, or for a segment cursor. Open uncertainty.
- **Element 4 (mapping).** Normalised 0 to 1 parameters with linear and logarithmic mappers:
  [Beamer docs](https://docs.rs/beamer-core/0.2.3/beamer_core/parameter_range) (undated, code
  documentation); an Apple CoreAudio list post says VST parameters are all normalised 0 to 1
  ([2006-02 thread](https://lists.apple.com/archives/coreaudio-api/2006/Feb/msg00155.html), by search
  summary, unverified). US7805685B2 (Apple), priority 2003-04-05: non-linear (logarithmic) mapping
  between a gain control and gain values ([Google Patents](https://patents.google.com/patent/US7805685B2/en)).
- **Element 5 (control rate, ramps, PDC).**
  - Control rate versus audio rate: Csound `kr`, `ksmps` and `k`/`a` variable naming
    ([Csound manual](https://csound.com/manual/OrchKvar.html)); a UCSB course note says Vercoe first
    implemented the control block in Music360 ([note](https://w2.mat.ucsb.edu/240/B/notes/Envelopes.html),
    undated, by search summary, unverified). Mathews, *The Technology of Computer Music*, MIT Press,
    1969 (book not opened); one blog claims Music V has no control rate (unverified).
  - US7176373B1 (individual, Longo), priority 2002-04-05, granted 2007-02-13: control envelopes
    producing control-rate and audio-rate signals in a performance interface
    ([Google Patents](https://patents.google.com/patent/US7176373B1/en)); expired.
  - Linear ramping of controls as a default: a linux-audio-dev post of 2002-03-16 (date from its
    title line) ([archive](https://www.mail-archive.com/linux-audio-dev@music.columbia.edu/msg03940.html),
    by search summary, unverified). Splitting a block at parameter-change offsets: Apple CoreAudio
    list threads of March 2008 ([example](https://lists.apple.com/archives/Coreaudio-api/2008/Mar/msg00159.html)),
    by search summary, unverified.
  - Plugin latency compensation also applied to automation data: Logic Pro help page
    ([Apple](https://support.apple.com/en-sa/guide/logicpro/lgcpe11997ba/mac)), undated, by search
    summary. No dated source for subtracting node latency from the evaluation time.
  - Grid-aligned evaluation independent of buffer size: no dated source found.
- **Element 6 (override).** Ableton Live: changing an automated parameter while not recording
  overrides it, its LED turns grey, "Re-Enable Automation" restores
  ([manual](https://www.ableton.com/en/live-manual/12/automation/)); Bitwig temporary override
  ([guide](https://www.bitwig.com/userguide/latest/automation)); Ardour automation states, Play mode
  controls follow the lane ([manual](https://git.ardour.org/ardour/manual/blame/commit/a1921a4a030ef5c5d257e54a5d16d29950ca7a24/include/automation-states.html));
  MusE ([developer docs](https://windfis.ch/git/muse/plain/muse2/doc/html/split/developer_docs/node5.html)).
  All undated manuals, by search summaries (unverified). No source for the 10 ms return ramp.
- **Element 7 (UI).** Breakpoint envelopes and bendable segments: Ableton
  ([manual](https://www.ableton.com/en/manual/automation-and-editing-envelopes/)), Logic Pro
  ([guide](https://support.apple.com/guide/logicpro-ipad/lpip1c94af4d/ipados)), undated. Parameter
  assigned to an automation lane from a control: Line 6 Helix
  ([manual](https://manuals.line6.com/helix-stadium-native/live/automation-controller-panel)) and
  Mixcraft ([manual](https://acoustica.com/mixcraft-11-manual/recording-real-time-automation)),
  undated. Patch-bay modulation routing of any source to any target: Apple MainStage
  ([guide](https://support.apple.com/guide/mainstage/lgsia1284cf/mac)), undated. Handles at 1/3 of
  the gap: no dated source found (1/3 is the common default for Catmull-Rom-like Bézier conversion,
  not sourced here).

## Candidates

Patents that might cover an element of the design and might still be in force. Status is filled
in by a human from Google Patents or USPTO Patent Center.

| # | Patent | Assignee | Priority / filing | Est. term end | Element it may touch | Link | Status |
|---|---|---|---|---|---|---|---|
| 1 | US7496416B2, Input/output curve editor | Luxology LLC (current: Foundry USCo Inc) | 2005-08-01 / 2006-07-11 | 2027-04-30 (Google adjusted; page prints "Active") | 1, 7 (control point with a handle, moving the handle reshapes the curve through the point; claim 1 gist, from the page: display an input-output curve, select a control point, break its value, alter the curve on a side; claims not read) | [GP](https://patents.google.com/patent/US7496416) | In force (checked by Chris Paro, 2026-11-13) |
| 2 | US9952739B2, Modular audio control surface | Avid Technology Inc | 2013-03-15 / 2013-06-03 | 2034-07-10 (Google adjusted; page prints "Active - Reinstated") | 7 (abstract: automation system for external applications, automation line graphs over waveform displays; claims not read, claim 1 gist is a modular surface) | [GP](https://patents.google.com/patent/US9952739B2/en) | In force (checked by Chris Paro, 2026-11-13) |
| 3 | US9991861B2 (application US20140044268A1), System and method for controlled dynamics adaptation for musical content | Magix AG (current as listed: Bellevue Investments GmbH & Co. KGaA) | 2012-08-10 / 2013-08-12 | 2035-02-24 (Google adjusted for the grant; the application page also records a lapse for non-payment effective 2026-07-13) | 4 (loosely: piecewise linear or logarithmic curve sections defined by curve points, called volume automation curves in a search summary; loudness-range target; claims not read) | [GP](https://patents.google.com/patent/US20140044268A1/en) | Lapsed (checked by Chris Paro, 2026-11-13) |

Status values: **In force**, **Lapsed (fees)**, **Expired (term)**, **Abandoned**, **Invalidated**,
**Not found**. Record who checked and when, under the table.

Expired by term (filing date + 20 years, plus any printed term adjustment, is in the past), listed
for completeness and needing no check:

- [US6154221A](https://patents.google.com/patent/US6154221A/en), Microsoft / Avid, filed 1997-04-04,
  expiry 2017-04-04 per Google (also lapsed for fees 2012-11-28). Parametric curve with tangent
  handles whose monotonic domain is checked and repaired after a drag (elements 1, 2, 7).
  [EP0869457A2](https://patents.google.com/patent/EP0869457A2/en), same family, withdrawn 2002-03-20.
- [US6469702B1](https://patents.google.com/patent/US6469702B1/en), Avid, filed 1999-04-16, expiry
  2019-04-16 per Google. Editing function curves by a 2-D region of selected keys (element 7, loosely).
- [US7176373B1](https://patents.google.com/patent/US7176373B1/en), individual (Longo), filed
  2003-11-26 (priority 2002-04-05), adjusted expiry 2023-09-14 per Google. Control envelopes with
  control-rate and audio-rate signals (element 5).
- [US7805685B2](https://patents.google.com/patent/US7805685B2/en), Apple, filed 2007-10-15 (priority
  2003-04-05), adjusted expiry 2024-08-09 per Google. Non-linear gain-control mapping (element 4).
- [US7610553B1](https://patents.google.com/patent/US7610553B1/en), Apple, filed 2003-04-05, adjusted
  expiry 2025-11-24 per Google (expired for unpaid fees). Reducing recorded parameter-event data,
  for example audio gain over time (element 1, loosely).
- [US7196707B2](https://patents.google.com/patent/US7196707B2/en), Autodesk, filed 2004-09-10,
  adjusted expiry 2024-11-24 per Google. Equalising tangent handle lengths of a Bézier spline
  (element 7, loosely); [GB2406028A](https://patents.google.com/patent/GB2406028A/en) same priority
  family, withdrawn.
- [US7317457B2](https://patents.google.com/patent/US7317457B2/en), Autodesk, filed 2004-07-20,
  adjusted expiry 2026-03-16 per Google. Keyframe handle slope and magnitude recomputed to stop
  overshoot (elements 1, 7, loosely).
- [US8209612B2](https://patents.google.com/patent/US8209612B2/en), Apple, filed 2010-04-19 (priority
  2003-10-15), adjusted expiry 2024-05-31 per Google. Editable speed graph for video (element 7,
  loosely).
- [WO2013170368A1](https://patents.google.com/patent/WO2013170368A1/en), Scratchvox, priority
  2012-05-18, printed "Ceased", expiry 2014-11-18 per Google. Bézier paths encoding pitch and volume
  (element 1); family members not seen.

Seen and not listed (the abstract names no element of the design, or claims differ in kind):

- US7884834B2 (Apple), adjusted expiry 2029-04-29, printed "Active": paint stroke property control
  at an anchor point ([Google Patents](https://patents.google.com/patent/US7884834B2/en)).
- US10410317B1 (Adobe), printed "Active", anticipated expiry 2038-03-26: image transformation by
  a spline handle ([Google Patents](https://patents.google.com/patent/US10410317B1/en)). A human may
  prefer to list it for the handle element.
- US9933990B1 (Sonitum), printed "Active", expiry 2035-12-31: mapping point-set topology to signal
  processor parameters ([Google Patents](https://patents.google.com/patent/US9933990B1/en)).
- US11527226B2 (Yamaha), printed "Active", adjusted expiry 2038-09-13: changing a parameter of
  other parts to follow a reference part's parameter change pattern
  ([Google Patents](https://patents.google.com/patent/US11527226B2/en)).
- US9905208 (Speed of Sound Software, 2018): DAW split points for track assembly, from a search
  summary; not opened.
- Cited by US6154221A, not opened: 50 listed on its Google page, for example US7345786B2,
  US8379030B1, US8280171B2, US9459771B2, US9779521B2, US10510186B2. Most are video or image tools.

## Claim analysis

Stage 2, 2026-11-14. Claim text comes from the Google Patents HTML pages (not OCR: both pages give
the claims as text), as passed on by the fetch tool, which returns quotes in pieces of at most 125
characters. The pieces below are given in order and join into each claim; they have not been
checked against the USPTO grant PDF (see "Open uncertainties").

### Row 1: US7496416B2 (priority 2005-08-01)

Claims 1 to 22; claim 1 is the only independent claim (claims 2 to 22 depend on it, directly or
indirectly). Claim 1:

| Element | Planned design |
|---|---|
| "A method of varying the relationship between an input parameter and an output parameter, comprising:" | Preamble. A curve relates timeline frame (input) to parameter value (output): design 1. |
| "displaying a relationship between a scalar input parameter and a scalar output parameter as a curve on a computer screen;" | Met: curves are drawn as lanes in the track pane (design 7). |
| "selecting a control point on the curve;" | Met: anchors are selected and dragged (design 7). |
| "breaking the value of the control point; and" | **Not met.** Each anchor has one value (design 1); the design has no operation that splits an anchor's value. Its handles stay collinear unless a modifier is held, which breaks the slope (dependent claims 2 to 4), not the value. |
| "altering the value of the curve on at least one side of the broken control point," | Not met: there is no broken control point. |
| "thereby varying the relationship between the input parameter and the output parameter" / "as a step-function at the broken control point." | **Not met.** The design's curve is continuous at every anchor: a segment ends at P3 = anchor i+1 and the next starts at that anchor (design 1). |

Verdict: **Does not read.** Missing element: "breaking the value of the control point" with the
resulting "step-function at the broken control point". The design's single-valued curve (dependent
claim 8, from the page summary) and collinear handles (dependent claims 18 to 22, from the page
summary; their text was not quoted) only matter if claim 1 is met. Caveat: if the design lets two
anchors share a frame, the curve has a step there (see "Open uncertainties").

Prior art in this note predating 2005-08-01: US6154221A (1997-04-04, tangent handles on a
parametric curve of a value over time), US7196707B2 (filed 2004-09-10) and US7317457B2 (filed
2004-07-20), and Bézier's and de Casteljau's work. None of them shows breaking a point's value into
a step.

### Row 2: US9952739B2 (priority 2013-03-15)

Claims 1 to 14; independent claims 1 (media control surface), 8 (media control surface module) and
10 (method). The three share their limitations; claim 1 is taken in full and claims 8 and 10 by
their differences.

Claim 1:

| Element | Planned design |
|---|---|
| "A media control surface comprising:" / "a plurality of modules including a display module, the display module comprising:" | **Not met.** The design is a DAW application (design preamble); it has no control surface or display module. |
| "a network interface that connects the display module to a network;" | **Not met.** The design has no network interface for curves. |
| "an output display for displaying properties of one or more media processing applications" / "hosted on one or more platforms external to the media control surface," / "wherein each of the one or more platforms is in communication with the media control surface via the network; and" | **Not met.** The design's nodes run in its own engine; no external platform, no network. |
| "an automation system capable of automating the one or more media processing applications," / "wherein automating a given one of the one or more media processing applications includes:" | Partly: the design automates node parameters (design 5), but not external applications. |
| "at the automation system, receiving control parameter adjustments via one or more input controls" / "of the media control surface, wherein the control parameter adjustments are applied to the processing of media data" / "by the given one of the one or more media processing applications;" | **Not met.** Curves are drawn point by point in lanes (design 7); a connected slider is ignored (design 6). No recording of control moves is described. |
| "time stamping and storing the control parameter adjustments in storage of the automation system;" | Not met: no recorded adjustments. Anchors carry frames (design 1), but they are drawn, not time-stamped control moves. |
| "in response to receiving an automation playback command subsequent to the storing of the control parameter adjustments:" / "causing logic within the automation system to retrieve the stored control parameter adjustments" / "from the storage of the automation system;" | Partly: curves are evaluated during playback (design 5), but they are not stored control adjustments. |
| "sending the stored control parameter adjustments to the one or more platforms external to the media control surface" / "in synchrony with playback of the media data; and" | **Not met.** Values go to nodes inside the engine, not to external platforms. |
| "wherein the media control surface:" / "receives over the network data comprising a temporal location of a plurality of clips of the media data" / "being processed by the given media processing application; and" | **Not met.** No network; the design's track pane knows its own clips. |
| "uses logic of the display module of the media control surface to generate" / "from the retrieved control parameter adjustments an automation line graph" / "and to temporally align the automation line graph with the temporal location of the plurality of clips" / "of the media data being processed by the given media processing application," | Partly: curve lanes sit in the track pane on the timeline (design 7), but not on a control surface and not from recorded adjustments. |
| "thereby enabling the logic of the display module to superimpose the automation line graph onto a display" / "of the temporal location of the plurality of clips," / "wherein the superimposition of the automation line graph onto the display of the temporal location" / "of the plurality of clips is independent of automation data received over the network" / "from the given media processing application." | **Not met.** The design does not say curves are drawn over clips; lanes are their own rows (design 7). There is no network and no external application. |

Claim 8 adds "a CPU;" and "a random access memory connected to the CPU;" and otherwise repeats
claim 1's limitations, including "a network interface for connecting the module to a network;" and
"platforms external to the audio control surface". Not met for the same reasons.

Claim 10 recites "receiving via one or more input controls of a media control surface adjustments
of a set of control parameters" that "apply to parameters of one or more media" / "processing
applications hosted on platforms external to the media control surface", time-stamping and storing
them, "sending the stored control parameter adjustments to the one or more platforms external to
the media control" / "surface in synchrony with playback of the media data", and the same aligned,
superimposed automation line graph built "independently of automation data received from the given"
/ "media processing application." Not met for the same reasons.

Verdict: **Does not read.** Missing elements, in every independent claim: a media control surface
(with, in claims 1 and 8, a network interface) and media processing applications "hosted on
platforms external to the media control surface" to which recorded control adjustments are sent.

Prior art in this note predating 2013-03-15: the Ardour, Ableton and MusE automation manuals cover
playback of automation in a DAW, but they are undated, so they cannot be placed before the priority
date. Nothing in the note addresses a networked control surface.

## Search log

All searches 2026-11-13. The web search tool returns a prose summary, not raw result lists, so hit
counts are approximate (about 10 per query).

| Database | Query | Date | Hits | Result |
|---|---|---|---|---|
| Web (general) | 1. patent automation envelope Bezier curve handles digital audio workstation automation lane | 2026-11-13 | 10 | No patent; TDCommons disclosure, manuals |
| Web (general) | 2. patent automation curve monotonic function of time Bezier control points constraint x non-decreasing animation curve editor | 2026-11-13 | 10 | US6154221, US7496416, US7317457, US20050237325A1 (not opened) |
| Web (general) | 3. patent plugin delay compensation automation data timing latency compensated parameter automation audio workstation | 2026-11-13 | 16 | No patent; manuals, forums |
| Web (general) | 4. patent automation override control slider follows automation curve disconnect return to stored value audio parameter | 2026-11-13 | 10 | No patent; manuals |
| Web (general) | 5. de Casteljau Bézier curves history 1959 1962 Citroën Renault publication | 2026-11-13 | 10 | Dates (see Prior art) |
| Web (general, extended) | 6. Google Patents "automation" audio sequencer parameter automation curve breakpoints interpolation user interface "automation lane" claims | 2026-11-13 | 9 | US9952739, US7176373, US9933990, US20140044268 |
| Web (general) | 7. patent keyframe Bezier tangent handles collinear break tangent animation curve editor graph editor Adobe OR Autodesk OR Pixar | 2026-11-13 | 15 | No collinear-handle patent; US7496416, EP0869457 |
| Web (general) | 8. patent assigning a plug-in parameter to a control curve "automation" button next to control knob, patch routing lanes to parameters music software | 2026-11-13 | 15 | No patent; manuals |
| Web (general) | 9. patent control rate audio parameter smoothing linear ramp per block automation interpolation sample accurate audio plug-in host | 2026-11-13 | 10 | No patent; forums |
| Web (general, extended) | 10. patent "automation data" "latency" compensation plug-in "automation" delayed audio track mixing timeline playback parameter changes synchronized with audio | 2026-11-13 | 9 | No automation patent; Logic Pro manual |
| Web (general) | 11. Ableton OR Steinberg OR Avid OR Apple patent automation envelope breakpoints curve editing audio parameter user interface granted | 2026-11-13 | 10 | No patent; US11527226 listed by an index |
| Web (general) | 12. patent parameter automation "overridden" OR "override" manual control automation playback indicator control follows automation value touch release | 2026-11-13 | 10 | No patent; manuals |
| Web (general) | 13. patent logarithmic parameter mapping normalized value 0 to 1 control curve audio parameter range linear logarithmic mapping modulation source | 2026-11-13 | 10 | US7805685 |
| Web (general) | 14. Csound control rate k-rate audio rate a-rate kr ksmps Music N history Mathews Music V 1969 control signals | 2026-11-13 | 16 | Csound manual; no Music N primary |
| Web (general) | 15. CSS Easing Functions Level 1 cubic-bezier x values must be in range 0 to 1 W3C specification history timing function | 2026-11-13 | 17 | W3C drafts 2017, 2021 |
| Web (general) | 16. monotone cubic Bézier x(t) monotonic condition control points differences a c nonnegative b^2 <= 4ac derivative quadratic nonnegative on [0,1] | 2026-11-13 | 10 | No source for the exact condition |
| Web (general) | 17. Fritsch Carlson 1980 "Monotone piecewise cubic interpolation" SIAM Journal on Numerical Analysis 17(2) 238-246 doi | 2026-11-13 | 10 | Citation and DOI via indexes |
| Web (general) | 18. Mathews "The Technology of Computer Music" 1969 MUSIC V control rate envelopes; Vercoe Csound 1986 control rate "k-rate" history MIT Music 11 | 2026-11-13 | 10 | No primary source |
| Web (general) | 19. After Effects graph editor Bezier keyframe interpolation handles "temporal" "spatial" introduced version history 1990s Adobe user guide | 2026-11-13 | 14 | Adobe help page, undated |
| Web (general, extended) | 20. patent "automation" "digital audio workstation" "spline" OR "Bezier" automation curve parameter "control point" tangent handle claims music production software US patent 2015..2024 | 2026-11-13 | 9 | WO2013170368, US7196707, US7317457, GB2406028 |
| Web (general) | 21. patent modulation routing "patch" lanes assigned to parameter ports node graph audio editor automation curve connected to parameter slider disabled while connected | 2026-11-13 | 13 | No patent; MainStage, Bitwig |
| Web (general) | 22. patent digital audio workstation automation "parameter automation" sample-accurate split audio block at automation points granted claims Steinberg OR PreSonus OR "Image-Line" OR Cakewalk OR Propellerhead | 2026-11-13 | 15 | No patent; US9905208 (unrelated) |
| Web (general) | 23. patent audio mixing automation data stored as breakpoints with curve shape between points, playhead position parameter value display, automation while stopped | 2026-11-13 | 10 | No patent; manuals |
| Google Patents (page fetch by number) | US6154221A, US7496416, US7176373B1, US9952739B2, US9933990B1, EP0869457A2, US7805685B2, US11527226B2, WO2013170368A1, US7196707B2, US7317457B2, GB2406028A, US7884834B2, US8209612B2, US10410317B1, US7610553B1, US20140044268A1, US6469702B1 | 2026-11-13 | 18 | Dates, status, abstracts read; claims not read; cited-by read for US6154221A only |
| Other pages fetched | Wikipedia "Bézier curve" | 2026-11-13 | 1 | History section read |
| Google Patents (claims, stage 2) | `patents.google.com/patent/US7496416B2/en`, fetched twice with a prompt for the claims | 2026-11-14 | 1 | Claims section within the first 100000 characters; claim 1 quoted, claims 2 to 22 summarised by the tool |
| Google Patents (claims, stage 2) | `patents.google.com/patent/US9952739B2/en`, read from character 100000 onward (the first fetch ended in the description) | 2026-11-14 | 1 | Claims 1 to 14 present; independent claims 1, 8, 10 quoted in full, dependents summarised |

Refused or failed: Espacenet REST search (`worldwide.espacenet.com/3.2/rest-services/search?...`)
returned HTTP 403. A Google Patents query endpoint (`patents.google.com/xhr/query?url=q=...`) for
"automation" "bezier" "audio" returned 74 results that were all unrelated (Nvidia, Huawei, UMNAI
and similar), so it is not counted as a search. Google Patents pages for US7176373B1, US9952739B2,
US9933990B1, US7805685B2, US11527226B2, WO2013170368A1, US7196707B2, US7317457B2, GB2406028A,
US7884834B2, US8209612B2, US10410317B1, US7610553B1 were truncated at 100000 characters and the
rest not read, so no claims were seen for any of them (stage 2 later read past the cut for
US9952739B2 only). FreePatentsOnline (`freepatentsonline.com/9952739.html`) returned HTTP 403 on
2026-11-14. Not tried: USPTO Patent Public Search and
Patent Center, FreePatentsOnline, Google Patents result pages by CPC class (G10H 1/0008, G10H
2210/..., G06F 3/04847, G11B 27/031) or by assignee (Avid, Steinberg, Ableton, Apple, Image-Line,
PreSonus, Native Instruments, Bitwig, Cockos, Blackmagic/Fairlight), and Japanese filings.

## Open uncertainties

- No working patent-database query was run, so "No patent found" for elements 2, 3, 5, 6 and 7 is
  a statement about web-search coverage. Searches by CPC class and by assignee are the main gap.
  A human should run them in Google Patents or USPTO Patent Public Search.
- The three candidate rows were found through abstracts and page headers. Row 3 was found lapsed by
  a human, so its claims were not read.
- Rows 1 and 2: claim text was read from Google Patents HTML through a fetch tool that passes quotes
  on in pieces; it was not checked word for word against the USPTO grant PDFs or for certificates
  of correction. A human may want to confirm claim 1 of US7496416B2 and claims 1, 8 and 10 of
  US9952739B2 against the PDFs before relying on the verdicts.
- Row 1, coincident anchors: the design does not say whether two anchors may share a frame. The
  validity rule (design 2) accepts a segment whose x control values are all equal (a = b = c = 0),
  which would make the curve a step at that frame. Whether creating such a pair counts as
  "breaking the value of the control point" with a "step-function at the broken control point"
  (US7496416B2 claim 1) is a question for counsel. A human should decide whether the design
  requires anchor frames to be strictly increasing; with that rule, the verdict "Does not read" has
  no caveat. The design must also never add a "break value" or "split point" command while the
  patent is in term (printed end 2027-04-30).
- Row 2: the verdict rests on the design having no external control surface and no network link to
  external applications. Adding hardware control-surface support (for example EUCON-style or
  networked surfaces recording automation) would call for a fresh reading of US9952739B2 (printed
  end 2034-07-10).
- Element 2: no dated source for the exact validity condition (a >= 0, c >= 0, and b >= 0 or b^2 <=
  ac), nor for refusing an edit and keeping the last valid position. US6154221A (expired) repairs by
  adjusting handles; the CSS x-in-[0,1] rule is a sufficient simplification only.
- Element 3: no dated source for fixed-iteration bisection, hold-first/hold-last, empty curve 0.5,
  or the segment cursor.
- Element 5: no dated source for evaluating at a 32-frame grid with block splitting, or for
  subtracting a node's PDC latency from the evaluation time; Logic Pro's help is undated. The Music
  N control-rate primary sources were not opened (Mathews 1969; Vercoe).
- Element 6: only undated manuals document override behaviour; no source for the 10 ms return ramp.
- Element 7: no dated source for numbered curve ports or a patch board; no patent found for either.
- The primary de Casteljau and Bézier publications were not opened; their dates rest on Wikipedia
  and a search summary of a 2024 arXiv survey.
- Cited-by lists were followed only for US6154221A, and only by number.
- Term end dates are as Google prints them; terminal disclaimers and adjustments not otherwise
  seen.
