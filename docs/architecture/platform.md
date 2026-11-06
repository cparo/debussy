# Platform Integration

## Patent-encumbered codecs

MP3 and AAC must be encoded and decoded through Apple's AudioToolbox framework, from inside the
macOS platform crate. No crate that implements either codec may be added ([ADR-0004](../adrs/0004-mp3-and-aac-go-through-apple-audiotoolbox.md)).
