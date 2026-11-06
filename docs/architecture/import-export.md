# Import and Export

## Formats

| Format | Import | Export | Implemented by |
|---|---|---|---|
| WAV (and RF64) | yes | yes | in-house |
| FLAC | yes | yes | AudioToolbox |
| AAC | yes | yes | AudioToolbox |
| MP3 | yes | no ([ADR-0024](../adrs/0024-compressed-codecs-go-through-audiotoolbox-and-mp3.md)) | AudioToolbox |

## WAV files

WAV and RF64 reading and writing are built in-house, and shared by import, export and project audio
storage ([ADR-0025](../adrs/0025-wav-is-built-in-house-and-integer-exports-are.md)).

The WAV reader treats every file as untrusted. It accepts only mono or stereo PCM 16/24/32-bit
integer or 32/64-bit float, at 1 Hz to 768 kHz. It reads `fmt ` and `ds64` chunks of at most
1 KiB, skips other chunks by seeking, and scans at most 1024 chunks. It checks every size against
the file length with checked arithmetic, and never allocates with a size taken from a header. A
non-`data` chunk past the end of the file is an error; an overlong `data` chunk is clamped to the
whole frames present and reported as truncated ([ADR-0029](../adrs/0029-the-wav-reader-accepts-only-mono-and-stereo-files.md)).

## Export bit depth

Export offers 16-bit integer, 24-bit integer and 32-bit float samples where the format allows.
Reducing to an integer depth applies TPDF dither ([ADR-0025](../adrs/0025-wav-is-built-in-house-and-integer-exports-are.md)).
