# Platform Integration

## Compressed audio codecs

MP3 decoding, and AAC and FLAC encoding and decoding, must go through Apple's AudioToolbox
framework, from inside the macOS platform crate. No crate implementing these codecs may be added.
MP3 export is not offered ([ADR-0024](../adrs/0024-compressed-codecs-go-through-audiotoolbox-and-mp3.md)).

## Audio device I/O

Audio devices are opened, enumerated and streamed through `cpal`, used only inside the platform
crate and only behind Debussy's own audio I/O interface. No other crate may depend on `cpal` ([ADR-0009](../adrs/0009-audio-device-i-o-uses-cpal-behind-our-own.md)).
