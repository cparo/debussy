# Audio Analysis

## Waveform summaries

Each stored mono audio file has a waveform summary: a pyramid of levels of buckets, each bucket
holding the minimum, maximum and RMS of its samples. Level 0 has 64 samples per bucket and each
level above has four times as many, up to the first level with a single bucket; the last bucket
of a level may be partial. Views zoomed in past 64 samples per pixel read the audio itself
([ADR-0062](../adrs/0062-zooms-finer-than-the-waveform-summaries-are-drawn.md)) and draw it as a
time-in-level trace: samples are joined by straight lines; a column's segments, from its first
sample to the next column's first, share its time equally, each spread over the pixel rows it
crosses in proportion to its overlap; and each pixel's share, divided by the column's largest
and square-rooted, is the opacity of the audio colour ([ADR-0065](../adrs/0065-zooms-finer-than-the-waveform-summaries-are-drawn.md)).

A summary is built in one streaming pass in memory independent of the file's length, and read
back a range at a time ([ADR-0045](../adrs/0045-waveform-summaries-and-spectrogram-overviews-are.md)).

## Spectrograms

Spectrogram frames are 2048-point Hann-windowed magnitude spectra (1025 bins) taken every 512
samples from the start of the file, zero-padded past its end, and given in dB relative to a
full-scale sine at a bin centre, floored at −120 dB. Frames for a view are computed on demand.

Each stored audio file also has a spectrogram overview: one column per 16384 samples, holding for
each bin the mean power of the frames that start in that column, in dB, as one byte per bin in
0.5 dB steps from 0 (0 dB or more) to 255 (−127.5 dB or less) ([ADR-0045](../adrs/0045-waveform-summaries-and-spectrogram-overviews-are.md)).

## Analysis cache files

Waveform summaries and spectrogram overviews are stored in the bundle's `cache/` directory as
`<audio file name>.waveform` and `<audio file name>.spectrogram`. Both are little-endian binary
files starting with a four-byte magic (`DBWF` or `DBSG`), a `u32` format version, their
parameters and the `u64` sample count, followed by the data: waveform levels finest first, each
bucket as three `f32` (minimum, maximum, RMS); spectrogram columns in order, each as 1025 bytes.
A cache file whose magic, version, parameters, sample count or length does not match is discarded
and rebuilt; it is never migrated. The `audio_analysis` crate owns both formats ([ADR-0045](../adrs/0045-waveform-summaries-and-spectrogram-overviews-are.md)).
