# Settings

## Application settings

The application keeps per-machine settings, outside any project, in
`~/Library/Application Support/Debussy/settings.json`: a `format_version` (2) and the chosen audio
device's id. The file is written crash-safely and read as untrusted input; a missing, newer or
invalid file gives the defaults ([ADR-0059](../adrs/0059-the-window-imports-exports-and-picks-one-audio.md)).
Since version 2 it also holds the click's `click_sound` (`block`, `click` or `stick`) and
`click_level_db` (−24 to +6); a version 1 file keeps its device and takes the Click sound at
0 dB, and a level out of range reads as 0 dB ([ADR-0083](../adrs/0083-the-click-is-mixed-at-the-output-stage-from.md)).
Since version 3 it also holds the last export's settings, `export`:
`format` (`wav`, `flac`, `aac` or `mp3`), `wav_depth` and `flac_depth` (`int16`, `int24` or
`float32`), `aac_kbps`, `mp3_kbps` and `mono`. A version 1 or 2 file takes the default export,
24-bit stereo WAV; a FLAC depth of `float32`, or a bit rate of 0 or above 1000, reads as its
default ([ADR-0130](../adrs/0130-export-asks-for-the-format-and-its-settings-before.md)).
Since version 4 (the version written) it also holds `midi_outputs`: each MIDI output endpoint's
`latency_ms` (0 to 1000), keyed by endpoint id, at most 64 of them. A version 1 to 3 file has
none; an entry with an empty or overlong id or a latency out of range is dropped
([ADR-0162](../adrs/0162-midi-tracks-reach-hardware-synths-through-midi.md)).
