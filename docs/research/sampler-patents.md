# Patent status of the sampler instrument's playback, zones and choke groups

Research note, 2026-11-16. This is not legal advice. It records what primary sources show, so a
reviewer can check it. Stage 1 listed the candidates; Chris Paro marked all eight In force
(2026-11-16); stage 2 (claim reading, 2026-11-16) is under Claim analysis. Chris Paro then found row 5 expired
(fee related) and read row 6's claims: it does not read. Dates in the Candidates
table are as Google Patents prints them (Google states that its status and expiry fields are
assumptions, not legal conclusions).

## Planned design

A polyphonic sampler instrument node (issue #174) that plays the user's own WAV files, mapped to
MIDI notes. Each element as it will be built:

1. **Varispeed playback.** Each voice keeps a playhead as a fractional frame position (`f64`) and
   advances it by a constant step per output sample:
   `step = 2^((note − root + tune + bend · range) / 12) · sr_file / sr_project`. Imported files are
   already resampled to the project rate, so `sr_file / sr_project = 1`. There is no
   time-stretching and no formant preservation: pitch and duration change together.
2. **Interpolated reads.** The sample is read at the fractional playhead with 4-point (cubic)
   Hermite interpolation over the frames `⌊p⌋−1 … ⌊p⌋+2`; frames before the start or past the end
   read as zero. This is the same kernel the delay line uses (cleared in ADR-0120,
   `docs/research/modulation-patents.md` element 5). No anti-alias filtering is applied when pitching up.
3. **Key-range zones.** A zone holds a low key, a high key, a root key, a playback mode, an
   optional choke group, and a mono or stereo sample held fully in memory (capped at 60 s). A
   note-on plays the first zone whose `low ≤ note ≤ high`; a note in no zone is ignored. There is
   no velocity layering, no round-robin and no crossfading between zones.
4. **One-shot or gated playback.** A one-shot zone ignores note-off and plays to the end of the
   sample. A gated zone releases through the amplitude ADSR on note-off. Either way a voice frees
   itself when its playhead passes the end of the sample. There are no loop points.
5. **Choke groups.** Groups are numbered 1–16. A note-on in group g fades out every other sounding
   voice in group g with a linear fade over about 3 ms (the open/closed hi-hat case).
6. **Velocity to gain.** Gain is `1 − amount + amount · velocity / 127`, with `amount` a 0–1
   parameter.
7. **Voice allocation, envelope and sustain pedal.** Reused unchanged from the synth (issue #173):
   a fixed pool of 16 voices, free voice first, then the quietest releasing voice, then the oldest
   held one, with a 3 ms steal fade; an exponential-segment ADSR; CC 64 holds released notes.
   Cleared in ADR-0148 (`docs/research/synth-patents.md`); no new research.

Output is stereo; a mono zone is written to both channels.

## Summary

Stage 2 (claim reading) done 2026-11-16. Six of the eight in-force candidates had their
independent claims read from Google Patents' HTML text, and none reads on the Planned design:
rows 1, 2, 3, 4, 7 and 8 **do not read** (row 3 also with its dependent claims 6 and 15). Rows 5 and
6 (Intelliterran) could not be read here; Chris Paro found row 5 **expired** (fee related) and read
row 6: it **does not read** (see Candidates 5 and 6). Per-claim reading under Claim analysis. The basic sampler
art (pitch by address increment with interpolation, 1984 to 1992; one-shot waveforms, 1991) is
expired. "No candidate found" means only that the searches in the Search log found none; no
database that lists by class was queried (see "Refused or failed").

| Technique | Verdict | Confidence |
|---|---|---|
| 1. Varispeed: playhead stepped by `2^(semitones/12)` from a root key | **Does not read** row 1 (US10224062B1, Microsoft, to 2038-04-30): claims 1, 8, 15 need coefficient sets for two filters and a third set interpolated between them by pitch-related fractions; the design reads every pitch with one fixed Hermite kernel. **Does not read** row 7 (US8035021B2, Yamaha, to 2030-04-15): claim 1 needs page-wise burst reads from non-volatile memory into a buffer behind prestored first pages; the design holds each sample whole in memory. **Does not read** row 8 (US9514724B2, Casio, to 2035-03-23): claims 1, 9, 10, 11 need sampling from a sound receiver and then playing a preset pattern or phrase with the sampled tone; the design plays imported files and auto-plays nothing. Expired art from 1984 (US4667556A), 1990 (US5111727A, US6365816B1). | High for the claims read |
| 2. 4-point Hermite read, zero outside the sample | **Does not read** row 1 (above) or row 7 (the design reads Hermite from memory, with no paging). Kernel cleared by ADR-0120 (`docs/research/modulation-patents.md` element 5). Import resampling to the project rate is ADR-0036's fixed-ratio polyphase filter, which has no interpolated coefficients either. | High |
| 3. Key-range zones with root key, first match, no layering | **Does not read** row 2 (US9330649B2, Apple, to 2034-03-19): claims 1, 7, 13 need several samples for different velocity levels and, on a repeat at the same velocity, a sample of a second velocity level; the design picks a zone by key only and plays the same zone every time. **Does not read** row 3 (US8461445B2, Yamaha, to 2029-09-11): claims 1, 18, 19 need physical playing pads grouped into an enlarged zone whose timbre is chosen from per-pad timbres by highest-priority pad number; a zone is a MIDI key range holding its own sample. No other patent found claiming key-range zones or root keys (searches 1, 2, 7, 8, 9). | High for the claims read |
| 4. One-shot or gated (note-off through ADSR), free at end | No in-term candidate found (searches 5, 6, 15); none of the claims read recites it. Earlier art: US5288940A (Kawai 1991, expired); US20090260505A1 (Samsung 2008, abandoned application) describes the same modes. | Low (search only) |
| 5. Choke groups 1 to 16, 3 ms linear fade | **Does not read** row 3: claims 6 and 15 recite a "special playing technique" (choke is one in the description, not in any claim) set on a designated instruction pad, and depend on claim 1, which does not read. **Does not read** row 4 (US9076264B1, iZotope, to 2031-10-15): claims 1, 8, 12 are a step-sequencer pattern/part display; choke is in the description only. Row 5 (US11688377B2) **expired** (fee related). **Does not read** row 6 (US12159610B2): its claims interpret the duration and frequency of taps on a foot-operable switch to control loops or other programming signals; the sampler is software only, with no foot switch. Art: SoundFont exclusiveClass (2.04 spec text), Akai MPC mute groups (manuals, undated). | High for rows 3, 4 and 6 |
| 6. Velocity to gain, `1 − a + a·v/127` | No candidate claims it (search 10). Row 2 reads velocity only to select among velocity-level samples, which the design lacks. | Low (search only) |
| 7. Voice allocation, ADSR, sustain | Cleared by ADR-0148 (`docs/research/synth-patents.md`); not re-searched. | n/a |

## Prior art

Items marked (unverified) come from a search summary or a second-hand quotation, not from the
publication itself. I could not read any PDF (the fetch tool returned binary for the SoundFont
specification PDFs).

- **Element 1 (pitch by stepping through stored samples, a root pitch at step 1).**
  - Fairlight CMI, introduced 1979 ([Wikipedia](https://en.wikipedia.org/wiki/Fairlight_CMI)): the
    page says Vogel found that "by playing the recording back at different pitches" it sounded more
    realistic than synthesis; it does not say how pitch was produced (Series I used a variable
    sample rate of 24 kHz to 8 kHz, not tied to pitch there). No dated Fairlight primary source
    seen; no Fairlight patent found (search 11).
  - E-mu Emulator, released 1981 ([Wikipedia](https://en.wikipedia.org/wiki/E-mu_Emulator)); the
    page does not say how it pitches samples.
  - US4667556A (Casio), priority 1984-08-09, filed 1985-07-29, granted 1987-05-26: external sound
    sampled into waveform memory; the pitch set by the depressed key controls the rate at which the
    address advances, so one note (C1 in the example) plays across the keyboard; loop points and
    four channels; no key zones
    ([Google Patents](https://patents.google.com/patent/US4667556A/en)).
  - US5111727A (E-mu; Rossum), priority and filing 1990-01-05, granted 1992-05-12: pitch shifting
    by stepping through the waveform with a fractional step and interpolating 7 or 8 surrounding
    points, with notched filter responses for upward shifts
    ([Google Patents](https://patents.google.com/patent/US5111727A/en)). Same application family:
    US6365816B1 (Creative), a per-sample "phase increment" where 1 means no shift, claim 1 an
    interpolator between two adjacent samples
    ([Google Patents](https://patents.google.com/patent/US6365816)); its sibling US6858790 was
    returned by a search but not opened.
  - US5218155A (Kawai), priority 1990-03-30: the fractional part of the read address is the
    interpolation coefficient between two adjacent points
    ([Google Patents](https://patents.google.com/patent/US5218155A/en)); US5252773 and US5146834
    were returned by a search for similar address-increment and loop-interpolation art, not opened.
  - US5698807A and US5248845A (E-mu; Massie, Rossum), priority 1992-03-20: transposing a note by
    playback speed shifts its formants ("munchkinization"); both describe separating formant and
    excitation to widen the range
    ([Google Patents](https://patents.google.com/patent/US5698807),
    [Google Patents](https://patents.google.com/patent/US5248845A/en)). This is the design's
    "no formant preservation" limit, stated as the known cost of varispeed.
- **Element 2 (4-point Hermite read).** See `docs/research/modulation-patents.md` "Prior art",
  element 5 (Catmull and Rom 1974; Niemitalo 2001; Dattorro 1997; no patent found claiming
  Hermite), and ADR-0120. Sample-playback-specific: US5111727A (above) interpolates with 7 or 8
  points, not Hermite. No Hermite-specific sampler source found (search 13).
- **Element 3 (key-range zones, root key, a sample per range).**
  - E-mu Emulator, 1981: built for split mode, a separate sample on each side of the keyboard
    ([Wikipedia](https://en.wikipedia.org/wiki/E-mu_Emulator)).
  - Kurzweil K2600 and PC3 manuals describe keymaps of key ranges, each with a sample root, and
    note that more ranges mean less pitch shifting of each root
    ([K2600](https://www.manualslib.com/manual/359934/Kurzweil-K2600.html?page=421)); the K2600
    and PC3 pages show no date (the K2600 manual's date was not seen).
  - SoundFont 2: the 2.04 specification is dated 2006 (per Wikipedia,
    [SoundFont](https://en.wikipedia.org/wiki/SoundFont), citing
    [sfspec24.pdf](https://www.synthfont.com/sfspec24.pdf)); a search summary of the Polyphone copy
    ([PDF](https://polyphone.io/doc/files/sf_specifications_v2.04.pdf)) lists revisions 2.00a
    (October 1995), 2.00b (May 1997), 2.01 (August 1997) and 2.04 (September 2002) (unverified;
    the PDF text was not readable by me, so I did not confirm that `keyRange` and
    `overridingRootKey` are in 2.00a). Wikipedia dates 2.0 to 1996 and 2.01 to 1998, and says the
    format was first used on the Sound Blaster AWE32 in 1994.
  - EP1646035B1 (Sony Europe), priority 2004-10-05: audio units mapped to keys by pitch, an E-flat4
    unit assigned to its key and neighbours with pitch transposition for the others, velocity
    selecting among units; Google prints "Expired - Lifetime", 2024-10-05
    ([Google Patents](https://patents.google.com/patent/EP1646035B1/en)). Its US application
    11/243,003 (2005-10-04) is named on the page; I did not look it up (Open uncertainties).
  - Current products, undated: [Native Instruments Maschine zones](https://native-instruments.com/ni-tech-manuals/maschine-mk3-manual/en/sampling-and-sample-mapping)
    (key and velocity range per zone), [Kontakt root key](https://native-instruments.com/ni-tech-manuals/kontakt-manual/en/the-mapping-editor),
    [Apple EXS24 zone parameters](https://help.apple.com/mainstage/mac/2.2/en/mainstage/logicproinstruments/chapter_11_section_14.html)
    (notes outside a zone's key range do not trigger it), [Ableton Sampler key zones](https://help.ableton.com/hc/en-us/articles/115001318670-How-To-Multisampling-with-Sampler).
    None is dated in the pages seen; none is cited as prior art.
- **Element 4 (one-shot versus gated).**
  - US5288940A (Kawai), priority 1991-02-15, granted 1994-02-22: one read circuit for one-shot and
    sustained tones, a looped silent second segment making the tone end as a one-shot; Google prints
    "Expired - Lifetime", 2012-02-13
    ([Google Patents](https://patents.google.com/patent/US5288940)).
  - US20090260505A1 (Samsung), priority and filing 2008-04-16, published 2009-10-22: the
    description has "loop forward" and "one shot" wave types, a one-shot being as if the note-off
    came right after the start; abandoned 2010-05-10 for failure to respond to an office action;
    claim 1 only recites a wave stored in two continuous parts with a discontinuity
    ([Google Patents](https://patents.google.com/patent/US20090260505)). The page shows no granted
    family member.
  - US4628790 (percussion sounds as attack followed by release) and US5248842 (forced release on
    key-off) were returned by a search; not opened.
  - Product manuals, undated: [Roland SP-404 MK2 one-shot](https://static.roland.com/manuals/sp-404mk2_reference_v550/en-US/7887911578451851.html),
    [Nektar DP-1](https://nektartech.com/nektarine/dp-1-drum-player/).
- **Element 5 (choke or exclusive groups).**
  - SoundFont `exclusiveClass`: the 2.04 specification, quoted second-hand: "when this note is
    initiated, any other sounding note with the same exclusive class value should be rapidly
    terminated" ([fluid-dev post, 2016](https://lists.nongnu.org/archive/html/fluid-dev/2016-09/msg00000.html)).
    Whether it is in 2.00a (1995) or only later is not established.
  - Linn-9000 (1984) and Akai MPC60 (1988): [Sound on Sound](https://www.soundonsound.com/music-business/akai-mpc60-revisited)
    gives the dates and says the MPC60 has a "programmable hi-hat decay slider" from the Linn-9000;
    it does not mention mute groups, so I cannot date mute groups to either machine.
  - Akai MPC1000 manual, undated: pads in the same mute group silence each other, with the
    open/closed hi-hat as the example ([manual](https://www.manualslib.com/manual/268323/Akai-Mpc-1000.html?page=84),
    reported by a search summary; page not opened).
  - US5262585 (electronic cymbal system, hi-hat choke contacts sending note-off) was returned by a
    search; not opened. By its number it was granted around 1993.
- **Element 6 (velocity to gain).** US4558623A, "Velocity and aftertouch sensitive keyboard",
  higher amplitude for higher key velocity ([Google Patents](https://patents.google.com/patent/US4558623A/en);
  from a search summary, dates not seen). EP1646035B1 (above) describes sample-based keyboards
  playing at a volume corresponding to key velocity. MIDI 1.0 (1983) not opened. No source for the
  exact `1 − amount + amount·velocity/127` form.
- **Element 7.** `docs/research/synth-patents.md` "Prior art", elements 2, 4 and 5; ADR-0148.

## Candidates

Patents that might cover an element of the design and might still be in force. Status is filled
in by a human from Google Patents or USPTO Patent Center. The claim notes in this table are from
stage 1 (claims seen in pieces, or not at all); stage 2's reading is under Claim analysis.

| # | Patent | Assignee | Priority / filing | Est. term end | Element it may touch | Link | Status |
|---|---|---|---|---|---|---|---|
| 1 | US10224062B1, Sample rate conversion with pitch-based interpolation filters. Claim 1 (seen in pieces): a pitch value from the input and output sampling rates; coefficients for two filters from a fractional address; a third set by interpolating between them; filter the input samples. | Microsoft Technology Licensing LLC (inventor Savell) | 2018-02-02 / 2018-04-30; granted 2019-03-05 | 2038-04-30 (Google, anticipated; no adjustment shown) | 1, 2 (pitch ratio set by the rate relationship; interpolated reads; the design steps by a note-derived ratio and reads with Hermite, with no coefficient interpolation) | [GP](https://patents.google.com/patent/US10224062B1/en) || In force (Google Patents "Active"; checked by Chris Paro, 2026-11-16) |
| 2 | US9330649B2, Selecting audio samples of varying velocity level. Claim 1 (pieces): play a sample for a first velocity level, on a repeat of the same velocity select a sample of a second velocity level. | Apple Inc (inventors Buskies, Gros) | 2013-07-12 / 2013-08-13; granted 2016-05-03 | 2034-03-19 (Google, adjusted) | 3, 6 (velocity-selected samples; the design has no velocity layers or round-robin and only scales gain) | [GP](https://patents.google.com/patent/US9330649B2/en) || In force (Google Patents "Active"; checked by Chris Paro, 2026-11-16) |
| 3 | US8461445B2, Electronic percussion instrument having groupable playing pads. Independent claims 1, 18, 19 combine adjacent pads into one zone with a representative timbre; choke and mute instructions only in dependent claims 6 and 15 (seen in a paraphrase). | Yamaha Corp (inventors Takehisa, Watanabe, Miyashita, Sato) | 2008-09-12 / 2009-09-11; granted 2013-06-11 | 2029-09-11 (Google, anticipated; no adjustment shown) | 5 (choke or mute instruction), 3 (zones of pads sharing a timbre) | [GP](https://patents.google.com/patent/US8461445B2/en) || In force (Google Patents "Active"; checked by Chris Paro, 2026-11-16) |
| 4 | US9076264B1, Sound sequencing system and method. Claims 1, 8, 12 are a step-sequencer display and playback; "choke groups" appear in the description only ("one sound ... cut off by the start of another"). | iZotope Inc, now Native Instruments USA Inc (inventors Gillespie, Regan, Todd) | 2009-08-06 / 2009-08-06; granted 2015-07-07 | 2031-10-15 (Google, adjusted) | 5 (description only; claims do not recite it) | [GP](https://patents.google.com/patent/US9076264) || In force (Google Patents "Active"; checked by Chris Paro, 2026-11-16) |
| 5 | US11688377B2, Synthesized percussion pedal and docking station. Description: a choke group makes a superseding instrument mute a preceding one (closed hi-hat chokes the open hi-hat sample). Claims not seen. | Intelliterran Inc (inventor Packouz) | 2013-12-06 / 2021-03-24; granted 2023-06-27 | 2034-03-17 (Google, anticipated; no adjustment shown) | 5 (choke group; claims unread) | [GP](https://patents.google.com/patent/US11688377B2/en) || Expired, fee related (checked by Chris Paro, 2026-11-16; first marked "Active" the same day) |
| 6 | US12159610B2, same family and title as row 5. The abstract is about repeating MIDI segments by a foot switch (per a search summary); choke not found in the text I read. Claims not seen. | Intelliterran Inc | 2013-12-06 / 2023-06-27; granted 2024-12-03 | 2034-03-17 (Google, anticipated; no adjustment shown) | 5 (family of row 5; claims unread) | [GP](https://patents.google.com/patent/US12159610B2/en) || In force (Google Patents "Active"; checked by Chris Paro, 2026-11-16); claims read by Chris Paro: does not read |
| 7 | US8035021B2 (publication US20100236384A1), Tone generation apparatus. Abstract: waveform data read page by page from NAND flash into a waveform buffer; the description has a pitch counter with integer and decimal parts and two-point linear interpolation. Claims not seen. | Yamaha Corp (inventors Shirahama, Shirakawa, Fujita) | 2009-03-23 / 2010-03-22; granted 2011-10-11 | 2030-04-15 (Google, adjusted) | 1, 2, 3 (sample waveform playback by a pitch counter; the design holds the whole sample in memory and reads Hermite) | [GP](https://patents.google.com/patent/US20100236384) || In force (Google Patents "Active"; checked by Chris Paro, 2026-11-16) |
| 8 | US9514724B2, Sampling device, electronic instrument, method, and program. Claim 1 (pieces): sample sound from a microphone, then automatically play a stored rhythm pattern or phrase using the sampled tone; dependent claim 8 has keys specifying pitches. | Casio Computer Co (inventors Setoguchi, Ishioka) | 2014-04-25 / 2015-03-23; granted 2016-12-06 | 2035-03-23 (Google, anticipated; no adjustment shown) | 1, 3 (playing a sampled tone at selected pitches; the design plays imported files and does not sample or auto-play a preset pattern) | [GP](https://patents.google.com/patent/US9514724) || In force (Google Patents "Active"; checked by Chris Paro, 2026-11-16) |

Status values: **In force**, **Lapsed (fees)**, **Expired (term)**, **Abandoned**, **Invalidated**,
**Not found**. Record who checked and when, under the table.

Expired by term (filing date + 20 years, plus any printed term adjustment, is in the past), listed
for completeness and needing no check. Dates are as Google Patents prints them.

- [US4202234A](https://patents.google.com/patent/US4202234A/en), National Research Development
  Corp, filed 1977-04-27, expiry 1997-05-13. Digital note generator, pitch set by the sampling rate
  (element 1, early).
- [US4667556A](https://patents.google.com/patent/US4667556A/en), Casio, filed 1985-07-29, expiry
  2005-07-29. Sampled external sound, pitch by address-advance rate per key (element 1).
- [US5111727A](https://patents.google.com/patent/US5111727A/en), E-mu, filed 1990-01-05, expiry
  2010-01-05. Interpolating multichannel sampler with a fractional step (elements 1, 2).
- [US6365816B1](https://patents.google.com/patent/US6365816), Creative, priority 1990-01-05, filed
  2000-07-19, expiry 2010-01-05 (lapsed for fees 2014). Phase increment and interpolator with a
  cache (elements 1, 2).
- [US5218155A](https://patents.google.com/patent/US5218155A/en), Kawai, filed 1990-03-29, expiry
  2010-06-08 (lapsed for fees 2005-07-06). Fractional read address as interpolation coefficient
  (element 2).
- [US5288940A](https://patents.google.com/patent/US5288940), Kawai, filed 1993-01-14, priority
  1991-02-15, expiry 2012-02-13. One-shot and sustaining waveforms (element 4).
- [US5248845A](https://patents.google.com/patent/US5248845A/en), E-mu, filed 1992-03-20, expiry
  2012-03-20, and [US5698807A](https://patents.google.com/patent/US5698807), Creative, priority
  1992-03-20, filed 1996-03-05, expiry 2012-03-20. Formant-preserving transposition, the feature the
  design omits (element 1).
- [EP1646035B1](https://patents.google.com/patent/EP1646035B1/en), Sony Europe, filed 2004-10-05,
  expiry 2024-10-05. Keyboard mapping of audio units with transposition and velocity selection
  (elements 3, 6). The US application's status is open (see Open uncertainties).
- [DE69733038T2](https://patents.google.com/patent/DE69733038T2/en), Yamaha, filed 1997-07-30,
  expiry 2017-07-31. Software tone generator with note-number read addresses (element 1).

## Claim analysis

All eight candidates were marked In force by Chris Paro (2026-11-16). Every claim text below comes
from the Google Patents HTML page of the grant, via a fetch tool that returns quotes only in pieces
of under 125 characters; each claim is its pieces joined, and a connecting word between pieces may
be missing. No page says whether its text is OCR; none of the quotes used below shows OCR-style
errors except where noted ("timber" for "timbre" in row 3, which may be in the grant itself). No
grant PDF was read (the two fetched were image-only; see Search log). Compare with the USPTO grant
text before relying on any quote.

### Candidate 1: US10224062B1 (Microsoft)

Source: [Google Patents](https://patents.google.com/patent/US10224062B1/en), second text window
(offset 100000). 20 claims; independent 1, 8, 15. Priority 2018-02-02, filed 2018-04-30, granted
2019-03-05; Google: Active, anticipated expiry 2038-04-30.

**Claim 1** (method):

| Element | Planned design |
|---|---|
| "A method comprising: obtaining a pitch value based at least on a relationship between an input sampling rate of input samples corresponding to a first audio signal to an output sampling rate of output samples corresponding to a second audio signal;" | Element 1's `step` is a pitch ratio; it contains `sr_file / sr_project` (always 1). Arguably met: the step is the rate at which file frames are consumed per output frame. |
| "using a fractional address to generate a first set of interpolated coefficient values corresponding to a first filter and to generate a second set of interpolated coefficient values corresponding to a second filter;" | **Not met.** Element 2 computes the four Hermite weights from `p − ⌊p⌋` (a fractional address) directly as polynomials: one kernel, not two filters, and no coefficients interpolated from a table. At most a "first set" on a broad reading; no "second filter". |
| "automatically generating a third set of coefficient values by interpolating between the first set of interpolated coefficient values and the second set of interpolated coefficient values using a set of fraction values related to the pitch value, wherein the set of fraction values is determined based on an analysis of a relationship between step changes in the fraction values and corresponding filter responses associated with at least one of the first filter or the second filter; and" | **Not met.** The kernel does not change with pitch; element 2 applies no anti-alias filtering when pitching up, so there is nothing to blend by pitch and no analysis of step changes. |
| "filtering data corresponding to the input samples using at least one of the first set of coefficient values, the second set of coefficient values, or the third set of coefficient values to generate the output samples." | The Hermite read is a 4-tap filter over the sample. Met only if the earlier sets existed. |

Verdict: **Does not read**. Missing: the "second filter" set and the "third set of coefficient
values" interpolated between the two by pitch-related fractions.

**Claim 8** (device, "a sample rate converter configured to:") has the same steps, with the first
and second sets made "by interpolating between a first set of coefficient values corresponding to a
first filter" and likewise for a second filter. **Claim 15** (system) adds "a buffer configured to
store input samples", "a sample rate converter; and" "an audio control processor", the pitch value
coming "from the audio control processor", then the same steps. Both miss the same elements.
Verdict: **Does not read**.

Import resampling (element 1's "already resampled to the project rate") is ADR-0036's exact
rational polyphase filter: one Kaiser-windowed sinc, phases precomputed, "Coefficients are never
interpolated or computed at run time". It also lacks the second and third sets.

**Prior art predating 2018-02-02:** US4667556A (1984), US5111727A (1990, fractional step with 7- or
8-point interpolation and notched responses for upward shifts), US6365816B1, US5218155A (all under
Prior art); Catmull and Rom 1974 for the Hermite kernel (via ADR-0120).

**Verdict: Does not read** (claims 1, 8, 15; High confidence). Adding pitch-dependent anti-alias
filtering by blending two filters' coefficients would need a fresh reading before 2038-04-30.

### Candidate 2: US9330649B2 (Apple)

Source: [Google Patents](https://patents.google.com/patent/US9330649B2/en), second text window
(offset 100000). 18 claims; independent 1, 7, 13. Priority 2013-07-12, filed 2013-08-13, granted
2016-05-03; Google: Active, adjusted expiry 2034-03-19.

**Claim 1** (computer-implemented method):

| Element | Planned design |
|---|---|
| "A computer-implemented method, comprising: receiving, by a computing device, a first instance of a musical stimulus having a first velocity level;" | A MIDI note-on with a velocity reaching the sampler node. Met. |
| "playing back a first audio sample corresponding to the first velocity level of the received musical stimulus, wherein the first audio sample is one of a plurality of audio samples that correspond to different velocity levels of the musical stimulus;" | **Not met.** Element 3 picks the zone by key only (first zone with `low ≤ note ≤ high`); a zone has no velocity range, so no sample "corresponds" to a velocity level. Velocity only scales gain (element 6). |
| "receiving a second instance of the musical stimulus having the first velocity level;" | The same note again at the same velocity. Met. |
| "selecting, from the plurality of audio samples, a second audio sample corresponding to a second velocity level of the received musical stimulus, wherein the second audio sample includes different audio characteristics than the first audio sample; and" | **Not met.** First match is deterministic: the same note always plays the same zone's sample, whatever the velocity or repetition. No round-robin. |
| "playing back the second audio sample." | Not met (no second sample is selected). |

Verdict: **Does not read**. Missing: "a plurality of audio samples that correspond to different
velocity levels" and selecting a second velocity level's sample on a repeat. Overlapping zones do
not change this: the first match always wins.

**Claims 7** (system: "one or more data processors; and" storage media with instructions) and
**13** (computer-program product) have the same operations word for word ("receiving a first
instance ...", "selecting, from the plurality of audio samples, a second audio sample corresponding
to a second velocity level ..."). Verdict: **Does not read**, same missing elements.

**Prior art predating 2013-07-12:** EP1646035B1 (Sony, priority 2004-10-05; velocity selecting
among audio units), US4558623A (velocity to amplitude; dates not seen).

**Verdict: Does not read** (claims 1, 7, 13; High confidence). Velocity layers combined with any
alternation on repeated notes (round-robin, or borrowing a neighbouring layer) need a fresh reading
before 2034-03-19.

### Candidate 3: US8461445B2 (Yamaha)

Source: [Google Patents](https://patents.google.com/patent/US8461445B2/en), second text window
(offset 100000). 20 claims; independent 1, 18, 19. Claim 6 depends on 1; claim 15 on 8, which
depends on 1 (8: "the plurality of playing pads are configured as a two-dimensional pad device
constituted by a planar manipulating face and a three-dimensional pad device constituted by a
protruded manipulating face."). Priority 2008-09-12, filed 2009-09-11, granted 2013-06-11; Google:
Active, anticipated expiry 2029-09-11. The claims write "timber" for "timbre" in places.

**Correction to stage 1:** no claim contains "choke", "choking", "mute" or "muting" (asked of the
whole claims section). Claims 6, 7, 15, 16 and 17 recite a "special playing technique"; the
description gives choking ("a playing technique of stopping or rapidly damping the musical tones
being generated") and muting ("generating musical tones in a muted timbre") as examples.

**Claim 1** (instrument):

| Element | Planned design |
|---|---|
| "An electronic percussion musical instrument comprising:" | Debussy with a sampler node. Assume met for the analysis. |
| "a music playing manipulation unit including a plurality of playing pads arranged so that adjacent playing pads border and confront each other along their boundaries, each of the plurality of playing pads constituting a manipulating face zone for playing music by manipulating the face zone;" | **Not met.** The sampler has no pads: it receives MIDI note events (from a CoreMIDI device or a clip). An external MIDI pad controller is not part of the design. |
| "a group setting unit that sets a grouped pad zone by combining at least two adjacent playing pads among the plurality of playing pads to provide a virtually enlarged area of the manipulating face zone;" | **Not met.** A zone's key range (element 3) makes several MIDI keys play one sample, but it groups note numbers, not pads, and enlarges no physical face. |
| "a storage unit that stores plural timbres of musical tones and information regarding identification numbers given to the respective playing pads;" | Zones store samples; no pad numbers. Not met as to pads. |
| "a musical tone allocating unit that allocates different timbres of musical tones stored in the storage unit to individual playing pads;" | Not met (no pads). |
| "a representative timber setting unit that collectively sets as a group, to all of the playing pads belonging to the set grouped pad zone, a same representative timber of musical tone among the plural timbres of musical tones allocated to the playing pads belonging to the set grouped pad zone, based on an identification number with a highest priority given among the identification numbers of the playing pads belonging to the set grouped pad zone;" | **Not met.** A zone holds its own sample; it is not chosen from timbres already allocated to its members by a priority number. (First-match is a priority among zones, not among the members of one group.) |
| "a music playing manipulation detecting unit that detects a music playing manipulation performed onto each of the playing pads; and" | Not met (no pads). |
| "a musical tone generating unit that generates, for each of the playing pads other than the playing pads belonging to the set grouped pad zone, a musical tone in the timbre allocated thereto, and for all of the playing pads belonging to the set grouped pad zone, a same musical tone of the representative timbre, according to the manipulation of any of the playing pads as detected by the music playing manipulation detecting unit." | Not met as to pads and representative timbre. |

Verdict: **Does not read**. Missing: the "playing pads" and "grouped pad zone", and the
"representative timber" chosen by highest-priority identification number.

**Claim 18** (instrument with "a controller configured to:") and **claim 19** (method, reciting
the same units and then the steps "combining, with the group setting unit, at least two adjacent
playing pads ...", "allocating ...", "collectively setting, with the representative timbre setting
unit ...", "detecting ...", "generating ...") have the same elements. Verdict: **Does not read**.

**Claim 6** (as claimed in claim 1, "further comprising:"):

| Element | Planned design |
|---|---|
| "a playing technique effect imparting unit that imparts an effect of a special playing technique to the musical tone to be generated in response to a music playing manipulation on at least one playing pad among the plurality of playing pads; and" | Element 5's choke fade could be called an effect on other voices, but there are no playing pads. |
| "a playing technique instruction pad setting unit that sets a playing pad among the plurality of playing pads as a playing technique instruction inputting pad for inputting an instruction from a player to effect a special playing technique," | **Not met.** A choke group is symmetric: any note-on in group g fades the others. No pad (or key) is designated as an instruction input for another's technique. |
| "the playing technique instruction inputting pad being determined in association with the at least one playing pad allocated for the musical tone to which the effect of the special playing technique is to be imparted," | Not met, as above. |
| "wherein the musical tone generating unit generates a musical tone in response to a manipulation on the at least one playing pad when the playing technique instruction inputting pad inputs an instruction to effect the special playing technique by the player." | **Not met.** The claim generates the technique-affected tone of the other pad when it is struck while the instruction is input; a choke group generates the triggering note's own sample and fades the others. |

Verdict: **Does not read** (it carries claim 1's missing elements, and its own instruction-pad
element is absent). **Claim 15** has the same added text as claim 6, on claim 8 (two- and
three-dimensional pad devices, also absent). Verdict: **Does not read**.

**Prior art predating 2008-09-12:** E-mu Emulator (1981, split keyboard); SoundFont exclusiveClass
(in 2.04, 2002 per an unverified revision list); Linn-9000 (1984) and MPC60 (1988), whose mute
groups are not dated.

**Verdict: Does not read** (claims 1, 18, 19, and dependent 6 and 15; High confidence). A pad-grid
view that merges adjacent pads into one sound, or a designated choke or mute pad, needs a fresh
reading before 2029-09-11.

### Candidate 4: US9076264B1 (iZotope, now Native Instruments USA)

Source: [Google Patents](https://patents.google.com/patent/US9076264), first text window (offset
0). 15 claims; independent 1, 8, 12. Priority and filing 2009-08-06, granted 2015-07-07; Google:
Active, adjusted expiry 2031-10-15. The tool reported that no claim mentions choke groups, cutting
off, muting or stopping one sound when another starts; the description does.

**Claim 1** (user interface display):

| Element | Planned design |
|---|---|
| "A user interface display for an audio sequencing device, comprising:" | **Not met.** The sampler has no sequencer and no display of a sequence. |
| "a part view comprising a square matrix of squares, wherein the squares in the matrix visually indicate a sequence of when a specific sound is to be played, and wherein when the sound sequence is being played, the part view matrix squares are highlighted sequentially as the indicated specific sound is played; and" | Not met: no step grid. |
| "a pattern view comprising a square matrix of squares, wherein at least one square in the pattern view matrix displays a part view related to the pattern view, and wherein when the pattern view is displayed and a song is being played, the part view matrix squares are still highlighted sequentially as the indicated specific sound is played;" | Not met. |
| "wherein when a user is viewing the pattern view, the user may select one of the squares in the pattern view matrix, whereupon the selected part view will expand and be displayed instead of the pattern view." | Not met. |

Verdict: **Does not read**. Missing: every element; the decisive one is "a part view comprising a
square matrix of squares".

**Claim 8** (method) needs "displaying by a processor, a pattern view on a display device, wherein
the pattern view comprises a square matrix of squares, wherein at least one square within the
matrix displays a part view;", enlarging a part view to replace it, and "when in play mode,
sequentially highlighting each of the part view matrix squares, and simultaneously producing audio
output if the highlighted square indicates that the specific sound should be played". **Claim 12**
(non-transitory medium) has the same steps. The sampler has no pattern or part view. Verdict:
**Does not read**.

**Prior art:** none in this note bears on a step-sequencer display; not needed.

**Verdict: Does not read** (claims 1, 8, 12; High confidence). The claims cover a display, not
sound generation, so element 5 is outside them. A future step-sequencer with nested pattern and
part grids that highlight while playing needs a fresh reading before 2031-10-15 (MIDI clip editing
in the track pane was not assessed here).

### Candidates 5 and 6: US11688377B2 and US12159610B2 (Intelliterran)

Not read. Google Patents' HTML for both (477799 and 481558 characters) was read through every text
window from offset 0 to the end; the converted text stops inside the description's "Aspects" list
("Aspect 23") and contains no claims. The page itself says the Aspects "are not to be construed as
patent claims". The grant PDF of US11688377B2 (Google's patentimages copy and USPTO's
image-ppubs copy) is page images with no text layer, and no tool here could render it. Justia,
FreePatentsOnline, uspto.report, RPX Insight, PatentGuru and 9235.net refused (403, 468 or a login
page). The published application US20230343315A1 (row 6's) was also truncated before its claims.

What was seen: in the description, "a choke group is used to tell a superseding instrument to mute
the sound of a preceding instrument" (row 5; the tool placed the choke-group passage in the
software section on moving between percussion loops). No Aspect seen mentions choke groups. A web
search summary (RPX Insight, not opened) said claim 1 of family member US10546568B2 covers foot
switches triggering MIDI sequences and fill sequences plus a docking station; that is a different
patent and is not evidence about these claims.

**Verdict (Chris Paro, 2026-11-16).** Row 5, US11688377B2: **expired** (fee related), so not
in force. Row 6, US12159610B2: **does not read**. Its claims interpret the duration and frequency
of taps on a foot-operable switch to control loops or other programming signals; the sampler is a
software-only instrument driven by MIDI notes, with no foot switch, tap timing or loop control.

The original stage 2 request, kept for the record: read the independent claims of both from the USPTO grant text
(Patent Center or Patent Public Search). The question that decides them is whether any claim
recites a choke group (a later instrument muting an earlier one). If it does only as part of a
foot-switch, looper or MIDI-sequence apparatus, it likely does not read, since the sampler has no
foot switch, looper or sequence playback; that is a guess from the family's subject, not a reading.

### Candidate 7: US8035021B2 (Yamaha)

Source: [Google Patents](https://patents.google.com/patent/US8035021B2/en), second text window
(offset 100000). 10 claims; independent 1 only. Priority 2009-03-23, filed 2010-03-22, granted
2011-10-11; Google: Active, adjusted expiry 2030-04-15.

**Claim 1** (tone generation apparatus):

| Element | Planned design |
|---|---|
| "A tone generation apparatus comprising: a non-volatile first memory storing therein series of sample data of a plurality of waveform data, said first memory being capable of burst readout on a page-by-page basis, each of the pages comprising a plurality of sample data, the series of sample data of each of the waveform being stored in a plurality of successive pages in said first memory;" | **Not met.** Element 3: each zone's sample is "held fully in memory" (ADR-0154: loaded whole) in RAM before playback. The WAV file on disk is never read during playback. |
| "a second memory including a leading waveform region prestoring the sample data of first pages of individual ones of the plurality of waveform data, and a buffer region that sequentially buffers the sample data of pages, succeeding the first page, of the waveform data to be sounded;" | **Not met.** No split into prestored heads and a streaming buffer. |
| "a control register which, in response to a tone generation start for given waveform data, registers a leading waveform address ... and second-page position information indicative of a stored position, in said first memory, of a second page following said first page;" | Not met. A note-on sets the voice's playhead to 0 in the whole sample. |
| "a readout control section which, in response to the tone generation start instruction, first reads out, at a given readout rate, the sample data of the first page stored in the portion of the leading waveform region ... and then reads out, at a given readout rate, the sample data of the succeeding pages from the buffer region, said readout control section outputting information indicative of a progression of readout;" | Only the readout at a rate is met (the playhead advancing by `step`); no first-page region or buffer. |
| "a page address generation section which ... generates page address information that increments, with the second-page position information used as an initial value, in accordance with a progression of readout ...; and" | Not met. |
| "a transfer control section which instructs said first memory to burst read out the sample data of one of the second and succeeding pages ... and writes, into the buffer region of said second memory, the sample data of the page burst read out from said first memory ..." | Not met. |

Verdict: **Does not read**. Missing: the paged "non-volatile first memory" read during playback and
the "leading waveform region" with a "buffer region" behind it (the register, page address and
transfer sections all depend on them).

**Prior art predating 2009-03-23:** for the pitch-rate readout only, US4667556A (1984) and
US5111727A (1990). Nothing in this note bears on paged flash streaming.

**Verdict: Does not read** (claim 1; High confidence). Streaming zone samples from disk or flash
with preloaded heads (common in large sample libraries) would need a fresh reading before
2030-04-15. Project clip streaming (ADR-0008, ADR-0034) is outside this design and was not
assessed against this claim.

### Candidate 8: US9514724B2 (Casio)

Source: [Google Patents](https://patents.google.com/patent/US9514724B2/en), first text window
(offset 0). 11 claims; independent 1, 9, 10, 11. Priority 2014-04-25, filed 2015-03-23, granted
2016-12-06; Google: Active, anticipated expiry 2035-03-23.

**Claim 1** (sampling device):

| Element | Planned design |
|---|---|
| "A sampling device, comprising: a sound wave receiver configured to receive external sound wave data;" | The sampler has none: its zones load the user's WAV files. (Debussy records audio inputs on tracks, but that is not part of the sampler.) |
| "a processor connected to the sound wave receiver, the processor being configured to receive a sampling command from a user; and" | **Not met.** No sampling command. |
| "a memory connected to the processor, the memory storing a preset play data representing either a rhythm pattern including rhythm pattern data or a musical phrase including both a plurality of pitches and associated duration of the pitches," | **Not met.** The sampler stores no preset pattern or phrase. (User-made MIDI clips are not preset in the sampler.) |
| "wherein in response to the sampling command from the user, the processor executes: sampling the sound wave data received by the sound wave receiver to convert at least a part of the sound wave data to a digitized tone data;" | Not met. |
| "after the sampling, automatically without user interventions, reading out the preset play data from the memory; and" | **Not met.** No auto-play after loading or recording. |
| "thereafter, automatically without user interventions, playing back the preset play data that have been read out from the memory to the user using the digitized tone data as a tone for either the rhythm pattern or the musical phrase." | Not met. |

Verdict: **Does not read**. Missing: "a sound wave receiver" with a "sampling command", "preset
play data", and reading and playing it "automatically without user interventions" after sampling.

**Claims 9** (method) and **10** (storage medium) recite the same receiver, preset play data,
"receiving a sampling command from a user;" and the two "automatically without user interventions"
steps. Verdict: **Does not read**, same missing elements.

**Claim 11** drops "automatically" and "preset" but adds: "the processor is configured to receive a
command from a user setting forth a mode of the sampling device to one of a song practice mode and a
rhythm play mode", with sampling for a phrase in the first, and in the second successive sampling of
"a plurality of rhythmic instrument tones" while "the processor causing the rhythm pattern to
continue playing while sampling new sound wave data". The closest Debussy can come is a user
recording a take, loading it into a zone and playing a MIDI clip through the sampler, each step a
separate user action. The design has no such modes and no sampling while a pattern plays. Missing:
the "song practice mode and a rhythm play mode" and sampling triggered by the sampling command.
Verdict: **Does not read**.

**Prior art predating 2014-04-25:** US4667556A (Casio, 1984: external sound sampled into waveform
memory and played across the keyboard). No art in this note for auto-play after sampling.

**Verdict: Does not read** (claims 1, 9, 10, 11; High confidence). A "record into the sampler"
feature that then plays a stored pattern or phrase, or that samples while a rhythm loops, would need
a fresh reading before 2035-03-23.

## Search log

Web searches return about ten hits each; they were summarised by the search tool, not listed by
a patent database.

| Database | Query | Date | Hits | Result |
|---|---|---|---|---|
| Web | `sampler patent keymap multisample key range zones root key pitch shift playback sample` | 2026-11-16 | 10 | Kurzweil and Reason manuals, no patent (1) |
| Web (extended) | `sampler patent granted 2015..2024 "sample zones" OR "key zones" mapped to MIDI notes virtual instrument sample playback root note pitch shifting claims` | 2026-11-16 | 9 | Vendor manuals; US10964299 and US11037538 (automated music generation, mention root keys in background only) (2) |
| Web | `patent drum sampler choke group exclusive class hi-hat mute voices same group` | 2026-11-16 | 10 | HISE forum threads only (3) |
| Web | `SoundFont 2.0 specification exclusiveClass generator 57 keyRange overridingRootKey 1998 pdf` | 2026-11-16 | 10 | fluid-dev posts, Polyphone-hosted 2.04 PDF (4) |
| Web | `Fairlight CMI 1979 Emulator 1981 sampler history ...` | 2026-11-16 | 17 | History pages (5) |
| Google Patents (web search, domain-limited) | `sampler "choke" hi-hat "exclusive" group sample playback note-on stops other voices` | 2026-11-16 | 9 | US9076264, US5262585, US9514724; rows 4, 8 (6) |
| Google Patents (web search) | `electronic musical instrument waveform sample pitch ratio read address increment key number transposition sample playback interpolation` | 2026-11-16 | 9 | US4667556A, US4202234A, US5371315A, EP0925576B1 and others (7) |
| Web | `Ableton patent sampler instrument sample mapped to key range zones playback pitch` | 2026-11-16 | 10 | No Ableton patent; manuals (8) |
| Web | `Native Instruments patent sample mapping keyboard zones automatic assignment samples to keys root note` | 2026-11-16 | 10 | Manuals and forums, no patent (9) |
| Google Patents (web search) | `Roland OR Akai OR Yamaha patent sample playback "key range" "root key" pitch transposition note number sample zone ...` | 2026-11-16 | 9 | EP1646035, US5698807 (10) |
| Google Patents (web search) | `patent drum machine "mute group" OR "choke group" OR "exclusive group" pad sample stop playback another pad same group` | 2026-11-16 | 9 | US8461445, US11688377, US12159610, US9076264; rows 3 to 6 (11) |
| Google Patents (web search) | `patent velocity sensitive sample playback volume scaled by velocity amount parameter sampler MIDI key velocity gain` | 2026-11-16 | 10 | US9330649, EP1646035B1, US4558623A, US8884888B2 (accelerometer velocity, not tabled); row 2 (12) |
| Web | `patent "one-shot" sample playback ignores note-off drum pad sampler gate mode release patent` | 2026-11-16 | 16 | Forums and manuals, no patent (13) |
| Web | `Fairlight Vogel Ryrie patent digital sound sampling keyboard ...` | 2026-11-16 | 10 | No Fairlight patent found (14) |
| Google Patents (web search) | `patent sampled sound keyboard "key groups" OR "keyboard split" ... Emulator Kurzweil patent 1980s` | 2026-11-16 | 9 | Only QWERTY-keyboard patents (15) |
| Google Patents (web search) | `patent software sampler virtual instrument sample playback pitch shift Hermite OR cubic interpolation fractional read position ... 2010..2024` | 2026-11-16 | 10 | US10224062B1 (row 1), US5111727A, US6365816B1, US20090260505A1, US20160217802A1 (sample-rate converter, not opened) (16) |
| Web | `Akai S900 OR S1000 OR MPC60 OR MPC3000 release year manual "mute group" ...` | 2026-11-16 | 19 | MPC60 and MPC1000 manuals, Sound on Sound (17) |
| Web (extended) | `patent "sample" instrument "zone" "key range" "root key" "loop" "release" mapping note-on selects zone ...` | 2026-11-16 | 9 | Vendor manuals; no patent (18) |
| Google Patents (web search) | `Yamaha OR Roland OR Casio patent waveform data read address pitch increment note number interpolation PCM tone generator "exclusive" OR "choke" ...` | 2026-11-16 | 9 | US5218155A, US5252773, US5146834, US20100236384A1 (row 7), DE69733038T2 (19) |
| Google Patents (web search) | `patent musical sound generation sampled waveform one-shot "note-off" ignored percussion sample reproduced to end ...` | 2026-11-16 | 9 | US20090260505A1, US4628790, US5248842, US7642445B2, EP1734508A1 (key-off handling, not opened) (20) |
| Web | `US11688377B2 Intelliterran synthesized percussion pedal claim "choke group"` | 2026-11-16 | 10 | Family listing; no claim text (21) |
| Google Patents (page fetch) | US9076264, US9330649B2, EP1646035B1, US9514724, US11688377B2, US12159610B2, US8461445B2, US5698807A, US5248845A, US4667556A, US4202234A, US20090260505, US10224062B1, US6365816, US5111727A, US5288940, US5218155A, US20100236384, DE69733038T2 | 2026-11-16 | 19 | Bibliographic data, status and expiry as quoted in the table; claims seen where noted (22) |
| Wikipedia (page fetch) | Fairlight CMI, E-mu Emulator, SoundFont | 2026-11-16 | 3 | Dates quoted under Prior art (23) |
| Sound on Sound, fluid-dev list (page fetch) | MPC60 Revisited; fluid-dev 2016-09 post | 2026-11-16 | 2 | MPC60 1988, Linn-9000 1984; exclusiveClass quotation (24) |
| Google Patents (stage 2 claim reading) | US10224062B1, US9330649B2, US8461445B2 (offset 100000: claims 1, 6, 7, 8, 15, 16, 18, 19 and a check for "choke"/"mute" in all claims), US8035021B2 (offset 100000); US9076264, US9514724B2 (offset 0) | 2026-11-16 | 6 | Independent claims read in full as consecutive pieces; HTML text, OCR status not stated (25) |
| Google Patents (stage 2 claim reading) | US11688377B2 (offsets 0, 100000, 200000, 300000, 400000, 445000, 460000), US12159610B2 (offsets 0, 200000, 300000, 400000), US20230343315A1 (offset 0) | 2026-11-16 | 0 | No claims in the converted text, which ends inside the description's Aspects list; choke group in the description only (26) |
| USPTO and Google grant PDFs | `image-ppubs.uspto.gov/.../downloadPdf/11688377`; `patentimages.storage.googleapis.com/.../US11688377.pdf` | 2026-11-16 | 0 | Image-only (CCITT G4 pages, no text layer); not readable here (27) |
| Web | `"11688377" Synthesized percussion pedal claims "What is claimed"`; `US12159610B2 Intelliterran claims ...`; `patentsgazette.uspto.gov US11688377 20230627`; same for US12159610; `trea.com "Synthesized percussion pedal and docking station" patentgrant Intelliterran` | 2026-11-16 | about 50 | No claim text for rows 5 or 6; trea.com page found is US10741154 (family, claims not on the page); RPX summary of US10546568B2 claim 1 (not opened) (28) |
| Web | `US10224062B1 sample rate conversion pitch-based interpolation filters Savell` | 2026-11-16 | 10 | Family US9236064, US10002618 (not opened) (29) |

Refused or failed:

- A Google Patents query URL (`patents.google.com/xhr/query?...`, CPC-free keyword search for
  "sampler" and "root key", US, priority after 2006) was refused by the permission system, so no
  database-side query ran. Espacenet and USPTO Patent Public Search were not tried after that.
  Every candidate comes from a web search or a fetched patent page, not a class listing.
- The SoundFont specification PDFs (Polyphone 2.04 copy, SynthFont 2.01 copy) came back as
  compressed binary the fetch tool could not read; the Akai MPC60 manual PDF the same.
- Returned 404: `en.wikipedia.org/wiki/Emulator_(sampler)`, `patents.google.com/patent/US9076264B2/en`,
  `patents.google.com/patent/US10964299B2/en`.
- Several Google Patents pages are over 100000 characters and the claims lay beyond the first
  window; only the claims noted in the table were read (stage 1). Stage 2 read the later windows.
- Stage 2, rows 5 and 6: Justia (`patents.justia.com/patent/11688377`, `/12159610`),
  FreePatentsOnline (`11688377.html`), uspto.report (`patent/grant/11688377`) and RPX Insight
  returned 403; PatentGuru returned HTTP 468; 9235.net returned a login page. The PDF reader here
  needs `pdftoppm`, which is not installed, so the image-only grant PDF could not be viewed either.

## Open uncertainties

- No class-listed search ran (G10H 7/00, G10H 1/00, G10H 1/18, G10H 2250/545). A human or a session
  with browser access should run `CPC=G10H7/00` and `G10H1/0058` style searches with
  `after=priority:20060101` in Google Patents. "No candidate found" holds only for the web searches
  in the Search log.
- **Rows 5 and 6 (Intelliterran, to 2034-03-17) have no verdict; stage 2 is not complete for
  them.** Their claims were not obtained (Claim analysis, Candidates 5 and 6). A human should read
  the independent claims of US11688377B2 and US12159610B2 from USPTO Patent Center or Patent Public
  Search and decide whether any recites a choke group, then whether element 5 meets every element
  of that claim. Until then element 5 is not cleared. Other family members (US9495947B2,
  US9892720B2, US9905210B2, US10546568B2, US10741154B2, US10741155B2) were not checked for status or
  claims.
- Row 3 (stage 2): the stage 1 table says claims 6 and 15 recite choke and mute; they recite only
  a "special playing technique" (choke and mute are description examples). The table was left as
  stage 1 wrote it; the correction is under Claim analysis.
- Row 4 (stage 2, does not read): the claims of its continuations, if any, were not looked for.
  Debussy's MIDI clip editing (track pane) was not compared with claims 1, 8, 12; only the sampler
  design was.
- Row 7 (stage 2, does not read): Debussy's project clip streaming (ADR-0008, ADR-0034) reads audio
  from disk during playback. It is outside the sampler design and was not compared with claim 1
  (which also needs prestored first pages of every waveform and burst page reads); a human may want
  that checked separately. A sampler that streams zone samples from disk would need a reading
  before 2030-04-15.
- Rows 1, 2, 3, 4, 7, 8 (stage 2): claim text is Google's HTML in pieces under 125 characters,
  joined; compare the decisive elements with the USPTO grant text before relying on them. Row 1's
  "obtaining a pitch value based at least on a relationship between an input sampling rate ... to
  an output sampling rate" was judged arguably met by the varispeed step; the verdict does not
  depend on it.
- Design changes that would need a fresh reading (each named under its candidate): pitch-dependent
  anti-alias filtering by blending filter coefficients (row 1, to 2038-04-30); velocity layers with
  round-robin or alternation on repeats (row 2, to 2034-03-19); a pad grid that merges pads or a
  designated choke pad (row 3, to 2029-09-11); a step-sequencer pattern/part grid (row 4, to
  2031-10-15); streaming zone samples (row 7); recording into the sampler followed by automatic
  pattern playback (row 8, to 2035-03-23). The issue #174 scope excludes all of these.
- EP1646035B1 is expired, but its US application 11/243,003 (Sony) was not looked up; a granted
  US patent could run past 2025. Search it before treating element 3's key mapping as clear.
- No dated primary source was read for: the Fairlight CMI's pitch method, the Emulator's pitching
  and keymap, Akai S-series or MPC mute groups, SoundFont `keyRange`, `overridingRootKey`,
  `exclusiveClass` and `sampleModes` in 2.00a (1995) specifically, or MIDI 1.0 velocity. The
  earliest dated art for elements 1 to 3 in this note is patent-form (US4667556A, 1984; US5111727A,
  1990). A human could read the SoundFont 2.01 PDF
  ([SynthFont](https://www.synthfont.com/SFSPEC21.PDF)) to date the zone and exclusive-class art.
- US5262585 (electronic cymbal system), US4628790, US5248842, US5252773, US5146834, US6858790,
  US7642445B2, EP1734508A1 and US20160217802A1 were returned by searches and not opened; the first
  six are old by number, the last three could be in term.
- Element 7 and the Hermite read are cleared only by reference to ADR-0148 and ADR-0120
  (`docs/research/synth-patents.md`, `docs/research/modulation-patents.md`).
- Term ends are as Google prints them (assumptions, not legal conclusions); none was checked
  against USPTO records.
