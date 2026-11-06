# ADR-0007 — The engine targets a 128-frame default buffer

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-06
- **Issue:** —
- **Architecture:** [Buffer size](../architecture/engine.md#buffer-size)

## Context

Musicians monitoring through the software while tracking hear the round-trip latency, so the
buffer size bounds how tight monitoring can feel and how much CPU each block has.

## Decision

Default block size is 128 frames (about 2.7 ms at 48 kHz), user-adjustable from 32 to 1024 frames.

## Alternatives considered

- **64-frame default** — tighter, but halves the time per block for effects on every machine,
  including base-model laptops.
- **256 frames or more by default** — relaxed, but the delay becomes audible to people monitoring
  through the software.

## Consequences

- Real-time code paths are designed and tested against 32-frame blocks, the worst case.
- Users who need more headroom for heavy effect chains can raise the buffer size.
