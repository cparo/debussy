# ADR-0004 — MP3 and AAC go through Apple AudioToolbox

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-06
- **Issue:** —
- **Architecture:** [Patent-encumbered codecs](../architecture/platform.md#patent-encumbered-codecs)

## Context

The first release must import and export MP3 and AAC. The architecture has no patent policy yet,
so CLAUDE.md treats every codec with a patent history as exposed. MP3's patents have expired; AAC
remains under licensing pools.

## Decision

MP3 and AAC encoding and decoding use Apple's AudioToolbox framework, called only from the macOS
platform crate. No crate implementing either codec is added.

## Alternatives considered

- **A Rust codec crate** — would put the project in the position of distributing a patent-exposed
  implementation.
- **Defer AAC and ship MP3 only** — leaves a first-release requirement unmet.

## Consequences

- Apple licenses the codecs it ships, so Debussy carries no codec patent exposure on macOS.
- A future platform must supply its own MP3/AAC implementation, or ship without those formats.
- The codec interface is a seam between platform-independent code and the macOS platform crate.
- WAV and FLAC are not covered by this decision; the patent policy section still needs writing.
