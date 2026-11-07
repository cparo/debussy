# Patent status of windowed-sinc resampling and fill-level ratio control

Research note, 2026-11-06. This is not legal advice. It records what primary sources show, so that
a reviewer can check it. Patent texts are USPTO scans, read by OCR (macOS Vision); quotes below are
from that OCR and should be checked against the PDF before they are relied on.

**Outcome (2026-11-06):** the developer chose to design around these patents: exact rational
polyphase filtering, with no coefficient interpolation and no drift correction
([ADR-0036](../adrs/0036-sample-rate-conversion-is-exact-rational-polyphase.md)).

## Question

Are there unexpired patents, or patent claims that are not royalty-free, on the two techniques
planned for the sample-rate converter?

1. Bandlimited interpolation by a Kaiser-windowed sinc held in a polyphase table, with linear
   interpolation between table phases so that any ratio (including an irrational one) works: the
   method of Smith & Gossett (ICASSP 1984) and Smith's Digital Audio Resampling Home Page.
2. A slowly varying resampling ratio, driven by a proportional-integral (PI) loop on the fill level
   of the FIFO between an input device and the engine, to absorb clock drift.

## Summary

| Technique | Verdict | Confidence |
|---|---|---|
| Windowed-sinc, interpolated table | The method was published in 1984 (code dated 1983), so any patent on it as published has expired. **But three unexpired US patents filed in 2009–2010 have independent claims that read on it as worded**: Audience US 8,405,532 B1 (to about 2031-04), Conexant US 8,253,610 B2 (to about 2029-06) and, arguably, TI US 8,369,973 B2 (to about 2031-07). On their face all three are anticipated by Smith & Gossett 1984, but that is a validity argument, not expiry. | Facts: high. Legal outcome: not determinable here |
| Kaiser window | Published by 1974 (and by Kaiser & Schafer 1980). No patent found; any patent would have expired. | High |
| Fill-level PI ratio control | Published software prior art: PulseAudio 2006 (proportional), JACK `alsa_in` 2009-03 (PI on ALSA delay). **One unexpired patent has a broad independent claim that may read on it**: Qualcomm US 8,217,812 B2 claim 1 (to about 2030-09), whose dependent claims 8–10 add "monitoring at least one buffer". JACK's PI code predates its priority date by more than a year. Other live ASRC patents found need hardware or loop structures the design lacks. | Medium |

**What would block calling the design unencumbered:**

- Under the patent policy (`docs/architecture/patents.md` "Patent policy") neither technique can
  be recorded as "every covering patent expired". US 8,405,532, US 8,253,610, US 8,369,973 and
  US 8,217,812 are unexpired and their legal status (maintenance fees, reexamination, IPR) could not
  be checked. Relying on 1984/2006/2009 prior art to treat them as invalid is a human decision via
  `flag-risk` (tag `patent`).
- Google Patents refused every query (HTTP 503), and Espacenet and FreePatentsOnline returned 403,
  so there was no structured CPC or full-text search. Candidates came from web search and from
  patents' own citation lists; others may exist.

## Findings

### 1. Windowed-sinc with interpolated coefficients (Smith & Gossett)

- **Paper.** J. Smith and P. Gossett, "A flexible sampling-rate conversion method", ICASSP '84,
  San Diego, vol. 9, pp. 112–115, DOI 10.1109/ICASSP.1984.1172555 (Crossref metadata; the IEEE
  Xplore page itself did not load). <https://api.crossref.org/works/10.1109/icassp.1984.1172555>
- **Smith's tutorial** says it "is an expansion of the conference paper in [SmithAndGossett84]".
  <https://ccrma.stanford.edu/~jos/resample/Abstract.html> The introduction says "a public-domain
  resampling algorithm is described which will evaluate a signal at any time specifiable by a
  fixed-point number", and "Continuity of the impulse response is simulated by linearly
  interpolating between samples of the impulse response stored in a table."
  <https://ccrma.stanford.edu/~jos/resample/Introduction.html>
- **Table and interpolation.** The implementation stores the right wing of a symmetric FIR plus a
  table of differences "to speed up the linear interpolation"; the coefficient used is
  h(l) + η·h̄(l), where η is the fractional field of the time register.
  <https://ccrma.stanford.edu/~jos/resample/Implementation.html>
- **Original code**, "Released under the BSD License", cites "A Flexible Sampling-Rate Conversion
  Algorithm, J. O. Smith and P. Gossett, ICASSP, San Diego, 1984"; its modification history starts
  "8/27/83 - JOS". <https://ccrma.stanford.edu/~jos/resample/original-source.html>
  The C `resample` 1.8.1 is LGPL. No page on the site mentions patents.
  <https://ccrma.stanford.edu/~jos/resample/Free_Resampling_Software.html>
