# Patent status of the in-house Delay effect (circular-buffer delay, tempo sync, tap crossfade, ping-pong, filtered and saturated feedback)

Research note, 2026-11-13. This is not legal advice. It records what primary sources show, so a
reviewer can check it. Stage 1 listed the candidates; a human marked **Status**; stage 2
(2026-11-13) analysed the claims of the one in-force candidate (see "Claim analysis"). Dates in the Candidates table are as Google Patents prints them (Google
states that its status and expiry fields are assumptions, not legal conclusions).

## Planned design

Software, portable scalar Rust, a "Delay" effect node in an audio editor; per-sample processing on
the real-time audio thread; two channels (left and right lines).

1. **Delay line.** One circular buffer per channel, sized at prepare for 4 s plus the offset range
   at the graph sample rate. Reads come from whole-frame taps; there is no fractional interpolation.
2. **Delay time.** Either free (1 to 4000 ms) or tempo-synced: note value x feel (straight, dotted
   x1.5, triplet x2/3), computed from frames per whole note = frames_per_beat x beat_unit. The
   result is clamped to the buffer.
3. **Change of delay time (or tempo) while playing.** Crossfades linearly from the old tap to the
   new one over a fixed ramp time; no pitch glide.
4. **Right channel offset.** -50 to +50 ms added to one side's delay time.
5. **Modes.** Stereo: each channel feeds back into itself. Ping-pong: the input is summed to mono
   into the left line, and each line's feedback is written into the other line.
6. **Feedback path.** read -> optional 2nd-order trapezoidal state-variable-filter low-pass
   (already cleared in [ADR-0088](../adrs/0088-equalisers-are-built-on-a-trapezoidal-state.md))
   -> optional tanh saturation (already cleared in
   [ADR-0102](../adrs/0102-the-amplifier-is-a-memoryless-waveshaper-with-pre.md)) -> x feedback gain
   (0 to 100%) -> added to the input and written. Each toggle crossfades its share in or out over
   the ramp time.
7. **Output.** dry x (1 - mix) + wet x mix.

Not in scope: modulation of the delay time (wow, flutter, chorus), fractional-delay interpolation,
tape or bucket-brigade circuit models.

## Summary

One candidate has a printed term end in the future (row 1, US9711119B2); its independent claims
1, 9, 10 and 12 do not read on the planned design (see "Claim analysis"). No candidate was found for elements 1,
3, 4, 5, 6 or 7 whose term could still run, in the searches in the Search log; this rests on web
searches and Google Patents page fetches only, because no patent database query interface could be
used (see "Refused or failed"). The crossfaded-tap art found (element 3) is from 1984 to 1998 and
expired; the closest art for the feedback path (element 6) is from 1998 and expired.

| Technique | Verdict | Confidence |
|---|---|---|
| 1. Circular-buffer delay, whole-frame taps | No in-term candidate found (searches 14, 15). Basic digital delay art is from 1970 to 1980 and expired. | Medium |
| 2. Tempo-synced delay time from note value x feel | Row 1 (US9711119B2): **Does not read**. Claims 1, 9, 10 need a sequence-clock count adjusted from a detected time difference; claim 12 needs a controller that changes music playback timing and speed to follow the effect. The design has neither. Earlier art, expired: US5430243A. | Medium |
| 3. Linear crossfade between old and new tap on a time change | No in-term candidate found (searches 3, 6, 11). Earlier art, expired: US4644400A, US5781461A, US5742532A. | Low |
| 4. Right-channel offset of -50 to +50 ms | No candidate found; no search targeted it specifically beyond searches 4, 9. | Low |
| 5. Stereo and ping-pong cross-feedback | No patent found in searches 2, 4, 9; the technique is documented in manuals and a 1999 article. | Low |
| 6. Filtered then saturated feedback, gain, added to input | No in-term candidate found (searches 5, 7, 8, 13). Earlier art, expired: US6351540B1. | Low |
| 7. Dry/wet mix | No candidate found; no search targeted it. | Low |

## Prior art

