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

## Export bit depth

Export offers 16-bit integer, 24-bit integer and 32-bit float samples where the format allows.
Reducing to an integer depth applies TPDF dither ([ADR-0025](../adrs/0025-wav-is-built-in-house-and-integer-exports-are.md)).
