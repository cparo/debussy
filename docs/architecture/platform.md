# Platform Integration

## Patent-encumbered codecs

MP3 and AAC must be encoded and decoded through Apple's AudioToolbox framework, from inside the
macOS platform crate. No crate that implements either codec may be added ([ADR-0004](../adrs/0004-mp3-and-aac-go-through-apple-audiotoolbox.md)).

## Audio device I/O

Audio devices are opened, enumerated and streamed through `cpal`, used only inside the platform
crate and only behind Debussy's own audio I/O interface. No other crate may depend on `cpal` ([ADR-0009](../adrs/0009-audio-device-i-o-uses-cpal-behind-our-own.md)).
