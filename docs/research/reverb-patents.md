# Patent status of the in-house Reverb effect (Room feedback delay network, Plate figure-eight tank, crossfade between them)

Research note, 2026-11-13. This is not legal advice. It records what primary sources show, so a
reviewer can check it. Stage 1 listed candidates; **Status** was filled from Google Patents; stage 2
(2026-11-13) read the independent claims of rows 1 to 10 (see Claim analysis). Dates in the
Candidates table are as Google Patents prints them (Google states that its status and expiry fields
are assumptions, not legal conclusions).

## Planned design

Software, portable scalar Rust, a "Reverb" effect node in an audio editor, built in-house, on the
real-time audio thread. Two algorithms chosen by a parameter.

1. **Shared controls and memory.**
   - Pre-delay 0 to 200 ms, in whole frames.
   - `decay` as RT60, 0.2 to 20 s, log scale.
   - `damping`: the frequency above which decay shortens.
   - `size` 0.5 to 2: scales every delay length; changeable only while stopped.
   - Dry/wet `mix`.
   - Buffers are allocated at prepare for size 2 at the graph rate; under 4 MB at 192 kHz.
2. **Room (feedback delay network, FDN).**
   - Input diffusion through 4 series Schroeder allpasses.
   - 8 delay lines with mutually prime base lengths x `size`.
   - 8x8 Hadamard feedback matrix, computed by fast Walsh-Hadamard transform.
   - Per-line gain `10^(-3*m_i/(T60*fs))` (m_i the line length in samples) and a one-pole low-pass
     per line for frequency-dependent decay (Jot and Chaigne 1991).
   - The stereo input feeds alternate lines; L/R outputs are taken with +/- sign patterns over the
     lines.
3. **Plate.**
   - Jon Dattorro's 1997 JAES "Effect Design Part 1" plate topology: input band-limit one-pole,
     4 input diffusers, a two-branch figure-eight tank with decay diffusers, damping one-poles and
     fixed output taps.
   - Lengths scaled from 29 761 Hz to the graph rate x `size`.
   - The tank allpasses are modulated by a slow LFO, with linear-interpolated fractional delay
     reads.
4. **Algorithm change.** Crossfade between the two engines over a fixed ramp time; both engines
   run while the fade lasts.

## Summary

