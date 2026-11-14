# Platform Integration

## Compressed audio codecs

MP3 decoding, and AAC and FLAC encoding and decoding, must go through Apple's AudioToolbox
framework, from inside the macOS platform crate. No crate implementing these codecs may be added.
MP3 is encoded by a host-installed `lame`, not AudioToolbox ([MP3 export](import-export.md#mp3-export))
([ADR-0024](../adrs/0024-compressed-codecs-go-through-audiotoolbox-and-mp3.md), [ADR-0129](../adrs/0129-mp3-export-runs-a-host-installed-lame-as-a.md)).

The codecs call AudioToolbox's `ExtAudioFile` through the `objc2-audio-toolbox` bindings, decoding
in the application process and never on the real-time thread. A decoded file is untrusted: one
whose format is not MP3, AAC or FLAC, or that reports more than 2 channels, a sample rate outside
8–384 kHz or more than 24 hours, is refused, and a decode that runs past 24 hours stops with an
error. Every AudioToolbox status is checked ([ADR-0051](../adrs/0051-audiotoolbox-codecs-use-objc2-bindings-unsafe.md)).

## Folders

The paths of the app's folders and files on macOS (the projects folder, the settings file, and the
folders searched for `lame`) are worked out in `macos_platform::folders`; no other crate names a
macOS folder. ([ADR-0098](../adrs/0098-projects-live-in-music-debussy-and-open-lists-them.md))

## Audio device I/O

Audio devices are opened, enumerated and streamed through `cpal`, used only inside the platform
crate and only behind Debussy's own audio I/O interface. No other crate may depend on `cpal` ([ADR-0009](../adrs/0009-audio-device-i-o-uses-cpal-behind-our-own.md)).

A duplex stream is a `cpal` input stream and output stream on the same device. The input callback
pushes samples into a lock-free ring of 4 buffers; the output callback takes one buffer's worth
(silence for any shortfall) and runs the audio callback with both. The input latency reported
includes the frames waiting in the ring ([ADR-0052](../adrs/0052-a-duplex-stream-is-two-cpal-streams-joined-by-a.md)).