Items marked (unverified) come from a search summary, not from the publication itself.

- **Element 1 (delay line as stored samples read back after a set interval).**
  - US3681531A (Industrial Research Products), priority and filing 1970-09-04, granted 1972-08-01:
    a digital delay system with "a progressive digital data store" and multiple readout means
    reading after different intervals; its description has recirculation with feedback
    ([Google Patents](https://patents.google.com/patent/US3681531A/en)).
  - US4348754A (Ampex), priority and filing 1980-05-15, granted 1982-09-07: audio written to
    sequentially addressed memory and read a set number of addresses later, with a time-delay
    register and a counter that restarts the addressing cycle
    ([Google Patents](https://patents.google.com/patent/US4348754A/en)).
- **Element 2 (delay time tied to musical tempo).**
  - US5430243A (Kawai), priority 1992-09-30, filed 1993-09-28, granted 1995-07-04: claim 1
    "calculating a basic delay time based on a period of a timing clock of said MIDI signal"
    ([Google Patents](https://patents.google.com/patent/US5430243A/en); claim wording from a fetch
    summary).
  - Note value x feel arithmetic (dotted x1.5, triplet x2/3) appears in product manuals and
    calculator pages, none dated in the sources opened: [Roland Sync Delay](https://static.roland.com/manuals/p-6/en-US/119755659144753419.html),
    [Max Cookbook, delay with tempo-relative timing](https://music.arts.uci.edu/dobrian/maxcookbook/delay-tempo-relative-timing)
    (the cookbook example: a dotted eighth at 120 BPM is 375 ms). No dated primary source for the
    frames-per-whole-note computation specifically. Kurzweil K2500 FXAlg #134 "8-Tap Delay BPM" sets
    delay lengths by tempo and beats ([manual page](https://www.manualslib.com/manual/288831/Kurzweil-K2500.html?page=40));
    the page shows no date.
  - US9711119B2 (Casio), priority 2015-12-14: DSP delay on a ring buffer, delay time set to beat
    multiples at 30 to 300 BPM ([Google Patents](https://patents.google.com/patent/US9711119B2/en)).
- **Element 3 (crossfade between old and new delay position on a change).**
  - US4644400A (NEC), priority 1984-04-09, granted 1987-02-17: audio held in memory is read from
    both the previous and the new offset and the two outputs are cross-faded over a set period, to
    avoid clicks when the delay changes
    ([Google Patents](https://patents.google.com/patent/US4644400A/en)). Whether the fade is linear
    is not established.
  - US5781461A (Stanford; Jaffe and Smith), priority 1996-05-09: delay line with two readers and a
    crossfader whose ramp is S-shaped, avoiding clicks and glissando on a change of delay length
    ([Google Patents](https://patents.google.com/patent/US5781461A/en); also WO1998010355A1,
    [Google Patents](https://patents.google.com/patent/WO1998010355A1/en), priority 1996-09-04,
    crossfade of "about 10 to 30 milliseconds" per a fetch summary).
  - US5742532A (Stanford; Van Duyne, Jaffe, Scandalis, Stilson), priority and filing 1996-05-09:
    two read pointers with an alternating crossfader, for fractional delay
    ([Google Patents](https://patents.google.com/patent/US5742532)).
  - Jaffe's two-pointer crossfade is cited as an ICMC 1997 paper, "A Lossless, Click-free,
    Pitchbend-able Delay Line Loop Interpolation Scheme"
    ([Michigan Publishing](https://quod.lib.umich.edu/i/icmc/bbp2372.1997.068?rgn=main;view=fulltext));
    the page returned HTTP 403, so its content is not read here (unverified, taken from a search
    summary).
  - Practitioner descriptions of crossfading two taps, undated: [Max Cookbook](https://music.arts.uci.edu/dobrian/maxcookbook/abstraction-crossfading-between-delay-times).
  - US5920842A (Pixel Instruments), priority 1994-10-12: changes signal duration without changing
    pitch using memory reset and crossfade; background of US8134644B2 describes it as a circular
    buffer with one write and two read addresses cross-faded
    ([Google Patents](https://patents.google.com/patent/US5920842A/en)); claims not read.
- **Element 4 (L/R offset of delay time).** No dated source found. Open uncertainty.
- **Element 5 (stereo and ping-pong cross-feedback).**
  - Hans Mikelson, Csound Magazine, Summer 1999: stereo delay with cross-feedback, "add some
    cross-feedback so that the right channel can feed into the left channel"
    ([Csound Journal](https://csoundjournal.com/ezine/summer1999/processing/index.html)).
  - Kurzweil K2500 manual: the sum of input and feedback "may be mixed or swapped" with the other
    channel's, giving a ping-pong effect ([manual page](https://www.manualslib.com/manual/288831/Kurzweil-K2500.html?page=40)),
    undated. A mono-sum-into-left variant is described for an E-MU Proteus X manual, undated, per a
    search summary only (unverified).
- **Element 6 (filter and nonlinearity in the feedback loop).**
  - US6351540B1 (Yamaha), priority 1998-09-22, filed 1999-09-14, granted 2002-02-26: digitised
    input into a feedback loop containing a digital delay, a distortion-adding block and an
    equalizer, the looped signal mixed with the dry sound
    ([Google Patents](https://patents.google.com/patent/US6351540B1/en)).
  - US4244262A (Roland), priority 1977-11-15: low-pass filters before and after a shift-type delay
    element (not in a feedback loop per its abstract)
    ([Google Patents](https://patents.google.com/patent/US4244262A/en)).
  - Filter and saturation in a tape-echo style feedback path: [Logic Pro Tape Delay](https://help.apple.com/logicpro-effects/mac/10.2/en.lproj/lgcef18b8994.html)
    and [Brainmodular Audio Tape Echo](https://www.brainmodular.com/manuals/hh7/fr/modules/audio/fx/audio-tape-echo),
    undated. The SVF and tanh pieces themselves are covered by ADR-0088 and ADR-0102.
- **Element 7 (dry/wet mix).** US6351540B1 above mixes the looped signal with the dry sound. No
  other source opened.

## Candidates

Patents that might cover an element of the design and might still be in force. Status is filled
in by a human from Google Patents or USPTO Patent Center.

| # | Patent | Assignee | Priority / filing | Est. term end | Element it may touch | Link | Status |
|---|---|---|---|---|---|---|---|
| 1 | US9711119B2, Audio processing device, method of audio processing, storage medium, and electronic musical instrument | Casio Computer Co., Ltd. | 2015-12-14 / 2016-12-13 | 2036-12-13 (Google; page prints "Active") | 2 (a tempo-synchronised DSP delay on a ring buffer with delay time set to beat multiples; claim 1 as read recites two processors counting a sampling clock and a sequence clock and adjusting the second count to reduce the time difference; claims 9, 10, 12 not read in full) | [GP](https://patents.google.com/patent/US9711119B2/en) | **In force** |

Checked by Claude from Google Patents, 2026-11-13, under Chris Paro's rule: no expired or revoked
note means In force. The US9711119B2 page shows "Active", anticipated expiration 2036-12-13.

Status values: **In force**, **Lapsed (fees)**, **Expired (term)**, **Abandoned**, **Invalidated**,
**Not found**. Record who checked and when, under the table.

Expired by term (filing date + 20 years, plus any printed term adjustment, is in the past), listed
for completeness and needing no check:

- [US6351540B1](https://patents.google.com/patent/US6351540B1/en), Yamaha, filed 1999-09-14,
  anticipated expiry 2019-09-14 per Google (also lapsed for fees 2010-02-26). Distortion block,
  delay and equalizer in a digital echo feedback loop (element 6).
- [US5430243A](https://patents.google.com/patent/US5430243A/en), Kawai, filed 1993-09-28, expiry
  2013-09-28 per Google (lapsed for fees 2003-07-04). Delay time from MIDI clock period (element 2).
- [US4644400A](https://patents.google.com/patent/US4644400A/en), NEC, filed 1985-04-08, expiry
  2005-04-08 per Google. Crossfade between old and new read offset (element 3).
- [US5781461A](https://patents.google.com/patent/US5781461A/en), Stanford, filed 1996-09-04 per
  Google (priority 1996-05-09), expiry 2016-05-09 per Google. Two-reader delay line with crossfader
  (element 3). Its PCT counterpart WO1998010355A1 is printed as "Ceased".
- [US5742532A](https://patents.google.com/patent/US5742532), Stanford, filed 1996-05-09, expiry
  2016-05-09 per Google (lapsed for fees 2010-04-21). Two read pointers with crossfader (element 3).
- [US5920842A](https://patents.google.com/patent/US5920842A/en), Pixel Instruments, filed
  1994-10-12, expiry 2016-07-06 per Google. Crossfade in a duration changer (element 3).
- [US7259316B2](https://patents.google.com/patent/US7259316B2/en), Roland, filed 2004-04-02,
  adjusted expiry 2025-08-22 per Google. Tempo-based loop repeat; its description says it avoids
  feedback delay (element 2).
- [US5811707A](https://patents.google.com/patent/US5811707A/en), Roland, filed 1995-06-05, expiry
  2015-09-22 per Google. Karaoke pitch conversion or delay (element 1, loosely).
- [US4244262A](https://patents.google.com/patent/US4244262A/en), Roland, filed 1978-09-07, expiry
  1998-09-07 per Google. Low-pass filters around a shift-type delay (element 6, loosely).
- [US3681531A](https://patents.google.com/patent/US3681531A/en), Industrial Research Products,
  filed 1970-09-04, expiry 1989-08-01 per Google. Digital delay store (element 1).
- [US4348754A](https://patents.google.com/patent/US4348754A/en), Ampex, filed 1980-05-15, expiry
  2000-05-15 per Google. Digital delay in addressed memory (element 1).

Seen and not listed (the claims or abstract name no element of the design, or the record is not a
live patent):

- US8145496B2 (Transeau, "Stutter Edit"), priority 2006-05-25, adjusted expiry 2030-10-23 per
  Google, printed "Active": claim 1 is a stutter/retrigger system tied to a master clock; the only
  mention of a tempo-synced delay line is in the description of an optional plug-in, and the page
  has no feedback ([Google Patents](https://patents.google.com/patent/US8145496B2/en)). Not
  listed because no claim or abstract reaches an element; a human may prefer to list it.
- US8134644B2 (Cooper, audio synchronizer), priority 2005-10-15, adjusted expiry 2030-12-05 per
  Google, printed "Expired - Fee Related": independent claims are about delay-change information
  and audio-video synchronisation, no crossfade in the claims read (claims 82 to 100 were cut off
  and not read) ([Google Patents](https://patents.google.com/patent/US8134644B2/en)).
- EP1681669A1 (Kawai, distortion device), withdrawn 2007-07-21; WO2008125582A1 and
  US20100195840A1 (Ciccone), ceased and abandoned; US10644658B2 (Samsung, radio transmitter),
  cited by US6351540B1 but unrelated to audio effects.
- US20090281793A1, the Stutter Edit application, same family as US8145496B2.

## Claim analysis

Stage 2, 2026-11-13, by Claude. Claims are quoted from the Google Patents text of US9711119B2
(see Search log), not OCR; the granted PDF is a scanned image with no text layer and was not OCR'd.

### Row 1: US9711119B2 (Casio), priority 2015-12-14

**Claim 1** (device).

| Element (quoted) | Planned design |
|---|---|
| "a first processor that cyclically counts a sampling clock to a first count value, and outputs an audio effect sound generated by processing a received audio waveform signal each time the count of the sampling clock reaches the first count value" | Partly. The Delay node runs per sample on the audio thread (design intro) and reads a tap a set number of frames back (items 1, 2), so an echo appears one delay time after the input. It keeps no cyclic counter that gates output; it outputs every sample (item 7). Whether a fixed tap offset is "cyclically counts ... to a first count value" is arguable, but does not decide the verdict. |
| "a second processor that cyclically counts a sequence clock to a second count value, and causes a corresponding segment of a preset music to be played each time the count of the sequence clock reaches the second count value so as to perform an automatic play of the preset music" | Not in the planned design. The design is one effect node; it has no sequence clock and plays no music. The editor's transport is outside the design; if it plays a project, it does so from the graph sample clock, which the Delay also uses (item 2: frames_per_beat). |
| "wherein each time the count of the sampling clock reaches the first value, the first or second processor, or a separate circuit unit in the audio processing device detects a time difference between a time at which the count of the sampling clock reaches the first count value and a time at which the count of the sequence clock reaches the second count value a number of times corresponding to said time at which the count of the sampling clock reaches the first count value," | No such element. The design detects no time difference between two clocks: delay time is computed from tempo in frames of the one graph clock (item 2). |
| "and adjusts the second count value for a subsequent cycle of counting in accordance with the detected time difference so as to reduce the detected time difference, thereby providing synchronization of the output of the audio effect sound with the automatic play of the preset music over time." | No such element. Nothing adjusts a sequence count; on a tempo change the Delay crossfades to a new tap (item 3), which changes the effect, not the music's count. |

Verdict, claim 1: **Does not read.** Missing: the second processor's sequence-clock count, the
detection of a time difference between the two counts, and the adjustment of "the second count
value for a subsequent cycle of counting".

**Claim 9** (method). Its elements are claim 1's, as steps: "causing the first processor to:
cyclically count a sampling clock to a first count value, and output an audio effect sound ...";
"causing the second processor to: cyclically count a sequence clock to a second count value, and
cause a corresponding segment of a preset music to be played ..."; and "each time the count of the
sampling clock reaches the first value, causing the first or second processor, or a separate circuit
unit in the audio processing device to: detect a time difference ... and adjust the second count
value for a subsequent cycle of counting in accordance with the detected time difference so as to
reduce the detected time difference". Mapping as for claim 1. Verdict: **Does not read**, same
missing elements.

**Claim 10** (storage medium). "A non-transitory computer-readable storage medium having stored
therein a program executable by an audio processing device having a first processor operating under
a sampling clock and a second processor operating under a sequential clock", the program performing
the same three steps as claim 9 (quoted wording identical apart from "the sampling clock" / "the
sequence clock"). The design is software, so the medium preamble could be met; the second processor
"operating under a sequential clock", the detection and the adjustment are not. Verdict: **Does not
read**, same missing elements.

**Claim 12** (instrument).

| Element (quoted) | Planned design |
|---|---|
| "a sound emitting unit that receives an audio waveform signal supplied from an input unit and repeatedly emits, at a prescribed timing, an audio effect sound generated by processing the audio waveform signal," | Arguably met: a tempo-synced delay with feedback repeats the input at the delay time (items 2, 5, 6). |
| "the sound emitting unit further outputting musical notes of a preset music stored in a storage unit to perform an automatic play of the preset music; and" | Not in the planned design. The Delay node plays no music; whether the host editor's playback of a project is "automatic play of the preset music" is outside this design. |
| "a controller that changes, in accordance with said prescribed timing at which the audio effect sound is emitted by the sound emitting unit, playback timing and speed of the automatic play of the preset music by the sound emitting unit." | No such element. The dependency runs the other way: the Delay's timing follows the tempo (item 2); nothing changes playback timing or speed to follow the effect. |

Verdict, claim 12: **Does not read.** Missing: the controller that changes "playback timing and
speed of the automatic play of the preset music" in accordance with the effect's timing.

**Patent verdict: Does not read** (no independent claim has every element met). Dependent claims
narrow these and so cannot read either. Prior art in this note that predates the 2015-12-14
priority: US5430243A (Kawai, priority 1992-09-30), delay time from a MIDI timing clock period, which
covers tempo-synced delay time in general; US7259316B2 (Roland, filed 2004-04-02), tempo-based loop
repeat. Neither was compared to the claimed count adjustment.

## Search log

All searches 2026-11-13. The web search tool returns a prose summary, not raw result lists, so hit
counts are approximate (about 10 per query unless noted). Google Patents page fetches read dates
and status; claims were read only where stated.

| Database | Query | Date | Hits | Result |
|---|---|---|---|---|
| Web (general) | 1. patent tempo-synchronized digital delay effect note value dotted triplet delay time computed from tempo | 2026-11-13 | 10 | No patent; manuals and calculators |
| Web (general) | 2. patent ping-pong stereo delay cross-feedback left right delay line feedback written into opposite channel | 2026-11-13 | 10 | No patent; manuals (Steinberg, E-MU, Kurzweil) |
| Web (general) | 3. patent crossfade between two delay line taps when delay time changes avoid click glitch-free digital delay | 2026-11-13 | 16 | No patent; Max and KVR threads |
| Web (general) | 4. patent digital delay feedback loop low-pass filter saturation in feedback path tape echo emulation | 2026-11-13 | 10 | US6351540; manuals |
| Web (general, extended) | 5. "delay time" "tempo" "delay line" patent US synchronize delay effect to musical tempo MIDI clock BPM echo claims | 2026-11-13 | 9 | US5430243, US9711119, US7968784, US7855334 |
| Web (general, extended) | 6. patent "delay time" changed "cross-fade" OR "crossfade" between read pointers delay buffer audio effect without pitch change claims | 2026-11-13 | 10 | US5742532, WO1998010355, US8134644 |
| Web (general, extended) | 7. patent stereo delay "cross feedback" OR "cross-coupled" OR "ping pong" left channel right channel delay feedback musical instrument effect US patent | 2026-11-13 | 9 | No cross-feedback delay patent; US5274708, US4355203, US4489439 not opened |
| Web (general, extended) | 8. US patent audio delay effect "feedback" "low-pass filter" "saturation" "delay time" synchronized tempo plug-in digital audio workstation granted 2015..2024 | 2026-11-13 | 9 | US8145496; older delay patents |
| Web (general) | 9. Eventide OR Strymon OR "Native Instruments" OR "Universal Audio" OR Boss patent delay effect tempo sync ping-pong feedback filter digital delay pedal claims | 2026-11-13 | 10 | No patent; manuals |
| Web (general) | 10. patent "delay" effect "tempo" "dotted" OR "triplet" "note value" delay time calculated sampling frequency frames audio effect apparatus | 2026-11-13 | 10 | No patent |
| Web (general) | 11. patent smooth change of delay time digital audio delay "delay time" changes "crossfade" old delay new delay tempo change echo effect | 2026-11-13 | 10 | US4644400 |
| Web (general) | 12. patent delay effect feedback "soft clipping" OR "saturation" OR "nonlinear" element in feedback loop delay line low-pass "analog delay" emulation digital bucket brigade | 2026-11-13 | 10 | No patent; code and plugin pages |
| Web (general, extended) | 13. Google Patents "ping-pong" delay effect stereo echo feedback each channel fed to the other digital delay patent Roland OR Yamaha OR Korg OR Line 6 OR Alesis | 2026-11-13 | 9 | No ping-pong patent; US5894523 (microphone feedback, unrelated) |
| Web (general) | 14. patent delay time "tempo" automatically changes delay time when tempo changes smoothly crossfade audio effect DAW plug-in delay "tempo change" | 2026-11-13 | 13 | No patent; products |
| Google Patents (page fetch by number) | US6351540B1, US9711119B2, US5430243A, US8134644B2, US5742532, WO1998010355A1, US5781461A, US4644400A, US5920842A, US4244262A, US8145496B2, EP1681669A1, WO2008125582A1, US20100195840A1, US10644658B2, US7259316B2, US5811707A, US3681531A, US4348754A | 2026-11-13 | 19 | Dates and status read; claims read only for US5430243A (summary), US9711119B2 (claim 1), US8134644B2 (partly), US6351540B1 (claims 1, 14 summary), US5742532 and WO1998010355A1 (summaries) |
| Google Patents (cited-by list) | US5430243A "Cited By" tables (17 documents) | 2026-11-13 | 17 | US9711119B2, US7259316B2, US5811707A read; others unrelated by title |
| Other pages fetched | Csoundjournal Summer 1999; Kurzweil K2500 manual page 40 | 2026-11-13 | 2 | Read |
| Google Patents (claims, stage 2) | US9711119B2 page from character 200000: claims 1, 9, 10, 12 quoted clause by clause, then key clauses of 1, 10, 12 rechecked against the page in a second fetch | 2026-11-13 | 1 | Claim text for "Claim analysis" (text, not OCR) |
| Granted PDF (stage 2) | patentimages.storage.googleapis.com/3e/4e/50/75fc265b029463/US9711119.pdf | 2026-11-13 | 1 | Scanned images (CCITT fax), no text layer; not OCR'd, not used for quotes |

Refused or failed: a Google Patents search-results page fetch (`patents.google.com/?q=...`)
returned only the page heading with no results. A fetch of the Google Patents query endpoint
(`patents.google.com/xhr/query?...`) was refused by the permission classifier and was not retried.
The ICMC 1997 paper page (quod.lib.umich.edu) returned HTTP 403. A fetch of
`fransabsil.nl/archpdf/tandelay.pdf` returned undecodable PDF data, so its author and date are not
established. Google Patents pages for US9711119B2, US8134644B2, US8145496B2, US5920842A, US10644658B2,
WO2008125582A1 and US5811707A were truncated at 100000 characters; for US9711119B2 the claims were
read from character 200000, for US8134644B2 from 100000 to 200000. Not tried: Espacenet, USPTO
Patent Public Search and Patent Center, FreePatentsOnline, any search by CPC class (for example G10H
1/00, G10H 1/0091, G10H 1/46, G10H 2210/281, G10K 15/12, H04S) or by assignee name in a database.
Cited-by and citation lists were read only for US5430243A and (names only) US9711119B2 and US6351540B1.

## Open uncertainties

- No patent-database search was run (see "Refused or failed"). "No candidate found" for elements 1,
  3 to 7 is a statement about web-search coverage, not about the databases. The single candidate
  count reflects that limit; searches by CPC class and by assignee (Roland, Yamaha, Boss, Line 6,
  Eventide, Strymon, Lexicon, TC Electronic, Native Instruments, Ableton, Steinberg) are the main
  gap.
- Element 4 (offset of -50 to +50 ms on one side): no source or patent found.
- Element 3: the linear shape of the crossfade and its fixed ramp time, and the crossfade of both a
  time change and a tempo change, were not matched to any source opened. The NEC, Stanford and
  Pixel Instruments art is expired, but later patents citing them were not searched.
- Element 2: US9711119B2 (row 1) does not read, but the verdict assumes the editor keeps one clock
  for the effect and the transport and never adjusts playback timing or speed to follow an effect.
  A human should confirm that, and re-check the patent if the editor ever adds a sequencer or
  transport that runs on its own clock and is corrected against the audio sample clock (claims 1,
  9, 10), or tempo or playback that follows an effect's timing (claim 12). The claim text came from
  the Google Patents text through a fetch tool, cross-checked once; the granted PDF was not OCR'd,
  so a human may want to compare against USPTO Patent Center. The figure descriptions were not
  read.
- Element 5: no dated primary source for ping-pong earlier than Csound Magazine Summer 1999; the
  Kurzweil and E-MU manuals are undated.
- Element 6: the combination (low-pass, then saturation, then gain, then added to the input) was
  matched only to US6351540B1 (expired) in a general sense; its claims were seen as a summary.
  Later patents citing US6351540B1 were read by title only.
- US8145496B2 (Stutter Edit) is in term and was not listed; a human may wish to list it for its
  tempo-synced delay line, mentioned only in its description.
- Term end dates are as Google prints them; terminal disclaimers and adjustments not otherwise
  seen. The Casio estimate 2036-12-13 is filing date plus 20 years with no adjustment printed.