Ten candidates are in force or pending (rows 1 to 10) and their independent claims were read
(see Claim analysis). One reads: row 6 (Abel, US10559295B1) claim 10, on the `size` control, which
expired art from 1998 and 2000 (US7561699, US6978027B1) predates. Four are unclear: row 1 (Sony
FDN; depends on whether the L/R outputs are elements of the Hadamard output vector), row 3 (ST;
depends on whether a half of the plate tank is a "feed forward loop"), row 7 (Casio filter
crossfade) and row 8 (Harman, pending; "early reflections signal"). Rows 2, 4, 5, 9 and 10 do not
read. Every foundational technique in elements 2 and 3 has dated prior art from 1960 to 1997 that
is expired or is a publication (see Prior art). Coverage rests on web searches only (see "Refused
or failed").

| Technique | Verdict | Confidence |
|---|---|---|
| 1. Pre-delay, RT60 control, damping frequency, `size` scaling of delay lengths, mix | **Exposed.** Row 6 claim 10 reads on `size` (stopped-only changes do not avoid it); prior art US7561699 (1998) and US6978027B1 (2000) predates its 2017 priority. Rows 4 and 9 do not read. | Medium |
| 2. Room FDN (4 allpass diffusers, 8 prime lines, Hadamard by FWHT, per-line T60 gain and one-pole damping, alternate-line input, +/- sign outputs) | **Not cleared.** Row 1 unclear (claim element 1d: outputs from the matrix output vector); rows 2, 3 (for Room), 4, 5, 9, 10 do not read. Row 1 has foundational art predating its 2007 priority: Stautner and Puckette 1982, Jot and Chaigne 1991, US5491754A. | Low |
| 3. Plate (Dattorro 1997 topology, scaled lengths, LFO-modulated allpass with linear-interpolated reads) | **Not cleared.** Row 3 unclear (whether a tank half is a "feed forward loop" in "a cascade manner"); Dattorro 1997 predates its 2006 priority. Row 6 claim 10 also reads on `size` scaling of the plate lengths. | Low |
| 4. Crossfade between two reverb engines on algorithm change | **Not cleared.** Row 7 unclear ("set of filters ... combining at least two" partial filters; "backup partial filter"); row 8 (pending) unclear ("early reflections signal"). Predating art: US5553150A (1993), US6978027B1 (2000). | Low |

## Prior art

Items marked (unverified) come from a search summary or an index, not from the publication itself.
The CCRMA PDF of Dattorro (a scanned image) and the AES E-Library pages for Jot and Chaigne and for
Dattorro (HTTP 403 or redirected) could not be read as text by the fetch tool, so their contents
are not established here.

- **Element 2, delay-network reverberators (FDN) and the Hadamard matrix.**
  - Gerzon, orthogonal-matrix feedback reverberation unit, 1971, and Stautner and Puckette, a
    four-channel FDN reverberator with stability conditions, 1982, whose feedback matrix is a
    permutation of a 4x4 Hadamard matrix with one row sign inverted; J. O. Smith, Physical Audio
    Signal Processing, history section
    ([JOS](https://ccrma.stanford.edu/~jos/pasp/History_FDNs_Artificial_Reverberation.html)). The
    page prints the years only inside its citation keys, so the years are taken from those keys
    (Gerzon 1971, Stautner and Puckette 1982) and not from a dated copy of either publication.
    Neither original was opened.
  - Jean-Marc Jot and Antoine Chaigne, "Digital Delay Networks for Designing Artificial
    Reverberators", AES 90th Convention, preprint 3030, 1991-02-06 (AES E-Library entry reported by
    a search summary, [AES](https://aes2.org/publications/elibrary-page/?id=5663); page not read
    because it returned HTTP 403). The JOS history page credits Jot (1991) with a design method that
    sets reverberation time largely independently in frequency bands.
    US5491754A (Jot and Chaigne, France Telecom, priority 1992-03-03) cites and describes the same
    construction: per-delay attenuation by a frequency-dependent factor that decreases
    monotonically with the simulated reverberation time and is proportional to the delay
    ([Google Patents](https://patents.google.com/patent/US5491754A/en)); abstract read, claims not.
  - Hadamard matrix as a feedback matrix: the JOS page "Hadamard Matrix" says a Faust example
    reverberator uses one and that for n a power of 4 it "requires no multiplies in fixed-point
    arithmetic" ([DSPRelated](https://www.dsprelated.com/freebooks/pasp/Hadamard_Matrix.html), the
    book is dated 2010, not an earliest source).
  - Fast Walsh-Hadamard transform, general (not reverb): US4446530A, Matsushita, priority
    1980-08-14, "Fast Hadamard transform device"
    ([Google Patents](https://patents.google.com/patent/US4446530A/en)). Use of a fast
    Walsh-Hadamard transform inside a reverberator feedback matrix: US8345887B1 (Sony, priority
    2007-02-23, Candidates row 1) describes it; no earlier source was found.
  - Mutually prime delay lengths: the JOS page on Schroeder allpass sections says lengths in a
    series cascade are "typically mutually prime" and gives 1051, 337, 113 as an example
    ([JOS](https://ccrma.stanford.edu/~jos/pasp/Schroeder_Allpass_Sections.html)); undated page.
    US8345887B1 claim 6 recites mutually prime delay periods (seen via a fetch summary).
  - T60-to-gain mapping: the JOS page "Achieving Desired Reverberation Times" derives the
    per-sample attenuation `G = 0.001^(1/N60) = 10^(-3/N60)` so that propagation through N60
    samples attenuates by 60 dB
    ([JOS](https://ccrma.stanford.edu/~jos/pasp/Achieving_Desired_Reverberation_Times.html)). With
    N60 = T60*fs/m_i per line this is the design's `10^(-3*m_i/(T60*fs))` per pass through a line of
    m_i samples. The page is undated and secondary; the primary is Jot 1991 above.
  - One-pole low-pass per line for damping: the JOS page "Damping Filters for Reverberation Delay
    Lines" returned HTTP 404 under the URL I guessed, so no source for the one-pole form was read.
    US7149314B2 (Creative, priority 2000-12-04) describes a low-pass filter in each absorbent
    delay line to control decay at a high frequency (via search summary and fetch summary of the
    page, [Google Patents](https://patents.google.com/patent/US7149314B2/en)).
  - Stereo input to alternate lines and +/- output sign patterns: no source found. Open
    uncertainty.
- **Element 2, Schroeder allpass diffusers.**
  - US3110771A, Logan and Schroeder, Bell Telephone Laboratories, priority and filing 1960-09-29,
    granted 1963-11-12: an electronic reverberator built from series all-pass feedback networks
    with different delays ([Google Patents](https://patents.google.com/patent/US3110771A/en)).
  - M. R. Schroeder and B. F. Logan, "'Colorless' Artificial Reverberation", JAES 9(3), 192-197,
    July 1961; abstract reports cascaded all-pass filters for reverberation with high echo density
    (AES E-Library entry via search summary, [AES](https://aes2.org/publications/elibrary-page/?id=465);
    not opened). Schroeder, "Natural Sounding Artificial Reverberation", JAES 10(3), July 1962
    (PDF copy [Rochester](https://hajim.rochester.edu/ece/sites/zduan/teaching/ece472/reading/Schroeder_1962.pdf),
    not opened; a search summary says it uses five series allpass delays).
  - JOS page on Schroeder allpass sections: series allpass cascade, typical feedback gain 0.7
    ([JOS](https://ccrma.stanford.edu/~jos/pasp/Schroeder_Allpass_Sections.html)).
- **Element 3, Dattorro plate.**
  - Jon Dattorro, "Effect Design, Part 1: Reverberator and Other Filters", JAES 45(9), 660-684,
    September 1997 (volume, pages and date from a search summary of the AES E-Library entry,
    [AES](https://aes2.org/publications/elibrary-page/?id=10160); the author's copy,
    [CCRMA PDF](https://ccrma.stanford.edu/~dattorro/EffectDesignPart1.pdf), is a scanned image the
    fetch tool could not read). Secondary descriptions (search summaries, not opened pages): pre-delay,
    bandwidth low-pass, four cascaded allpasses, a figure-8 tank, tank allpass delays modulated by
    LFOs ([UCSD course page](https://tre.ucsd.edu/wordpress/?p=1192), [Valley Audio Plateau](https://valleyaudio.github.io/rack/plateau)).
    The 29 761 Hz reference rate, the decay-diffuser and damping placement, the output taps and
    the interpolated reads are therefore not established from the paper here.
  - Griesinger, "Practical Processors and Programs for Digital Reverberation", Proceedings of the
    AES 7th International Conference, 187-195, 1989 (cited in a later patent per a search summary;
    not opened, unverified). Several secondary sources say Dattorro's topology follows Griesinger's
    Lexicon work ([freeverb3 notes](https://freeverb3-vst.sourceforge.io/tips/reverb.shtml)); the
    attribution is secondhand.
  - Modulation of reverberator delays, expired patents: US4268717A (Moore, priority 1979-04-19),
    reverberation simulator with fixed and time-modulated delay taps, feedback from the modulated
    taps ([Google Patents](https://patents.google.com/patent/US4268717A/en)); US5553150A (Yamaha,
    priority 1993-10-21), read position offset by random numbers and cross-faded
    ([Google Patents](https://patents.google.com/patent/US5553150A/en)).
  - Linear-interpolated fractional delay read: no dated primary source opened.
- **Element 1, size scaling of delay lengths.**
  - US7561699 (Creative, priority 1998-11-13): claim 1 scales delay and decay parameters in
    proportion to a room-size factor (fetch summary of the Google Patents page; the page was
    fetched under US7561699B2, [Google Patents](https://patents.google.com/patent/US7561699B2/en)).
  - US6978027B1 (Creative, priority 2000-04-11): delay line lengths made longer or shorter
    with virtual room size, and delay changes made by crossfading between two read taps (a search
    summary; the claim-1 paraphrase on the fetched page covers the ramped level control around
    read-pointer moves, [Google Patents](https://patents.google.com/patent/US6978027)).
  - Pre-delay in whole frames, pre-allocating for size 2: no source found.
- **Element 4, crossfade between two reverbs.**
  - US6978027B1 above (crossfade between read taps and ramping a level around a pointer move
    within one reverberator).
  - Greenblatt, Abel and Berners, a crossfade between convolution and FDN components of a hybrid
    reverb (search summary only; [A hybrid reverberation crossfading technique](https://bvbvf1.bib-bvb.de/vufind/PrimoRecord/cdi_ieee_primary_5495753),
    not opened, date not read).
  - Commercial blending of two algorithms: Muramasa Audio MorphVerb review
    ([MusicTech](https://musictech.com/reviews/plug-ins/muramasa-audio-morphverb/), undated, not
    relied on).
  - A dated primary source for a fixed-time crossfade between two complete reverb engines: not
    found. Open uncertainty.

## Candidates

Patents that might cover an element of the design and might still be in force. Status is filled
in by a human from Google Patents or USPTO Patent Center.

| # | Patent | Assignee | Priority / filing | Est. term end | Element it may touch | Link | Status |
|---|---|---|---|---|---|---|---|
| 1 | US8345887B1, Computationally efficient synthetic reverberation | Sony Computer Entertainment America LLC (current per Google: Sony Interactive Entertainment LLC) | 2007-02-23 / 2007-02-23 | 2030-08-31 (Google adjusted; page prints "Active") | 2 (claim 1 as seen via a fetch summary: delay lines, a matrix transformer, feedback mixers, a subset of outputs as multi-channel reverberation; dependent claims 3, 4 Hadamard and 16x16 Hadamard, 6 mutually prime delays, 13 fast Walsh-Hadamard transform; no filters; claims not read in full) | [GP](https://patents.google.com/patent/US8345887) | **In force** |
| 2 | US8204240B2, Apparatus and method for artificial reverberation | Individual (Brian C. Neunaber) | 2007-06-30 (provisional 60/937,817) / 2008-06-25 | 2031-04-06 (Google adjusted; page prints "Active") | 2 (claim 1 as seen via a fetch summary: N-1 input summing nodes each feeding a filter and a digital delay in series, summed and fed back through attenuators, a feedback node with no input feed; not shown to recite a matrix; claims not read in full) | [GP](https://patents.google.com/patent/US8204240B2/en) | **In force** |
| 3 | US8351614B2, Digital reverberations for audio signals | STMicroelectronics Asia Pacific Pte Ltd (current per Google: STMicroelectronics International NV) | 2006-02-14 / 2007-02-02 | 2030-07-01 (Google adjusted; page prints "Active") | 2, 3 (delay lines, allpass and lowpass filters; claim 1 as seen: a reverberation generator with a direct path and "a plurality of feed forward loops configured in a cascade manner"; a cascade of feed-forward loops is not shown to match the design's feedback structures) | [GP](https://patents.google.com/patent/US8351614B2/en) | **In force** |
| 4 | US10043509B2, Reverberator and method for reverberating an audio signal (published as US20170323632A1; family includes WO2011057868A1, JP6487383B2) | Fraunhofer-Gesellschaft | 2009-10-21 / 2017-07-26 | 2030-10-06 (Google; page prints "Active") | 1, 2 (claim 1 per a fetch summary: a feedback delay loop processor delaying at least two frequency subband signals by different loop delays, each subband with its own delay line, feedback loop and combiner; per-subband attenuation depends on the target reverberation time and the loop delay; the design has one delay network, not per-subband loops) | [GP](https://patents.google.com/patent/US10043509B2/en) | **In force** |
| 5 | US8908875B2, Electronic device with digital reverberator and method | King's College London (current per Google: De Sena Enzo) | 2012-02-02 / 2012-02-02 | 2032-11-28 (Google adjusted; page prints "Active") | 2 (digital reverberator with surface-absorption filters placed in the delay paths of a delay-network reverberator; abstract and Google's FDN background only; claims not read) | [GP](https://patents.google.com/patent/US8908875) | **In force** |
| 6 | US10559295B1, Artificial reverberator room size control | Individual (Jonathan S. Abel) | 2017-12-08 (provisional 62/596,656) / 2018-12-07 | 2038-12-07 (Google; no adjustment shown; page prints "Active") | 1, 2 (independent claim 10 per a fetch summary: a network of delay lines whose lengths change with a size control; claims 1, 5 resample a room impulse response; claim 8 adjusts decay time by size and air absorption; claims not read in full) | [GP](https://patents.google.com/patent/US10559295) | **In force** |
| 7 | US10311845B2, Filter characteristics changing device | Casio Computer Co., Ltd. | 2017-03-15 / 2018-02-20 | 2038-02-20 (Google; no adjustment shown; page prints "Active") | 4 (claim 1: a set filter formed by combining partial filters, crossfading two of them on a change of characteristic; claim 4 refers to "a predetermined reverberation characteristic"; crossfades filters, not two reverb engines) | [GP](https://patents.google.com/patent/US10311845B2/en) | **In force** |
| 8 | EP4468748A1, Seamless reverbaration transition in virtual venues | Harman Becker Automotive Systems GmbH | 2023-05-22 / 2023-05-22 | Pending; 2043-05-22 if granted (20 years from filing; not a printed date) | 4 (claim 1 fragments: generate first and second early-reflections signals for two sets of reverberation parameters simultaneously, "combining ... through a fading process", output with the direct signal; a reverberation tail is switched discretely; claim text not read in full) | [GP](https://patents.google.com/patent/EP4468748A1/en) | **In force** (pending application) |
| 9 | US20240379089A1, Rendering of Reverberation with Startup Control | Nokia Technologies Oy | 2023-05-08 / 2024-05-03 | Pending application; 2044-05-03 if granted, plus any adjustment (20 years from filing; not a printed date) | 2 (digital reverberator with a control line of a delay and gain filter that suppresses part of the late reverberation; the description presents an FDN with per-line attenuation filters and a feedback matrix; claims not read) | [GP](https://patents.google.com/patent/US20240379089A1/en) | **In force** (pending application) |
| 10 | US12407999B2, Adjustment of reverberator based on source directivity | Nokia Technologies Oy | 2021-12-03 / 2022-12-01 | 2043-03-02 (Google adjusted; page prints "Active") | 2 (abstract: gains from source directivity data, averaged, written to a bitstream with room parameters, rendered with a reverberator; a search summary says the reverberator has per-line graphic-EQ attenuation filters; claims not read) | [GP](https://patents.google.com/patent/US12407999B2/en) | **In force** |
| 11 | US8041045B2, Unnatural reverberation (application US20060086237A1) | Richard S. Burwen Living Trust (original: Burwen Technology, Inc.) | 2004-10-26 (provisional 60/622,294) / 2005-10-21 | 2030-06-18 (Google adjusted; page prints "Expired - Fee Related") | 2 (processor applies gain coefficients to delayed samples and sums them into reverberation mixed with the direct signal; structure not described beyond the abstract; claims not read) | [GP](https://patents.google.com/patent/US8041045B2/en) | **Lapsed (fees)** |

Checked by Claude from Google Patents, 2026-11-13, under Chris Paro's rule: no expired or revoked
note means In force. Rows 1–7 and 10 show "Active"; rows 8 and 9 show "Pending" (applications,
not grants); row 11 shows "Expired - Fee Related".

Status values: **In force**, **Lapsed (fees)**, **Expired (term)**, **Abandoned**, **Invalidated**,
**Not found**. Record who checked and when, under the table.

Row 11's printed term end is in the future but Google prints a fee-related expiry; a human should
confirm. Rows 8 and 9 are applications, not granted patents.

Expired by term (filing date + 20 years, plus any printed term adjustment, is in the past), listed
for completeness and needing no check:

- [US3110771A](https://patents.google.com/patent/US3110771A/en), Bell Telephone Laboratories
  (Logan and Schroeder), filed 1960-09-29, expiry 1980-11-12 per Google. Series all-pass
  reverberator (element 2 diffusers).
- [US4268717A](https://patents.google.com/patent/US4268717A/en), Moore (individual), filed
  1979-04-19, expiry 1999-04-19 per Google. Time-modulated delay taps in a reverberation simulator
  (element 3 modulation).
- [US4446530A](https://patents.google.com/patent/US4446530A/en), Matsushita, filed 1981-08-05,
  expiry 2001-08-05 per Google (also lapsed for fees 1996-07-09). Fast Hadamard transform device
  (element 2 matrix).
- [US5553150A](https://patents.google.com/patent/US5553150A/en), Yamaha, filed 1994-08-10, expiry
  2014-08-10 per Google. Reverberation with random-modulated read positions cross-faded (element 3).
- [US5491754A](https://patents.google.com/patent/US5491754A/en), France Telecom (Jot and Chaigne),
  filed 1993-02-19, expiry 2013-02-19 per Google. Delay-network reverberation with
  frequency-dependent per-delay attenuation tied to reverberation time (elements 1, 2).
- [US5109419A](https://patents.google.com/patent/US5109419A/en), Lexicon (Griesinger), filed
  1990-05-18, expiry 2010-05-18 per Google. Room enhancement with time-variant reverberation units
  (element 3 context; not a reverberator-topology claim).
- [US6978027B1](https://patents.google.com/patent/US6978027), Creative Technology, filed
  2000-04-11, expiry 2020-04-11 per Google. Reverberator with delay lengths varied by room size and
  tap crossfading (elements 1, 4 context).
- [US7561699](https://patents.google.com/patent/US7561699B2/en), Creative Technology, filed
  2004-10-26 (priority 1998-11-13), expiry 2022-10-04 per Google (adjusted). Room-size scaling of
  delay and decay (element 1).
- [US7149314B2](https://patents.google.com/patent/US7149314B2/en), Creative Technology (Dahl and
  Jot), filed 2000-12-04, expiry 2023-10-21 per Google (adjusted). Absorbent allpass filters and
  per-line low-pass in a reverberation feedback loop (elements 2, 3).
- [US7062337B1](https://patents.google.com/patent/US7062337B1/en), Blesser (Harman now), filed
  2001-08-06, expiry 2024-03-10 per Google (adjusted). Comb-shaped group delay filter and
  randomised connections in a reverberator (element 2 context).
- [US20020067836A1](https://patents.google.com/patent/US20020067836A1/en), Parthus Technologies,
  filed 2001-08-07, abandoned 2004 per Google; never granted. Shared-delay early and late
  reflection generators with random tap selection (elements 2, 3 context).

Seen and not listed (title, abstract and claims as seen touch no element, or no term date was
established):

- US8391504B1 (Universal Audio, Abel and Berners, adjusted term 2031-11-05 as printed): spring
  reverb by dispersive waveguides; claim 1 identifies propagation modes of a spring; no element.
- US12175956B2 (Casio, 2043-07-07 as printed): waveguide musical-tone synthesis switching one
  all-pass filter among delay units; not a reverberator and no crossfade.

## Claim analysis

Stage 2, 2026-11-13. Claim text is from the Google Patents HTML pages (search log, "stage 2"),
which carry machine-readable text, not OCR as far as the pages show; it was relayed in pieces
through the fetch tool's summarising model and has not been compared with the granted PDFs.
"Design" references are to Planned design elements (for example 2 = Room FDN).

**Row 1, US8345887B1 (Sony; priority 2007-02-23). Independent claims 1, 9, 15. Verdict: Unclear.**

| Claim 1 element | Design |
|---|---|
| 1a "a plurality of delay lines, each to accept an audio signal presented at an input of the delay line and emit a delayed version of the audio signal at an output of the delay line;" | Met: 8 delay lines (2). |
| 1b "a matrix transformer having a respective plurality of inputs coupled to the outputs of the plurality of delay lines to apply a matrix transform to a first vector including the delayed versions ... to produce a second vector on a plurality of outputs of the matrix transformer; and" | Met: 8x8 Hadamard by FWHT on the line outputs (2). The per-line gain and one-pole sit in the loop; "coupled" and "comprising" probably do not exclude them. |
| 1c "a plurality of feedback mixers having respective inputs coupled to the outputs of the matrix transformer, each to mix an element of the second vector with the audio signal presented to an input of a corresponding one of the plurality of delay lines, wherein" | Met at least for the 4 lines that take stereo input (alternate-line input, 2); a plurality suffices. |
| 1d "a subset of elements of the second vector are to be further processed as a multi-channel synthetic reverberation signal." | **Decides it.** The design takes L/R "with +/- sign patterns over the lines". If those patterns are rows of the same Hadamard matrix (dependent claim 7 picks "rows with the most sign interleaving"), the outputs are elements of the second vector and 1d is met (stereo is multi-channel). If they are taken from the line outputs with patterns that are not matrix rows, 1d is not literally met. The design does not fix this. |

Claim 9 (method) has the same elements: 9a "producing a plurality of differently-delayed versions of
an input audio signal by a plurality of delay lines;", 9b "applying a matrix transformation ...",
9c "feeding the outputs of the matrix transformation back into the input audio signal by mixing an
element of the vector with the respective input audio signal; wherein", 9d "a subset of the outputs
of the matrix transformation are to be processed as a multi-channel synthetic reverberation
signal." Claim 15 (system) adds 15a "an audio signal source;" (met: the node's input) and ends
15e "an audio output module to produce a multi-channel synthetic reverberation signal from a subset
of the plurality of the outputs of the matrix transformation module." 9d and 15e decide them the
same way as 1d. Hadamard (claims 3, 12), mutually prime delays (6) and FWHT (13) are dependent and
are all present in the design, so they add exposure only if an independent claim reads. Prior art
predating 2007: Stautner and Puckette 1982 (Hadamard-type FDN matrix), Jot and Chaigne 1991,
US5491754A (priority 1992); whether they take outputs from the matrix output was not established.

**Row 2, US8204240B2 (Neunaber; priority 2007-06-30). Independent claims 1, 16. Verdict: Does not
read.** Claim 1 requires "N−1 input summing nodes", each feeding "said input filter and said input
delay element arranged in a first series combination" (the design's line plus one-pole could meet
that), then "each of said first series combination outputs fed into a summing amplifier" with
"said summing amplifier having an output fed back into each of said input summing nodes", plus "N−1
first minor loop feedback attenuators" feeding each output back to its own node with the opposite
sign to the global feedback, and "a feedback summing node ... having no feed from said input signal
port" whose delayed and filtered output goes to "said output signal port". **Missing:** the single
summing-amplifier output fed back into every input node and the sign-conditioned minor loops; the
design feeds each line a different Hadamard row, not one shared sum. Claim 16 has the same
structure ("summing and weighting said first combination outputs and feeding back into one or more
of said input summing node inputs and feeding into a feedback summing node input"; "negatively
feeding back each of said first combination outputs ... if ... positively fed back"). The design's
4 lines without input resemble a node with "no feed of said input signal", but they get distinct
matrix rows, not the summed signal. Prior art predating 2007: Stautner and Puckette 1982, Jot and
Chaigne 1991.

**Row 3, US8351614B2 (STMicroelectronics; priority 2006-02-14). Independent claims 1, 16, 19.
Verdict: Unclear (Plate); Does not read (Room).**

| Claim 1 element | Design |
|---|---|
| "a reverberation generator comprising: an input configured to receive a digital audio signal input;" | Met. |
| "a summing circuit configured to generate a digital audio signal output containing the digital audio signal input and reverberations;" | Met: dry/wet `mix` (1). |
| "a digital audio signal direct path connected to the input and the summing circuit, ... configured to provide the digital audio signal input to the summing circuit without manipulation; and" | Probably met; whether a dry gain from `mix` is "manipulation" is open. |
| "a plurality of feed forward loops configured in a cascade manner, wherein the outputs of the feed forward loops are connected to the summing circuit, a first one of the feed forward loops is connected to the input, and an output of the first feed forward loops is configured to be fed to the summing circuit and an input of a second one of the feed forward loops." | **Decides it.** Room: the 4 series allpasses feed only the next stage and the FDN, never the output sum, so not met. Plate: the two tank halves each hold an allpass, delay, one-pole and gain (the parts dependent claim 3 lists for a loop), half 1 feeds half 2, and output taps from both halves are summed; but the tank recirculates (half 2 feeds half 1), and the taps are inside the delays, not at a loop's output. The description routes no loop output back and contrasts its design with "feedback systems" (fetch-tool quote), so a figure-eight half is probably not a "feed forward loop", but that is a claim-construction question. |

Claims 16 and 19 add extracting "reverberation characteristics" (echo density, build-up rate,
differential decay) from the input signal; the design extracts nothing from its input, so they do
not read. Prior art predating 2006: Dattorro 1997 (the plate topology itself), Griesinger 1989
(unverified), Schroeder 1962.

**Row 4, US10043509B2 (Fraunhofer; priority 2009-10-21). Independent claims 1, 14, 16. Verdict:
Does not read.** Claim 1: "a feedback delay loop processor for delaying at least two different
frequency subband signals comprising a first frequency subband signal being a first time-frequency
tile and a different second frequency subband signal being a second different time-frequency tile";
per subband "a first delay line comprising a first plurality of delay line taps", "a first feedback
loop connected to the first delay line and a first combiner"; "the second loop delay amount being
different from the first loop delay amount." **Missing:** subband (time-frequency tile) signals and
a separate tapped delay line, loop and combiner per subband; the design processes the full band in
one network and frequency-dependent decay comes from one-poles. Claims 14 (method) and 16 (storage
medium) recite the same subband elements. RT60-dependent attenuation is only in dependent claim 11.
Prior art predating 2009: Jot and Chaigne 1991, US5491754A.

**Row 5, US8908875B2 (King's College London; priority 2012-02-02). Independent claims 1, 16, 17.
Verdict: Does not read.** Claim 1 requires "a memory storing dimensions of a virtual room", "a
plurality of junctions distributed on the virtual surfaces of the virtual room, each junction
connected to every other junction by a respective delay line, ... comprises a bidirectional delay
line", "a source injection node", "a receiver node", and "digital surface absorption filters, each
of which is positioned on one of the delay lines to filter the digital audio data only once with
each reflection from a virtual surface". **Missing:** room dimensions, surface junctions with
bidirectional lines, and source and receiver nodes; `size` is a scale factor, not stored room
dimensions, and the per-line one-pole is not tied to a surface reflection. Claim 16 (memory) and
claim 17 (method, "retrieving from the memory dimensions of a virtual room") recite the same
network. Prior art predating 2012 for per-line absorption filters: Jot and Chaigne 1991,
US7149314B2.

**Row 6, US10559295B1 (Abel; priority 2017-12-08). Independent claims 1, 5, 8, 10, 13. Verdict:
Reads (claim 10).**

| Claim 10 element | Design |
|---|---|
| "A system for artificial reverberation comprising" | Met: the Reverb node. |
| "a perceived room size control, wherein the perceived room size control specifies a perceived room size that is different than a nominal room size, and" | Met: `size` 0.5 to 2 relative to size 1 (1). |
| "a network of delay lines, each having a nominal length associated with the nominal room size, and wherein" | Met: Room's 8 lines with base lengths, and Plate's lengths at 29 761 Hz scaled to the rate (2, 3). |
| "the length of at least one of the delay lines is adjusted from the nominal length according to the perceived room size control." | Met: `size` scales every delay length. The claim does not require changes while running, so stopped-only does not avoid it. |

Claims 1 and 5 convolve with a resampled or adjusted room impulse response (no convolution in the
design: do not read); claim 8 adjusts decay time for "a decay time associated with air absorption"
(not in the design); claim 13 uses "a set of resonant filters representing modes of a room" (not in
the design). Dependent claim 11 (feedback filter adjusted by size "to substantially achieve a
reverberation time determined by the perceived room size control and the nominal reverberation
time") may also read, since the per-line gain depends on the scaled m_i, but the design's RT60 is
set by `decay` alone. Prior art predating 2017-12-08: US7561699 (priority 1998-11-13; claim 1 scales
delay and decay by a room-size factor, per a fetch summary) and US6978027B1 (priority 2000-04-11;
delay lengths varied with virtual room size). Both are expired; whether they invalidate claim 10 is
for counsel.

**Row 7, US10311845B2 (Casio; priority 2017-03-15). Independent claims 1, 8, 10. Verdict:
Unclear.**

| Claim 1 element | Design |
|---|---|
| "a set of filters which includes a plurality of partial filters and which forms a characteristic by combining at least two filters selected from among the plurality of partial filters; and" | **Decides it.** The node holds two engines, each built of many filters, and the output comes from one engine except during a fade. Met if "partial filters" can be the component filters of both engines; not met if they must be units whose combination forms the steady-state characteristic. |
| "a processor which performs, as crossfading processing for a first filter and a second filter selected from among the plurality of partial filters, fade-out processing ... of the first filter ... and fade-in processing ... of the second filter ..., when an instruction is provided for changing the characteristic formed by the set of filters," | Met in substance by element 4: on an algorithm change the old engine fades out and the new one fades in over a fixed ramp. |
| "wherein the processor selects a partial filter used for characteristic formation before the instruction as the first filter, sets a parameter to at least one backup partial filter ... corresponding to the characteristic indicated by the instruction, and selects the at least one backup partial filter as the second filter for the crossfading processing." | Probably met if the incoming engine (idle until the change) counts as a "backup partial filter" configured for the chosen algorithm; turns on the same construction. |

Claims 8 (method) and 10 (storage medium) recite the same crossfade and backup-filter elements.
Dependent claim 4 ("delay recursive filters" forming "a predetermined reverberation
characteristic", switched "to an unused delay recursive filter ... by using crossfading") is the
closest to the design. Prior art predating 2017: US5553150A (priority 1993; cross-faded read
positions in a reverberator), US6978027B1 (priority 2000; crossfade between read taps).

**Row 8, EP4468748A1 (Harman; priority 2023-05-22; pending application, published claims judged,
which may change before grant). Independent claims 1, 18. Verdict: Unclear.**

| Claim 1 element | Design |
|---|---|
| "receiving an input audio signal to be output to a user with reverberation;" | Met. |
| "obtaining a first set of reverberation parameters associated with a first acoustic environment;" / "obtaining a second set ... associated with a second acoustic environment, different from the first set" | Probably met: algorithm choice plus shared controls before and after a change (dependent claim 9 allows "a user-induced parameter change"). |
| "processing the input audio signal based on the first set ... to generate a first early reflections signal, and based on the second set ... to generate a second early reflections signal, simultaneously, wherein each ... comprises a respective synthesized audio signal representing initial sound reflections in the respective acoustic environment;" | **Decides it.** Both engines run during the fade, but each produces a whole diffuse reverb with no separate early-reflection stage; the claim treats the tail separately (dependent claim 4 switches it). Met only if a complete reverb output counts as an "early reflections signal". |
| "combining the first and second early reflections signals through a fading process to generate a transitional early reflections signal ..." | Met if the above is met: fixed-ramp crossfade (4). |
| "providing an output audio signal comprising a combination of the direct input audio signal and the transitional early reflections signal." | Met: dry/wet `mix`. Claim 18 ends "providing an output audio signal based on the transitional early reflections signal" (no direct signal needed). |

Prior art predating 2023: US5553150A, US6978027B1 (crossfades inside one reverberator); Greenblatt,
Abel and Berners (date not read).

**Row 9, US20240379089A1 (Nokia; priority 2023-05-08; pending application, published claims
judged). Independent claims 1, 13. Verdict: Does not read.** Claim 1: "providing at least one
control line comprising a delay line and a gain filter, the at least one control line using the at
least one reverberation parameter to generate an output from the at least one control line"; and
"wherein at least a portion of late reverberation in the generated at least one reverberated audio
signal interfering with at least one reflection echo is substantially attenuated." **Missing:** a
control line whose output suppresses part of the late reverberation; the pre-delay is a delay but
attenuates nothing. Claim 13 (apparatus) is the same.

**Row 10, US12407999B2 (Nokia; priority 2021-12-03). Independent claims 1, 10. Verdict: Does not
read.** Claim 1: "obtain directivity data having an identifier, wherein the directivity data
comprises data for at least two separate directions;", "determine averaged gain data based on the
gain data; and", "generate a bitstream defining a rendering, the bitstream comprising the averaged
gain data and the at least one room parameter". **Missing:** directivity data and a bitstream; the
design has neither. Claim 10 (method) is the same.

## Search log

All searches 2026-11-13. The search tool returns a prose summary, not raw result lists, so hit
counts are approximate (about 10 per query unless noted).

| Database | Query | Date | Hits | Result |
|---|---|---|---|---|
| Web (general) | 1. patent feedback delay network reverberator Hadamard feedback matrix absorptive filters Jot | 2026-11-13 | 10 | No patent; JOS and papers |
| Web (general) | 2. patent Griesinger reverberation modulated allpass figure-eight tank Lexicon | 2026-11-13 | 10 | US5109419, US5729613 (cited reference) |
| Web (general) | 3. Dattorro "Effect Design Part 1" Reverberator and Other Filters JAES plate reverb | 2026-11-13 | 10 | Paper citation; no patent |
| Web (general) | 4. Jot Chaigne "Digital delay networks for designing artificial reverberators" AES 90th Convention 1991 | 2026-11-13 | 10 | AES entry; US8204240 (print link), US5491754, USRE39189 |
| Web (general) | 5. patent artificial reverberation crossfade between two reverberation algorithms switching reverb type | 2026-11-13 | 10 | US10311845, EP4468748, US5553150, US6978027; papers |
| Web (general, extended) | 6. Google Patents "feedback delay network" reverberator unitary feedback matrix delay lines lowpass filter per delay line reverberation time claim | 2026-11-13 | 10 | US7149314, US20020067836, US20090003614 (= US8204240), US8908875, US20240379089, US20060116781 |
| Web (general) | 7. Lexicon Griesinger patent reverberator allpass modulated delay "time variant" reverberation unit US patent 1980s 1990s | 2026-11-13 | 10 | US5109419 only; no reverberator-topology patent |
| Web (general) | 8. patent reverberator allpass filter delay modulated by low frequency oscillator interpolated read tank loop plate reverb digital | 2026-11-13 | 10 | No patent; open-source code, Dattorro |
| Web (general) | 9. patent reverberator "Hadamard" feedback matrix delay lines Walsh-Hadamard transform reverb | 2026-11-13 | 10 | US8345887 |
| Web (general) | 10. Schroeder "Natural sounding artificial reverberation" JAES 1962 allpass; Schroeder Logan "Colorless artificial reverberation" 1961 | 2026-11-13 | 10 | Papers, AES entries |
| Web (general) | 11. Dattorro reverberator patent OR Eventide OR Lexicon "plate" reverberation figure-eight loop allpass diffusers decay diffusion US patent | 2026-11-13 | 10 | No Dattorro, Eventide or Lexicon plate patent |
| Web (general) | 12. patent reverberation effect modulated allpass delay chorus in feedback loop prevent metallic ringing guitar pedal digital reverb granted | 2026-11-13 | 10 | US8351614, US7149314, US3110771; no modulated-allpass patent |
| Web (general, extended) | 13. Jot DTS OR Dolby OR Creative patent feedback delay network reverberator "reverberation time" per delay line gain filter granted 2010..2024 | 2026-11-13 | 10 | WO2011057868, US20170323632, JP6487383, US12407999, US11651762 (not opened) |
| Web (general) | 14. patent reverberation "crossfade" OR "cross-fade" between reverb algorithms preset change two reverberators running simultaneously fade audio effect processor | 2026-11-13 | 10 | No patent; manuals, forums, MorphVerb review |
| Web (general) | 15. patent reverberator delay line lengths scaled by room size parameter "size" delay lengths scaling reverberation sample rate digital effect | 2026-11-13 | 10 | US10559295, US6978027, US7561699 |
| Web (general, domain-limited: patents.google.com, patft.uspto.gov, freepatentsonline.com, patents.justia.com) | 16. Lexicon Inc patent reverberator "Griesinger" inventor US patent digital reverberation allpass loop | 2026-11-13 | 9 | US5109419, USRE39189E1, US8391504B1, WO1995010831A1; Lexicon US5136650A and US4862502A named in a snippet, not opened |
| Web (general, domain-limited: patents.google.com, patents.justia.com, freepatentsonline.com) | 17. patent reverb "feedback delay network" Eventide OR Strymon OR Universal Audio OR Native Instruments OR Valhalla OR Softube reverberation modulated delay lines | 2026-11-13 | 9 | CN105916095A (not opened), US4268717A; no vendor patent found |
| Google Patents (page fetch by number) | US5109419A, US8204240B2, US5491754A, US10311845B2, EP4468748A1, US7149314B2, US8345887, US8908875, US20020067836A1, US6978027, US8351614B2, US5553150A, US20240379089A1, US7062337B1, US20060086237A1, US10559295, US7561699B2, US20170323632A1, US8041045B2, US12407999B2, US10043509B2, US8391504B1, US12175956B2, US4446530A, US4268717A, US3110771A | 2026-11-13 | 26 | Dates and status read; claims read only in fragments or paraphrase; many pages truncated at 100000 characters |
| Google Patents (second page of long documents, offset 100000) | US10311845B2, EP4468748A1, US8204240B2, US10043509B2 (offset 100000), US8345887 (offset 60000) | 2026-11-13 | 5 | Casio claim 1 and claim 4 seen; EP4468748 claim 1 fragments; US10043509 claim 1 and start of claim 14; US8204240 citation list (no claim text); US8345887 gave citing-patent list only |
| Google Patents (stage 2, claims by number) | US8345887B1, US10559295B1, US8204240B2, US8351614B2 (claims and description of "feed forward loop"), US12407999B2 (offset 100000), US10043509B2 (offsets 100000, 200000), US8908875B2 (offset 100000), US10311845B2 (offset 100000), EP4468748A1 (offset 100000), US20240379089A1 (offset 100000) | 2026-11-13 | 10 | All independent claims of rows 1 to 10 read in quoted pieces; claim 14 of US10043509B2 read across both offsets; US8908875B2 claim 16 text garbled on the page ("source unidirect junctions, ional") |
| JOS online book (pages) | History of FDNs; Achieving Desired Reverberation Times; Schroeder Allpass Sections; FDN Reverberation (index); Hadamard Matrix (DSPRelated) | 2026-11-13 | 5 | Read |

Refused or failed: the CCRMA PDF of Dattorro (scanned image, no text), AES E-Library pages (id 5663
returned HTTP 403 on aes.org; ids 5663 and 10160 on aes2.org redirected to aes.org, and 10160 on
aes.org returned HTTP 403), the USPTO print PDF for 12407999 (image-only, unreadable), Google
Patents US7561699B1 (HTTP 404; US7561699B2 worked), JOS "Damping Filters for Reverberation Delay
Lines" under the URL I tried (HTTP 404). Not tried: Google Patents search UI, Espacenet, USPTO
Patent Public Search and Patent Center, FreePatentsOnline, Crossref for the AES papers. No search
was run by CPC class (for example G10K 15/08, G10H 1/00, G10H 2250/111, G10H 2250/281, H04S 7/30)
or by inventor in a database; every candidate came through a general web search, a citation list on
a fetched page, or a fetch by number. Citation lists were read for US7149314B2 (7 backward, 36
forward, partly), US8345887B1 (4 backward), US6978027B1 (14 backward), US8351614B2 (13 backward),
US8204240B2 (4 backward, 2 direct forward), US10559295B1 (selected), and US5109419A (9 backward, 85
forward, all Lexicon/Harman room-enhancement and loudspeaker patents, none opened).

## Open uncertainties

- No patent-database search was run (see "Refused or failed"); "no candidate found" for Dattorro's
  topology, modulated-allpass plate tanks and crossfade between complete reverb engines is a
  statement about web-search coverage, not about the databases.
- The Dattorro 1997 paper was not read as text, so the plate topology's constants (29 761 Hz
  reference rate, tap positions, diffuser coefficients) and whether the paper itself gives LFO
  modulation of tank allpasses and linear-interpolated reads are unestablished here; descriptions
  are from secondary pages. The Jot and Chaigne 1991, Stautner and Puckette 1982, Gerzon 1971,
  Schroeder 1961 and 1962, and Griesinger 1989 publications were not opened; dates come from index
  entries or search summaries.
- No source found for: stereo input fed to alternate FDN lines and +/- sign output patterns (element
  2); whole-frame pre-delay and pre-allocating buffers for size 2 (element 1); a dated source for
  the one-pole-per-line damping filter form (the JOS page was not read); a fixed-time crossfade
  between two complete reverb engines (element 4).
- Stage 2 claim quotes came from Google Patents HTML through the fetch tool's summarising model, in
  pieces; none was compared with the granted PDF or USPTO text. A human should check the quotes the
  verdicts rest on (row 1 element 1d, row 3's "feed forward loops" element, row 6 claim 10, row 7
  claim 1, row 8 claim 1) against the PDFs.
- Row 6 (US10559295B1) claim 10 reads on `size`. A human must decide: take advice on validity over
  US7561699 and US6978027B1 (both earlier, both expired), drop or fix `size`, or accept the risk.
  Stopping-only changes do not avoid the claim.
- Row 1 (US8345887B1) turns on element 1d. A human must decide whether the Room's L/R sign patterns
  may be rows of the feedback Hadamard matrix (then 1d is met), or must be taken from the delay-line
  outputs with patterns that are not matrix rows (no literal reading; equivalence not assessed).
  Whether Stautner and Puckette 1982 or Jot and Chaigne 1991 take outputs after the matrix was not
  established.
- Row 3 (US8351614B2): whether a half of Dattorro's recirculating figure-eight tank is a "feed
  forward loop" "configured in a cascade manner", and whether a dry gain is "manipulation" of the
  direct path, are claim-construction questions for counsel; Dattorro 1997 predates the priority.
- Row 7 (US10311845B2): whether two whole reverb engines, only one of which forms the output outside
  a fade, are "partial filters" of a "set of filters" that "forms a characteristic by combining at
  least two filters", and whether the idle engine is a "backup partial filter".
- Row 8 (EP4468748A1, pending): whether a complete reverb output is an "early reflections signal";
  the claims may change before grant, and US 18/669,943 and other family members were not read.
- Row 9 is a pending application; its claims may change before grant and should be rechecked if it
  issues.
- Seen in a search result or citation list but not opened, so no date or claim is stated and they
  may belong in Candidates: US11651762 ("Reverberation gain normalization"), CN105916095A
  ("Feedback delay network timbre optimization method"), WO2011057868A1 and JP6487383B2 (Fraunhofer
  family, member of row 4), US9805704B1 (Abel, modal-decomposition reverberation), US20060116781A1
  ("Artificial ambiance processing system"), USRE39189E1, US5729613 and WO1995010831A1 (Industrial
  Research Limited, reverberators for assisted reverberation), US5136650A and US4862502A (Lexicon,
  "Sound reproduction"), US20170238119A1 and US9961473B2 (Fraunhofer, citing US8204240B2),
  US5740716 (Stanford, length-modulated delay line), US20130202125A1 (De Sena, family of row 5).
  A human or stage 2 should open these.
- The US8345887B1 page printed only a partial citing list; its forward citations were not read.
- Term end dates are as Google prints them; terminal disclaimers and adjustments not otherwise
  seen. Rows 8 and 9 are applications, so any term depends on grant.
