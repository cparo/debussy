# ADR-0006 — Each project has one fixed sample rate

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-06
- **Issue:** —
- **Architecture:** [Sample rate](../architecture/engine.md#sample-rate)

## Context

Recordings, imports and the output device may each have a different sample rate. The engine has to
mix them, and resampling during playback costs real-time CPU and adds artefacts.

## Decision

Each project has one sample rate (44.1, 48, 88.2 or 96 kHz), chosen at creation. Imports are
resampled to it once; device rate mismatches are converted at the device boundary.

## Alternatives considered

- **Follow the device** — switching to a device at a different rate would mean resampling every
  clip, or changing the project out from under the user.
- **Per-clip rates, resampled live** — flexible, but spends real-time CPU on every clip, every
  playback.

## Consequences

- A resampler is required both offline (import) and in real time (device boundary). It is
  audio-core code, so it defaults to being built in-house under the dependency policy.
- Changing a project's rate after creation, if ever offered, is an explicit offline conversion.
- The project format must record the project's rate.