- The Kaiser window is cited by Smith to "J. F. Kaiser, Using the I0-sinh window function", pp.
  20–23, April 22–25 1974. Smith's entry names *IEEE Trans. Circuits and Systems*, but the
  four-day date range suggests symposium proceedings; the original was not seen.
  <https://ccrma.stanford.edu/~jos/sasp/Kaiser_Window.html>,
  <https://ccrma.stanford.edu/~jos/sasp/Bibliography.html>. Crossref also lists Kaiser & Schafer,
  "On the use of the I0-sinh window for spectrum analysis", IEEE TASSP, Feb. 1980, DOI
  10.1109/TASSP.1980.1163349. No Kaiser-window patent was found (Google Patents unavailable).
- **US 8,405,532 B1**, Audience, Inc. (inventors Clark, Massie). Provisional 61/181,988 filed
  2009-05-28; application 12/790,552 filed 2010-05-28; granted 2013-03-26; 321 days PTA. Estimated
  expiry 2031-04-14 if fees are paid. Claim 1 (method; claim 11 is the same on a storage medium):
  "adding a time increment value to a current time register ... [that] includes a current time
  integer value and a current time fractional value; determining a starting address in a data table
  based on the current time integer value; performing interpolation of coefficients related to a
  finite impulse response filter to generate interpolated coefficients; and configuring a finite
  impulse response filter from the interpolated coefficients." This is the planned design, and the
  1983/1984 method. The only reference cited on its front page is US 6,279,019 (Oh); Smith &
  Gossett was not before the examiner.
  <https://image-ppubs.uspto.gov/dirsearch-public/print/downloadPdf/8405532>
- **Conexant family** (Jonsson, Thorvaldsson, Thormundsson), all from 12/488,322 filed 2009-06-19:
  - US 7,920,078 B2 (granted 2011-04-05, 61 days PTA, about 2029-08-19). Every independent claim
    needs a "small fraction approximator" that uses the nearest phase when the offset is within a
    "predetermined threshold". A design that always interpolates does not have this.
    <https://image-ppubs.uspto.gov/dirsearch-public/print/downloadPdf/7920078>
  - **US 8,253,610 B2** (continuation, filed 2011-02-17, granted 2012-08-28, 0 days PTA, about
    2029-06-19). Claim 1: "a poly-phase filter having a nominal input sampling rate, an output
    sampling rate and a plurality of phases, each phase ... comprising a plurality of filter
    coefficients ...; and an approximator configured to approximate the poly-phase filter at a phase
    offset derived from an input sample having a variable rate." Claim 3: "the interpolator is a
    linear interpolator." Claim 9 is the method form. These read on the design as worded.
    <https://image-ppubs.uspto.gov/dirsearch-public/print/downloadPdf/8253610>
  - US 8,872,678 B2 (continuation, granted 2014-10-28, 295 days PTA, about 2030-04-10). Claims 1
    and 11 again require snapping to a nominal coefficient "if said input sample occurs within a
    first predetermined time"; not met by always interpolating.
    <https://image-ppubs.uspto.gov/dirsearch-public/print/downloadPdf/8872678>
- **US 8,369,973 B2**, Texas Instruments (Risbo). Provisional filed 2008-06-19; application
  12/397,521 filed 2009-03-04; granted 2013-02-05; 871 days PTA; about 2031-07-23. Claim 1:
  determining whether the input rate is lower or higher than the output rate and converting
  "according to a first sample rate conversion algorithm" in each case (the printed claim says
  "first" in both branches), then "calculating digital filter coefficients ... by evaluating
  polynomial expressions for the digital filter coefficients responsive to the phase offset". The
  spec means piecewise polynomial approximations of the impulse response; h + η·h̄ is a degree-1
  polynomial per table interval, so whether it reads is arguable. Claims 20, 31, 32 need
  time-stamp counter circuitry. <https://image-ppubs.uspto.gov/dirsearch-public/print/downloadPdf/8369973>
- **US 7,417,569 B2**, Broadcom, filed 2006-11-30, 0 days PTA. Claims a phase accumulator that
  adds a ratio's numerator and takes the modulus by its denominator. **Expired for non-payment of
  maintenance fees** (checked in Google Patents and USPTO records by Chris Paro, 2026-11-06).
  <https://image-ppubs.uspto.gov/dirsearch-public/print/downloadPdf/7417569>

### 2. Adaptive ratio from FIFO fill level, PI loop

