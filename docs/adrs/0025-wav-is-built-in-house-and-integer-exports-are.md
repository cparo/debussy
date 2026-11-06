# ADR-0025 — WAV is built in-house and integer exports are dithered

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-06
- **Issue:** —
- **Architecture:** [WAV files](../architecture/import-export.md#wav-files)

## Context

Project audio is stored as 32-bit float WAV ([ADR-0014](0014-project-audio-is-stored-as-mono-32-bit-float-wav.md)), so WAV and RF64 handling is needed
regardless of import and export. Exporting to 16- or 24-bit integer needs a rounding policy.

## Decision

WAV and RF64 reading and writing are built in-house and shared by storage, import and export.
Export offers 16-bit, 24-bit and 32-bit float, and reductions to integer depths are TPDF-dithered.

## Alternatives considered

- **WAV through AudioToolbox** — not portable, and duplicates the in-house code that storage needs
  anyway.
- **A crate such as `hound`** — WAV is simple and on the core data path, where the dependency
  policy defaults to **build**.
- **No dither** — plain rounding causes audible quantisation distortion on quiet 16-bit fades.

## Consequences

- WAV/RF64 parsing handles files from other tools, which is untrusted input; it needs bounds
  checks and fuzz-style tests on malformed headers.
- The dither generator is a small piece of audio-core code built in-house, with no patent history
  (TPDF dither is a textbook technique).
- FLAC export at 16 or 24 bits uses the same dither step before AudioToolbox encodes it.
