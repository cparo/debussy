# ADR-0009 — Audio device I/O uses cpal behind our own interface

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-06
- **Issue:** —
- **Architecture:** [Audio device I/O](../architecture/platform.md#audio-device-io)

## Context

The engine must enumerate input and output devices (built-in and USB interfaces such as a MOTU M6
or Focusrite Scarlett Solo), open them at the project sample rate with a fixed buffer size, and
stream audio on the device's real-time thread. This needs Core Audio. Raised as a
`license`/`portability` risk because it adds a dependency. The dependency policy defaults to
**buy** for OS backends.

## Decision

Use `cpal` (0.18, caret version in the library crate that wraps it) inside the macOS platform
crate, behind Debussy's own audio I/O interface. Nothing outside the platform crate sees `cpal`
types.

## Alternatives considered

- **`objc2-core-audio` directly** — 12 crates against `cpal`'s 22, and full Core Audio access
  (one combined input/output callback, channel maps, aggregate devices). Rejected for now: much
  more code, including `unsafe` calls, for control the first features do not need. It stays
  the escape hatch if `cpal` falls short, and the swap is confined to the platform crate.
- **Hand-written Core Audio FFI** — no dependency, but all bindings and `unsafe` code would be
  ours, with no correctness or performance gap to justify building.

## Consequences

- At evaluation (2026-11-06): `cpal` 0.18.2, Apache-2.0, MSRV 1.85, 22 crates on macOS, all
  licences on the allowlist. It uses `objc2-core-audio` underneath.
- Input and output are separate streams. Aligning recordings with playback (tracking over existing
  tracks) is our job, using stream timestamps and reported device latency.
- Selecting individual inputs of a multi-channel interface means opening the device's channels
  and picking from them in the engine.
- Other platforms get a working backend for free, though AudioToolbox codecs
  ([ADR-0004](0004-mp3-and-aac-go-through-apple-audiotoolbox.md)) still do not port.