- **Adriaensen**, "Controlling adaptive resampling", LAC 2012 (April 12–15, CCRMA). The delay error
  "is first processed by a second order lowpass filter and then becomes the input to the second
  order loop filter"; it says of `alsa_in`/`alsa_out` that none "really gets this right".
  Published after the Qualcomm priority date, so not prior art to it.
  <https://lac.linuxaudio.org/2012/papers/23.pdf>, <https://lac.linuxaudio.org/2012/>
- **zita-resampler** `VResampler` "has to interpolate filter coefficients"; source is
  "Copyright (C) 2006-2023 Fons Adriaensen", GPL-3.0-or-later. A GPL licence gives a separate Rust
  implementation no patent comfort, and no code from it should be copied.
  <http://kokkinizita.linuxaudio.org/linuxaudio/zita-resampler/resampler.html>,
  <http://kokkinizita.linuxaudio.org/linuxaudio/downloads/zita-resampler-1.11.2.tar.xz>
- **JACK `alsa_in`** (Torben Hohn), jack1 commit `8e8412ab3c`, authored 2009-03-03: reads
  `snd_pcm_delay()`, takes `offset = delay - target_delay`, and computes the libsamplerate ratio
  with the comment "ok. now this is the PI controller. // u(t) = K * ( e(t) + 1/T \int e(t') dt' )".
  The 2008-04-09 version (`7170803622`) already drove the ratio from the same delay error with a
  cubic law. jack1 tools are GPL. Git author dates are self-reported and were not checked against
  the original svn repository.
  <https://github.com/jackaudio/jack1/blob/8e8412ab3c21e0b84eb7ab298c123e08c0723f76/tools/alsa_in.c>
- **PulseAudio `module-combine`**, commit `c25c549c` (2006-04-13): `adjust_rates()` compares each
  output's total latency with a target and changes its rate in proportion to the difference.
  <https://gitlab.freedesktop.org/pulseaudio/pulseaudio/-/blob/c25c549c/src/modules/module-combine.c>
- **Older patents describing fill-level or buffer-count control**, all expired:
  - Sony EP 0 665 546 B1, priority 1994-01-26, published 1995-08-02; Rohde & Schwarz US 6,559,781
    describes it as "regulating the sampling rate ratio ... as a function of a fill level of the
    buffer store". <https://data.epo.org/publication-server/rest/v1.2/patents/EP0665546NWB1/document.html>,
    <https://image-ppubs.uspto.gov/dirsearch-public/print/downloadPdf/6559781>
  - Apple US 5,875,354, filed 1996-03-01 (rate of conversion modified from buffer count).
    <https://image-ppubs.uspto.gov/dirsearch-public/print/downloadPdf/5875354>
  - R&S US 6,559,781 (DE priority 2001-01-18), claim 3 a "proportional controller" on phase.
  - Creative US 6,819,732 (filed 2000-08-22, 845 days PTA, about 2022-12).
  - TI US 7,345,600 (Fedigan; filed 2006-03-09, about 2026-03-12), Microsoft US 7,620,137 (filed
    2004-11-13, 499 days PTA, about 2026-03-27), Altera US 7,369,637 (filed 2004-06-04, 742 days
    PTA, about 2026-06-16) and its terminal-disclaimed continuation US 7,680,233, Nvidia US
    7,414,550 (filed 2006-06-30, about 2026-09-06).
  - Apple US 8,904,066 / US 10,558,422 (Lengeling family): terminal-disclaimed continuations back
    to 10/877,762 filed 2004-06-25, so about 2024-06 (estimate).
- **US 8,217,812 B2**, Qualcomm (Wang, Balatsos). Provisional filed 2010-03-11; application
  12/774,398 filed 2010-05-05; granted 2012-07-10; 136 days PTA; about 2030-09-18. Claim 1:
  "generating a phase difference signal based on comparison of a source clock of the input signal
  to a local clock, wherein the phase difference signal is generated by updating at least a
  fractional phase component of the comparison; and compensating for a detected clock rate
  mismatch ... includes adjusting a conversion rate of at least one output sample ... based on the
  phase difference signal." Claims 8–10: generating it "comprises monitoring at least one
  [input/output] buffer". A FIFO fill error is arguably such a signal; whether "updating a
  fractional phase component" is met depends on claim construction. Claims 14 and 35 also need
  separate up- and down-sampling stages; claims 45 and 55 need "non-approximated integer
  components". JACK's PI loop (2009-03-03) predates the provisional by more than one year.
  <https://image-ppubs.uspto.gov/dirsearch-public/print/downloadPdf/8217812>
