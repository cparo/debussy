# ADR-0053 — Imports are named after their file and exports use a reproducible dither

- **Status:** Accepted
- **Decided by:** agent, unratified
- **Date:** 2026-11-07
- **Issue:** #53
- **Architecture:** [Import](../architecture/import-export.md#import)
  ([Export](../architecture/import-export.md#export))

## Context

#53 builds import and export from existing rules: mono stored files (ADR-0014), in-house WAV with
dithered integer exports (ADR-0025), compressed formats through the platform (ADR-0024), the
import resampler (ADR-0036) and offline render (ADR-0046). A few choices were left open: how stored
files are named, how a resampled import is aligned, how export reaches the render, and how dither
is seeded.

## Decision

- **Detection:** a file is read in-house if it starts like WAV or RF64 (`RIFF`/`RF64` … `WAVE`),
  otherwise through the codecs, which identify it themselves.
- **Stored names:** the imported file's stem, with characters other than letters, digits, spaces,
  `-` and `_` replaced by `_`, cut to 64 characters (`audio` if nothing useful is left), then the
  smallest number free for every channel, then ` L`/` R` for stereo. People can tell the files
  apart, and the names are plain names `project_storage` accepts.
- **Length and alignment:** a stored file holds `ceil(frames × project rate ÷ file rate)` frames.
  The resampler's delay is removed rounded to whole frames, so a resampled import is aligned to
  under half a sample.
- **Failure:** a failed import removes every file it made.
- **Export input:** `export` takes the arguments of `audio_engine::render` (`OfflineRender`). The
  render's output callback reports a write failure through a new `audio_engine::Error::Output`.
- **Dither:** fixed seeds per channel, so the same project exports to the same bytes. FLAC is
  dithered to its depth before encoding, so the encoder receives samples already on its grid.
- **Export rate:** the project rate; converting on export is left for when it is needed.

## Alternatives considered

- **Random or timestamped stored names:** unique without probing the directory, but
  meaningless to someone looking inside the bundle.
- **Fractional-delay alignment:** exact, but needs a resampler start phase that `dsp` does not
  offer; half a sample is about 10 µs at 48 kHz.
- **Leaving FLAC's integer conversion to the encoder:** it would round without dither, against
  ADR-0025.
- **Random dither seeds:** each export would differ in its least significant bits.

## Consequences

- Importing the same file twice makes `… 1` and `… 2`; cleanup (ADR-0049) removes whichever
  ends up unused.
- Building the render graph and feeds from a project is #54's bridge; export takes them as given.
