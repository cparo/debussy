# ADR-0036 — Sample-rate conversion is exact rational polyphase filtering without drift correction

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-06
- **Issue:** #30
- **Architecture:** [Sample rate](../architecture/engine.md#sample-rate)

## Context

Each project has one sample rate ([ADR-0006](0006-each-project-has-one-fixed-sample-rate.md)), so
audio is resampled once on import, and in real time wherever a device runs at another rate. The
first design put to the developer was a Kaiser-windowed sinc with coefficients interpolated
between table phases (any ratio), plus a PI loop on FIFO fill to absorb clock drift between
separate devices. Patent research ([research note](../research/resampler-patents.md)) found
unexpired US patents whose independent claims, as worded, read on that design: US 8,405,532
(coefficients interpolated from a fractional time register), US 8,253,610 (a polyphase filter
approximated at a variable-rate phase offset), US 8,369,973 (coefficients from evaluated
polynomials, arguably), and, for drift, US 8,217,812 (fractional-phase rate adjustment and buffer
monitoring). The developer chose to design around them.

## Decision

- **Algorithm:** exact rational polyphase filtering. For input rate `fs_in` and output rate
  `fs_out`, with `L = fs_out / g` and `M = fs_in / g` (`g` their greatest common divisor), the
  converter uses `L` precomputed phases of one Kaiser-windowed sinc lowpass. A phase accumulator
  advances by `M` modulo `L` per output frame. Coefficients are never interpolated or computed at
  run time; ratios are fixed when a converter is made.
- **Quality, two presets** of the same algorithm, with edges relative to the lower rate `f_min`:
  - *Import* (offline): passband to `0.4535 × f_min` (20 kHz at 44.1 kHz) within ±0.01 dB,
    stopband from `f_min / 2`, at least 120 dB down.
  - *Device* (real time): the same passband, stopband from `f_min − passband edge`, at least 96 dB
    down. Only content above the passband can alias, and a shorter filter results.
- **No drift correction.** Input and output must share a clock: the engine runs input and output
  on one device, or on devices the user has aggregated at the system level.
- **Latency:** each converter reports its delay, so delay compensation
  ([ADR-0022](0022-the-engine-compensates-recording-and-effect.md)) includes it.
- **Patent evidence:** polyphase decomposition and rational sample-rate conversion are textbook
  multirate signal processing, published long before any unexpired filing (for example Crochiere
  and Rabiner, *Multirate Digital Signal Processing*, 1983). The Kaiser window was published in
  1974. The one patent found on the exact numerator/denominator phase accumulator, US 7,417,569
  (Broadcom, filed 2006), has expired for non-payment of maintenance fees (checked in Google
  Patents and USPTO records by Chris Paro, 2026-11-06). The design has none of the elements the
  four patents above add: no interpolated or polynomial coefficients, no variable-rate phase, and
  no clock comparison or buffer monitoring.

## Alternatives considered

- **Windowed sinc with interpolated coefficients and adaptive drift correction** — handles any
  ratio and separate devices, but reads, as worded, on the four unexpired patents above. Relying
  on prior-art invalidity arguments was declined.
- **Pause for legal review** — would have blocked import of other-rate files and mismatched
  devices for an unknown time.
- **Apple's AudioConverter through `macos_platform`** — licensed by Apple, but needs AudioToolbox
  FFI (`unsafe`), and it was uncertain whether it supports a continuously varying ratio.
- **Cubic interpolation** — cheap, but it aliases audibly and loses high frequencies.

## Consequences

- Separate input and output devices with independent clocks drift apart; until drift correction
  is revisited (after the patents expire, 2029–2031, or after a legal review), the UI must steer
  users to one interface, or to an aggregate device, for input and output.
- Rates whose ratio needs a very large `L` are refused rather than approximated.
- The patent search was not a professional freedom-to-operate search; the research note lists
  its gaps.
