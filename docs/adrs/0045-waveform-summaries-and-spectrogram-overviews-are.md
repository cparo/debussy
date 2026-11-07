# ADR-0045 — Waveform summaries and spectrogram overviews are cached per audio file in disposable binary formats

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-07
- **Issue:** #49
- **Architecture:** [Analysis cache files](../architecture/analysis.md#analysis-cache-files)
  ([Waveform summaries](../architecture/analysis.md#waveform-summaries),
  [Spectrograms](../architecture/analysis.md#spectrograms))

## Context

The brief asks to "see recorded audio channels as waveforms or spectrograms", and ADR-0008 expects
precomputed summaries stored alongside the streamed audio, since drawing a multi-hour track from
the audio itself would read gigabytes. Issue #49 left open what a waveform bucket holds, the
pyramid's resolution, whether summaries are cached, and how spectrogram data is produced. A cache
file is a persisted format, so the choices were put to the developer.

## Decision

- **Waveform buckets** hold minimum, maximum and RMS as `f32`, so a view can draw the RMS body
  inside the peak outline.
- **The pyramid** starts at 64 samples per bucket and grows by four per level until one bucket
  remains. For three hours of mono 48 kHz audio that is about 130 MB of summary against 2 GB of
  audio.
- **Waveform summaries are cached** in `cache/`, in a format `audio_analysis` defines over
  `Read`, `Write` and `Seek`. The level offsets follow from the sample count in the header, so
  a builder writes each level in place through small buffers and a reader fetches only the
  buckets a view needs; neither holds the whole summary in memory.
- **Spectrogram detail is computed on demand** from 2048-point Hann frames every 512 samples, in
  dB floored at −120 dB, built on `dsp::Stft` (ADR-0039).
- **A coarse spectrogram overview is also cached**: one column per 16384 samples, the mean power
  of the frames in it, quantised to one byte per bin in 0.5 dB steps (about 32 MB for three hours
  of mono 48 kHz audio).
- **Cache files are disposable**: anything that does not match is rebuilt, never migrated, so
  they carry a version but no migrations. Stored audio is immutable (ADR-0011), so a cache file
  is keyed by its audio file's name and checked against its sample count.

## Alternatives considered

- **Minimum and maximum only** — about a third smaller, but no RMS body, and adding RMS later
  would change the format.
- **A 256-sample base, or a factor of two** — 256 is four times smaller but sends more zoom levels
  to the audio; a factor of two doubles the pyramid's overhead for finer level choice.
- **No cache, or the format in a later issue** — rebuilding on every open reads all project audio;
  deferring the format would leave ADR-0008's summaries without a home.
- **Spectrogram detail only, on demand** — zoomed-out views would need one FFT per pixel column
  over audio scattered across the file; a cached full-resolution spectrogram would be gigabytes.
- **Overview as `f32`, or with 65536-sample columns** — about 130 MB per three hours, or blurrier
  columns when partly zoomed in.

## Consequences

- A summary costs about 6% of its audio's size on disk; the overview about 1.6%.
- Writing the cache files into the bundle, and building them after import and recording, belongs
  to `project_storage` and `import_export` and is not yet done.
- Changing a cache format means bumping its version; old files are rebuilt.
