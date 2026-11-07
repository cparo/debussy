# ADR-0051 — AudioToolbox codecs use objc2 bindings, unsafe confined to sys, and checked in-process decoding

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-07
- **Issue:** #52
- **Architecture:** [Compressed audio codecs](../architecture/platform.md#compressed-audio-codecs)
  ([Dependency rules](../architecture/crates.md#dependency-rules))

## Context

ADR-0024 sends MP3 decoding and AAC and FLAC coding through AudioToolbox. #52 needed a way to
bind it, approval for the `unsafe` code FFI requires (CLAUDE.md "Memory Safety and Unsafe"), and a
decision on how an untrusted imported file reaches Apple's parsers (`flag-risk`, tag `security`).
These were put to the developer.

## Decision

- **Bindings:** `objc2-audio-toolbox`, `objc2-core-audio-types` and `objc2-core-foundation`,
  already in the graph through `cpal`, with the `ExtendedAudioFile`, `AudioFile`,
  `AudioConverter` and `AudioFormat` features. No new crates.
- **`unsafe`:** confined to `macos_platform`'s `sys` module, every block with a `// SAFETY:`
  comment. `sys` exposes safe types (an `ExtAudioFile` disposed on drop); the crate root sets
  `#![deny(unsafe_code)]` and only `sys` allows it.
- **Untrusted input:** decode in-process through AudioToolbox, the system's maintained parsers,
  never on the real-time thread. Refuse formats other than MP3, AAC and FLAC, more than 2
  channels, sample rates outside 8–384 kHz, and more than 24 hours, whether reported up front or
  reached while reading. Check every status; never panic on bad input.

## Alternatives considered

- **Hand-written `extern "C"` declarations:** no direct dependency, but every signature and
  struct layout would be ours to get right.
- **All `unsafe` in one file, or no AudioToolbox codecs yet:** one file was the reviewer's
  alternative; deferring the codecs would block import and export.
- **Decoding in a sandboxed helper process (an XPC service):** contains a parser exploit, at the
  cost of a second binary, IPC and signing work.

## Consequences

- A flaw in AudioToolbox's parsers runs in Debussy's process; macOS security updates fix it.
- WAV stays in-house (ADR-0025); AudioToolbox is asked only for the three formats.
- AAC encoding is limited to the rates Apple's encoder accepts; others report an error.
