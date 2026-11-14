# Import and Export

## Formats

| Format | Import | Export | Implemented by |
|---|---|---|---|
| WAV (and RF64) | yes | yes | in-house |
| FLAC | yes | yes | AudioToolbox |
| AAC | yes | yes | AudioToolbox |
| MP3 | yes | yes, if `lame` is installed | AudioToolbox (import); an external `lame` (export) |

## WAV files

WAV and RF64 reading and writing are built in-house, and shared by import, export and project audio
storage ([ADR-0025](../adrs/0025-wav-is-built-in-house-and-integer-exports-are.md)).

The WAV reader treats every file as untrusted. It accepts only mono or stereo PCM 16/24/32-bit
integer or 32/64-bit float, at 1 Hz to 768 kHz. It reads `fmt ` and `ds64` chunks of at most
1 KiB, skips other chunks by seeking, and scans at most 1024 chunks. It checks every size against
the file length with checked arithmetic, and never allocates with a size taken from a header. A
non-`data` chunk past the end of the file is an error; an overlong `data` chunk is clamped to the
whole frames present and reported as truncated ([ADR-0029](../adrs/0029-the-wav-reader-accepts-only-mono-and-stereo-files.md)).

## Import

A file is read in-house if it starts like WAV or RF64, and through the platform codecs otherwise.
Only mono and stereo files are imported. Each channel is converted to the project rate with the
import-quality resampler, its delay removed to the nearest sample, and stored as a mono 32-bit float
WAV of `ceil(frames × project rate ÷ file rate)` frames. Stored files are named after the imported
file: its stem, with characters other than letters, digits, spaces, `-` and `_` replaced by `_`,
cut to 64 characters, then the smallest free number, then ` L` or ` R` for stereo
(`Lead Vox 1 L.wav`). A failed import removes every file it made ([ADR-0053](../adrs/0053-imports-are-named-after-their-file-and-exports-use.md)).

## Export

Export renders the requested range offline at the project rate and writes the stereo mix, or the
mono mix `(L + R) × 0.7071`. Integer depths, for WAV and for FLAC before it is encoded, are
TPDF-dithered with a fixed seed per channel, so an export is reproducible. WAV is written in-house;
FLAC and AAC go through the platform codecs, and MP3 through [`lame`](#mp3-export) ([ADR-0053](../adrs/0053-imports-are-named-after-their-file-and-exports-use.md)).

## Export bit depth

Export offers 16-bit integer, 24-bit integer and 32-bit float samples where the format allows.
Reducing to an integer depth applies TPDF dither ([ADR-0025](../adrs/0025-wav-is-built-in-house-and-integer-exports-are.md)).

## MP3 export

MP3 is encoded by a `lame` executable that the user installed, run as a separate process; LAME is
never linked or bundled. At startup `debussy` looks for `lame` once, in `/opt/homebrew/bin`,
`/usr/local/bin` and `/opt/local/bin`, then in each absolute folder on `PATH`, the list coming from
`macos_platform::folders`. The first candidate that is an executable file and whose
`lame --version` exits 0 within 2 s and prints `LAME` is used. If none is, MP3 is left out of the
export dialog, and an `.mp3` export fails with "MP3 export needs LAME: `brew install lame`".

`import_export::Lame` runs lame through `std::process::Command` with an argument list, never a
shell, and gives it no path: the mix goes to its standard input as raw signed little-endian 16-bit
PCM, already TPDF-dithered to 16 bits, and its standard output is copied into a file Debussy
created. Its standard output and standard error are drained on their own threads; the last 4 KiB
of standard error is kept for the error message. A non-zero exit, a broken pipe, or an abandoned
export stops lame and removes the partial file.

The bit rate is constant: 128, 256 or 320 kbit/s, 320 by default. Projects above 48 kHz are
resampled by lame, to 44.1 kHz from multiples of 44.1 kHz and to 48 kHz otherwise
([ADR-0129](../adrs/0129-mp3-export-runs-a-host-installed-lame-as-a.md)).
