# ADR-0024 — Compressed codecs go through AudioToolbox and MP3 export is deferred

- **Status:** Superseded by ADR-0129
- **Decided by:** Chris Paro
- **Date:** 2026-11-06
- **Issue:** #8
- **Architecture:** [Compressed audio codecs](../architecture/platform.md#compressed-audio-codecs)

## Context

[ADR-0004](0004-mp3-and-aac-go-through-apple-audiotoolbox.md) routed MP3 and AAC encoding and decoding through AudioToolbox. Testing on macOS
27.0.1 showed that AudioToolbox decodes MP3 but cannot encode it (`ExtAudioFileSetProperty
('cfmt')` fails with `fmt?`). It encodes and decodes AAC and FLAC.

## Decision

MP3 decoding, and AAC and FLAC encoding and decoding, go through AudioToolbox in the platform
crate. MP3 export is deferred (#8). This supersedes ADR-0004.

## Alternatives considered

- **LAME for MP3 export** — reference quality, but LGPL, which is outside the allowlist
  ([ADR-0001](0001-debussy-is-licensed-mit-or-apache-2-0-and-accepts.md)).
- **Build an MP3 encoder** — allowed by the patent policy ([ADR-0023](0023-expired-or-royalty-free-patents-may-be-implemented.md)) since MP3 patents
  expired in 2017, but a large DSP project whose quality would trail LAME for a long time.
- **FLAC in-house or through a crate** — portable, but a sizeable project or another dependency
  for a format the OS already handles. FLAC is royalty-free, so either remains open for other
  platforms.

## Consequences

- The first release exports WAV, FLAC and AAC, but not the MP3 export the brief lists; AAC covers
  lossy sharing. MP3 import still works.
- One codec interface in the platform crate covers every compressed format.
- Another platform needs its own AAC route; FLAC and MP3 decoding could use crates there.