- **Live, but claims not met by the design:**
  - Sonova US 10,534,573 B2 (filed 2018-05-29, about 2038-07). Claims 1 and 12 need receiving "by
    an antenna"; claim 14 needs an SRC "receiving data words at an adjustable period T" set by the
    input-buffer fill level. Later than the JACK and PulseAudio prior art.
    <https://image-ppubs.uspto.gov/dirsearch-public/print/downloadPdf/10534573>
  - TI US 9,014,321 B2 (filed 2011-11-09, about 2033-07). Claim 1 applies one of two clock drift
    compensation values to "a clock rate for the buffer" above or below a fullness threshold.
    <https://image-ppubs.uspto.gov/dirsearch-public/print/downloadPdf/9014321>
  - Actiwave US 9,166,563 B2 (PCT filed 2012-10-25, about 2032-10). Claims 1 and 9 need sigma-delta
    loops and a count-disparity counter; the FIFO fill controller is only dependent claim 3.
    <https://image-ppubs.uspto.gov/dirsearch-public/print/downloadPdf/9166563>
  - Audience US 8,405,532 claims 16 and 19 correct a ratio of time increments measured from sample
    arrival times, not a fill level.

## Patent search log (2026-11-06)

| Source | Query or action | Result |
|---|---|---|
| Google Patents `xhr/query` and `/patent/` pages | `"adaptive resampling"` + fill level, and a single patent page | 503 "automated queries" on first request; not used |
| Web search | Smith & Gossett title; title on ieeexplore.ieee.org | Citations only; Xplore record not returned |
| Crossref API | title + author; Kaiser I0-sinh | DOI 10.1109/ICASSP.1984.1172555; Kaiser & Schafer 1980 |
| Web search (extended) | SRC ratio from buffer fill level, clock drift | US 9,166,563; 6,747,581; 7,680,233; 7,061,409; 7,345,600; 8,217,812; 7,369,637; 8,405,532; 7,970,088 |
| Web search (extended) | `"resampling ratio" "fill level"` PI controller | US 10,534,573; 6,559,781; 8,904,066; 9,014,321; 7,620,137 |
| Web search | Apple adaptive SRC buffer level | US 5,875,354; Lengeling family |
| Web search | ADI / Cirrus ASRC ratio estimation | US 7,345,600; 6,092,126; 6,819,732 (no ADI hits) |
| Web search (extended) | interpolating filter coefficients, polyphase, time register | US 8,405,532; 6,462,682; EP 1458097 A1 |
| Web search | Risbo US 2009/0319065 | Granted as US 8,369,973; also 7,152,086; 7,417,569; 7,414,550; 6,208,671 |
| USPTO `image-ppubs` PDFs | front page, and claims where relevant, of every US patent above | Read by OCR |
| Front-page citations | US 8,253,610 cites US 2009/0319065 (Risbo) | Led to US 8,369,973 |
| EPO publication server | EP 0 665 546 B1 | Read (A1 returned 500) |

Not read or not checked: US 6,747,581 (Octiv, filed 2002-02-01, expired about 2023-02), US
7,061,409 (Nokia, filed 2005-02-07, expired 2025-02), US 7,970,088 (Intersil, terminal-disclaimed
continuation of a 2004 application), US 7,152,086 (filed 2002-10-01), US 6,208,671 (Cirrus, filed
1998), US 6,462,682 (filed 2001), US 6,092,126 and EP 1458097 A1 (claims not read; dates put them
past expiry on the front pages or search snippets).

**Refused or failed:** Google Patents (503); FreePatentsOnline and Espacenet REST (403); USPTO
maintenance-fee page (CloudFront 403); USPTO assignment API (no response); Patent Center returns
only a JavaScript shell; IEEE Xplore returned 202 with no record; `resample.pdf` has an empty
bibliography.

## Open uncertainties

1. **Legal status** of US 8,405,532, 8,253,610, 8,369,973 and 8,217,812 (fees, lapses,
   reexaminations, IPRs, assignments) was not checked. A lapsed patent would remove the concern.
2. **Validity versus expiry.** Smith & Gossett 1984 and its 1983 BSD code appear to disclose every
   element of Audience claim 1 and Conexant claim 1; JACK 2009 and PulseAudio 2006 disclose
   fill-level ratio control. Invalidity is a legal judgement the policy does not let an agent make.
3. **Claim construction.** Whether "phase difference signal ... updating at least a fractional phase
   component" (Qualcomm) or "evaluating polynomial expressions" (TI) covers the design depends on
   the specifications, which were not fully read.
4. **Search coverage.** No CPC search (H03H 17/06, H03H 17/0628) could be run; continuations of the
   Conexant, Audience and Qualcomm families after their front pages were not searched; non-US
   counterparts (EP, CN, JP) were not checked.
5. **OCR.** Claim text comes from OCR of scans; quoted wording should be confirmed on the PDFs.
6. **Expiry estimates** assume a 20-year term from the earliest non-provisional US filing plus the
   printed PTA, and no terminal disclaimer unless one is printed on the front page.
