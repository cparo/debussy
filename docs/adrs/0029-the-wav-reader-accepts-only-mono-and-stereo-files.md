# ADR-0029 — The WAV reader accepts only mono and stereo files and bounds every size it reads

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-06
- **Issue:** #14
- **Architecture:** [WAV files](../architecture/import-export.md#wav-files)

## Context

`wav_files` reads WAV and RF64 files that other tools produced, so it is the first place the
project parses untrusted input. A header can claim any channel count, sample rate or chunk size,
and a parser that trusts those fields can be driven to panic, loop for a long time, or make a huge
allocation. Real files are also often imperfect: streaming recorders and crashed sessions leave a
`data` size of 0, `0xFFFFFFFF`, or one longer than the file. The limits were put to the developer
as a `security` risk.

## Decision

- **Formats:** only PCM 16-, 24- and 32-bit integer and 32- and 64-bit float, as
  `WAVE_FORMAT_PCM`, `WAVE_FORMAT_IEEE_FLOAT` or `WAVE_FORMAT_EXTENSIBLE`, in RIFF or RF64.
  `block_align` must equal channels × bytes per sample.
- **Channels:** 1 or 2 ([ADR-0017](0017-signals-are-mono-or-stereo-only.md)). Other counts are
  rejected with a distinct error.
- **Sample rate:** 1 Hz to 768 kHz.
- **Chunks:** `fmt ` and `ds64` are read only if they are at most 1 KiB. Other chunks are skipped
  by seeking, never read. At most 1024 chunks are scanned before `data`.
- **Sizes:** every size is checked against the file length in `u64` with checked arithmetic. A
  chunk other than `data` that runs past the end of the file is an error. A `data` chunk that does
  is clamped to the whole frames present, and the reader reports that it was truncated.
- **Allocation:** nothing is allocated with a size taken from a header field. Reads fill
  caller-supplied buffers through a fixed scratch buffer of at most 64 KiB.
- **Tests:** malformed-input cases, plus a deterministic mutation test (truncations and byte
  flips of the fixtures) that checks every result is a value or an error, never a panic.

## Alternatives considered

- **Up to 32 or 256 channels**, split into mono files on import — supports surround and array
  recordings, but widens the input surface beyond what the engine can use: signals are mono or
  stereo only.
- **Reject any `data` size past the end of the file** — simplest, but loses crashed or streamed
  recordings from other tools. Clamping is just as bounded, because it is limited by the real
  file length.
- **Fuzzing with `cargo-fuzz`** — needs a nightly toolchain and a new dependency; the mutation
  test covers the same failure modes more shallowly.

## Consequences

- Users must split multichannel files with another tool before importing them; import should say
  so in its error.
- Import must surface the truncation report so the user is warned about a damaged file.
- Raising the channel limit later needs a new record and a matching change to signals
  (ADR-0017).
