# ADR-0052 — A duplex stream is two cpal streams joined by a sample ring

- **Status:** Accepted
- **Decided by:** agent, ratified by Chris Paro
- **Date:** 2026-11-07
- **Issue:** #52
- **Architecture:** [Audio device I/O](../architecture/platform.md#audio-device-io)

## Context

`platform_interfaces` promises one duplex callback per buffer, with input and output together
(ADR-0036 needs both on one clock). `cpal` 0.18 has no duplex stream: input and output are
separate streams with separate callbacks, even on the same device.

## Decision

- A duplex stream opens a `cpal` input stream and output stream on the same device, at the same
  rate and buffer size.
- The input callback pushes its samples into a single-producer, single-consumer ring of
  `AtomicU32` sample slots (4 buffers deep), written in safe code. Input that does not fit is
  dropped.
- The output callback takes one buffer of input from the ring (silence for any shortfall), runs
  the audio callback with both, and records how many input frames are still waiting. The reported
  input latency adds them.
- Input-only and output-only streams run the audio callback directly from their one stream.
- Callbacks report errors and latencies through atomics only.

## Alternatives considered

- **Driving a CoreAudio AUHAL unit with input and output enabled ourselves:** a true duplex
  callback, but more `unsafe` code beside `cpal`, against ADR-0009.
- **A ring of `audio_buffers` blocks:** needs pools and return queues for a stream of samples; the
  sample ring is simpler.

## Consequences

- Input reaches the callback up to a few buffers late. The reported latency includes the delay,
  so recording alignment (#67) can compensate.
- The duplex path needs microphone permission to test, so it is not covered by automated tests.
  The output path and the ring are.
