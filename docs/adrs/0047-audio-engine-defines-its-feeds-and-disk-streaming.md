# ADR-0047 — audio_engine defines its feeds, and disk_streaming implements them

- **Status:** Accepted
- **Decided by:** agent, ratified by Chris Paro
- **Date:** 2026-11-07
- **Issue:** #50
- **Architecture:** [Clip playback](../architecture/engine.md#clip-playback)

## Context

ADR-0046 has the reader thread render each track channel's clips into a disk stream, and the
real-time thread read those streams. But `audio_engine` is a real-time core crate and may depend
only on other real-time core crates (`docs/architecture/crates.md` "Dependency rules"), so it can
use neither `disk_streaming` (I/O) nor `project_model` (core). Offline render must also run the
same clip renderer without a reader thread.

## Decision

- `audio_engine` holds the clip renderer (`TrackRenderer`) over its own per-channel clip type
  (`ChannelClip`: file index, offset, length, position, linear gain, fades), and defines the
  traits it is fed through: `TrackFeed` (a channel's audio, read on the real-time thread),
  `Streams` (moves every feed when the transport starts or jumps) and `ClipAudio` (reads stored
  audio for the renderer).
- `disk_streaming` depends on `audio_engine` and implements them: `TrackFeed` for `Playback`,
  `Streams` for `Streamer`, `TimelineSource` (a renderer as a stream source) and `WavClipAudio`
  (stored WAV files, opened when first read).
- Offline render feeds the engine `RenderFeed`s, which call the renderer directly, through the
  same per-block step as playback.
- Turning `project_model` clips into `ChannelClip`s, and the routing into a graph, is left to the
  layer that bridges the model and the engine (#54).

## Alternatives considered

- **The renderer in `disk_streaming`:** offline render, in `audio_engine`, could not reach it,
  and playback and render would no longer share one code path.
- **A new crate for the clip renderer:** one more crate for one module, with the same
  dependencies as `audio_engine`.
- **Moving `audio_engine` to the core tier:** it runs on the real-time thread and must stay
  free of third-party and I/O dependencies.

## Consequences

- The engine is tested without disk or threads; `disk_streaming`'s tests cover the real streams.
- A second kind of feed (recording input, #67) plugs into the same interface.
