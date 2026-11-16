# Patent status of the sampler instrument's playback, zones and choke groups

Research note, 2026-11-16. This is not legal advice. It records what primary sources show, so a
reviewer can check it. Stage 1 only: the candidates are listed, no claim has been analysed, and
**Status** is blank for a human. Dates in the Candidates table are as Google Patents prints them
(Google states that its status and expiry fields are assumptions, not legal conclusions).

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

Stage 1. Eight candidates have a printed term end in the future (rows 1 to 8). None was claim-read.
From titles, abstracts and the claim fragments I saw, none obviously covers an element as built:
the nearest are row 1 (pitch-ratio resampling with interpolated filter coefficients, against a
direct Hermite read), row 2 (repeat-note velocity layer selection, against no layering) and row 3
(choke only in dependent claims, with grouped pads). The basic sampler art (pitch by address
increment with interpolation, 1984 to 1992; one-shot waveforms, 1991) is expired. Where a row says
"No candidate found", it means the searches in the Search log found none; the databases that list
by class could not be queried (see "Refused or failed").

| Technique | Verdict | Confidence |
|---|---|---|
| 1. Varispeed: playhead stepped by `2^(semitones/12)` from a root key | Expired art from 1984 (US4667556A), 1990 (US5111727A, US6365816B1). Row 1 (US10224062B1, Microsoft) is a resampler with pitch-dependent filters; claims not read. No other in-term candidate found (searches 3, 4, 12, 13, 14). | Low |
| 2. 4-point Hermite read, zero outside the sample | Cleared by ADR-0120; see `docs/research/modulation-patents.md` element 5. Row 1 touches interpolated sample-rate reads but recites coefficient interpolation between two filters, not a Hermite read (claims not read). No patent found claiming Hermite for a sample read. | Low |
| 3. Key-range zones with root key, first match, no layering | No patent found claiming key-range zones or root keys (searches 1, 2, 7, 8, 9). Row 2 (Apple) selects among velocity layers, which the design lacks. Mapping is in vendor manuals and Kurzweil, E-mu and SoundFont documentation (Prior art). | Low |
| 4. One-shot or gated (note-off through ADSR), free at end | No in-term candidate found (searches 5, 6, 15). Earlier art: US5288940A (Kawai 1991, expired); US20090260505A1 (Samsung 2008, abandoned application) describes the same modes. | Low |
| 5. Choke groups 1 to 16, 3 ms linear fade | Rows 3 to 6 mention choke or mute: row 3 only in dependent claims (Yamaha), rows 4 to 6 only in descriptions as far as read. No patent found claiming exclusive-class or choke as such. Art: SoundFont exclusiveClass (2.04 spec text), Akai MPC mute groups (manuals, undated). | Low |
| 6. Velocity to gain, `1 − a + a·v/127` | No candidate found claiming a velocity amount scaling gain (search 10). Row 2 touches velocity-layer volume matching. | Low |
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
in by a human from Google Patents or USPTO Patent Center. All rows are marginal as far as read:
none had its claims read in full; where I saw claim text it is noted.

