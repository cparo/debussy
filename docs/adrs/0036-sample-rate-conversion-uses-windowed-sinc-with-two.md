# ADR-0036 — Sample-rate conversion uses windowed sinc with two quality presets and drift correction

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-06
- **Issue:** #30
- **Architecture:** [Sample rate](../architecture/engine.md#sample-rate)

## Context

Each project has one sample rate ([ADR-0006](0006-each-project-has-one-fixed-sample-rate.md)), so
audio is resampled once on import, and in real time wherever a device runs at another rate.
`cpal` delivers input and output as separate streams
([ADR-0009](0009-audio-device-i-o-uses-cpal-behind-our-own.md)), which may come from separate
devices with independent clocks. The algorithm, quality target and drift handling were put to the
developer.

## Decision

- **Algorithm:** bandlimited interpolation with a polyphase Kaiser-windowed sinc table, linearly
  interpolated between phases, so any ratio works, including one that changes over time. One
  design serves import and the device boundary.
- **Quality, in two presets of the same algorithm:**
  - *Import* (offline): passband flat to 20 kHz within ±0.01 dB at a 44.1 kHz output, stopband
    attenuation at least 120 dB.
  - *Device* (real time): passband to 20 kHz, stopband at least 96 dB, with a shorter filter for
    lower latency and CPU cost.
- **Drift:** at the device boundary the ratio adapts. A slow proportional-integral loop on the fill
  level of the FIFO between the device and the engine keeps it from overflowing or running dry.
- **Latency:** each converter reports its delay, so delay compensation
  ([ADR-0022](0022-the-engine-compensates-recording-and-effect.md)) includes it.

## Alternatives considered

- **Rational polyphase only** — exact ratios are slightly cheaper, but cannot follow clock drift,
  and awkward ratios need large tables.
- **Cubic interpolation** — very cheap, but it aliases audibly and loses high frequencies.
- **One quality setting** — 120 dB everywhere costs latency and CPU on the real-time path for no
  audible benefit; 96 dB everywhere would short-change imports, which are converted once and kept.
- **No drift correction** — separate devices would drift apart and click or drop out
  periodically; users would have to use one interface for input and output.

## Consequences

- The patent evidence for these techniques must be recorded in an ADR under
  `docs/architecture/patents.md` "Patent policy" before they are built.
- The device preset must run within the 32-frame worst case
  ([ADR-0007](0007-the-engine-targets-a-128-frame-default-buffer.md)); its filter length is set
  by measurement.
- The drift loop's constants are tuned so ratio changes stay inaudible: slow, smooth corrections
  rather than jumps.