| # | Patent | Assignee | Priority / filing | Est. term end | Element it may touch | Link | Status |
|---|---|---|---|---|---|---|---|
| 1 | US10224062B1, Sample rate conversion with pitch-based interpolation filters. Claim 1 (seen in pieces): a pitch value from the input and output sampling rates; coefficients for two filters from a fractional address; a third set by interpolating between them; filter the input samples. | Microsoft Technology Licensing LLC (inventor Savell) | 2018-02-02 / 2018-04-30; granted 2019-03-05 | 2038-04-30 (Google, anticipated; no adjustment shown) | 1, 2 (pitch ratio set by the rate relationship; interpolated reads; the design steps by a note-derived ratio and reads with Hermite, with no coefficient interpolation) | [GP](https://patents.google.com/patent/US10224062B1/en) | |
| 2 | US9330649B2, Selecting audio samples of varying velocity level. Claim 1 (pieces): play a sample for a first velocity level, on a repeat of the same velocity select a sample of a second velocity level. | Apple Inc (inventors Buskies, Gros) | 2013-07-12 / 2013-08-13; granted 2016-05-03 | 2034-03-19 (Google, adjusted) | 3, 6 (velocity-selected samples; the design has no velocity layers or round-robin and only scales gain) | [GP](https://patents.google.com/patent/US9330649B2/en) | |
| 3 | US8461445B2, Electronic percussion instrument having groupable playing pads. Independent claims 1, 18, 19 combine adjacent pads into one zone with a representative timbre; choke and mute instructions only in dependent claims 6 and 15 (seen in a paraphrase). | Yamaha Corp (inventors Takehisa, Watanabe, Miyashita, Sato) | 2008-09-12 / 2009-09-11; granted 2013-06-11 | 2029-09-11 (Google, anticipated; no adjustment shown) | 5 (choke or mute instruction), 3 (zones of pads sharing a timbre) | [GP](https://patents.google.com/patent/US8461445B2/en) | |
| 4 | US9076264B1, Sound sequencing system and method. Claims 1, 8, 12 are a step-sequencer display and playback; "choke groups" appear in the description only ("one sound ... cut off by the start of another"). | iZotope Inc, now Native Instruments USA Inc (inventors Gillespie, Regan, Todd) | 2009-08-06 / 2009-08-06; granted 2015-07-07 | 2031-10-15 (Google, adjusted) | 5 (description only; claims do not recite it) | [GP](https://patents.google.com/patent/US9076264) | |
| 5 | US11688377B2, Synthesized percussion pedal and docking station. Description: a choke group makes a superseding instrument mute a preceding one (closed hi-hat chokes the open hi-hat sample). Claims not seen. | Intelliterran Inc (inventor Packouz) | 2013-12-06 / 2021-03-24; granted 2023-06-27 | 2034-03-17 (Google, anticipated; no adjustment shown) | 5 (choke group; claims unread) | [GP](https://patents.google.com/patent/US11688377B2/en) | |
| 6 | US12159610B2, same family and title as row 5. The abstract is about repeating MIDI segments by a foot switch (per a search summary); choke not found in the text I read. Claims not seen. | Intelliterran Inc | 2013-12-06 / 2023-06-27; granted 2024-12-03 | 2034-03-17 (Google, anticipated; no adjustment shown) | 5 (family of row 5; claims unread) | [GP](https://patents.google.com/patent/US12159610B2/en) | |
| 7 | US8035021B2 (publication US20100236384A1), Tone generation apparatus. Abstract: waveform data read page by page from NAND flash into a waveform buffer; the description has a pitch counter with integer and decimal parts and two-point linear interpolation. Claims not seen. | Yamaha Corp (inventors Shirahama, Shirakawa, Fujita) | 2009-03-23 / 2010-03-22; granted 2011-10-11 | 2030-04-15 (Google, adjusted) | 1, 2, 3 (sample waveform playback by a pitch counter; the design holds the whole sample in memory and reads Hermite) | [GP](https://patents.google.com/patent/US20100236384) | |
| 8 | US9514724B2, Sampling device, electronic instrument, method, and program. Claim 1 (pieces): sample sound from a microphone, then automatically play a stored rhythm pattern or phrase using the sampled tone; dependent claim 8 has keys specifying pitches. | Casio Computer Co (inventors Setoguchi, Ishioka) | 2014-04-25 / 2015-03-23; granted 2016-12-06 | 2035-03-23 (Google, anticipated; no adjustment shown) | 1, 3 (playing a sampled tone at selected pitches; the design plays imported files and does not sample or auto-play a preset pattern) | [GP](https://patents.google.com/patent/US9514724) | |

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

Not done: stage 1 only. For each candidate a human marks **In force**, claims are read in stage 2.

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
  window; only the claims noted in the table were read.

## Open uncertainties

- No class-listed search ran (G10H 7/00, G10H 1/00, G10H 1/18, G10H 2250/545). A human or a session
  with browser access should run `CPC=G10H7/00` and `G10H1/0058` style searches with
  `after=priority:20060101` in Google Patents. "No candidate found" holds only for the web searches
  in the Search log.
- Rows 5 and 6 (Intelliterran): only the description of US11688377B2 was read; claims of both are
  unseen. Choke is in the description. Whether any claim recites it needs a read.
- Rows 4 (iZotope, in force to 2031) recites choke only in its description, so the claims are not
  expected to read, but the claims of its continuations, if any, were not looked for.
- Row 7 (US8035021B2): claims unseen; I opened the publication page (US20100236384A1) only.
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
