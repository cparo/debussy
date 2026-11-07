# ADR-0046 — The engine renders clips on the reader thread and accepts clip edits only while stopped

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-07
- **Issue:** #50
- **Architecture:** [Clip playback](../architecture/engine.md#clip-playback)
  ([Transport](../architecture/engine.md#transport),
  [Offline render](../architecture/engine.md#offline-render))

## Context

Issue #50 asked for a design pass before `audio_engine` is built. It needed to settle how clips
become track signals (ADR-0037 left the overlap crossfade open), how the engine learns of edits,
where the mono mix (ADR-0030) is applied, and how offline render relates to playback. The
questions were put to the developer.

## Decision

- **Scope:** #50 builds the transport, clip playback and offline render. Recording capture and
  input monitoring move to their own ticket, blocked by #50.
- **Clip mixing on the reader thread:** each track channel is one `disk_streaming` stream whose
  source renders the track's clips (overlaps, crossfades, clip gain, fades) from the stored files.
  The real-time thread only copies each stream into the track's graph source.
- **Crossfade:** 10 ms, equal-power, at each edge of an overlap, inside the top clip, shortened
  to half the top clip's length if that is less. Clip fades are linear in gain.
- **Edits:** clip and track edits are accepted only while the transport is stopped or paused, and
  the UI disables clip-editing controls during playback. The engine takes the project's clips
  when the transport starts or jumps. Routing edits during playback are #54.
- **Transport:** pause holds the playhead; stop returns it to where playback last started.
- **Mono mix:** a mono listening switch in the engine's output stage, after the plan; a monitoring
  setting, not saved. Mono export folds in `import_export`.
- **Offline render:** the same plan and clip renderer, called directly instead of through the
  reader thread. The first `latency` frames are dropped so output frame 0 is the range's first
  frame, and exactly the requested range is rendered. Effect tails wait for #61.

## Alternatives considered

- **Mixing clips on the real-time thread, one stream per clip:** edits would not wait on
  read-ahead, but `disk_streaming`'s streams are fixed when it starts, the stream count would grow
  with clip count, and render would need a second mixing path.
- **Hearing edits during playback:** re-seeking the edited track (a ~100 ms gap) or splicing new
  audio in after the 2 s already buffered. Both were rejected in favour of editing while stopped.
- **A three-way split of #50, or one ticket:** render is small once playback exists, and the
  acceptance test compares them; recording needs device inputs and is separable.
- **5 ms or linear crossfades, or a per-project length:** 5 ms risks clicks on low frequencies; a
  linear crossfade dips about 3 dB between uncorrelated takes; a setting adds a saved field.
- **Mono mix as a graph node:** it is a listening check, not part of the mix.
- **A fixed render tail, or unaligned output:** no built-in effect rings yet; trimming latency is
  the engine's job.

## Consequences

- Playback and render share one clip renderer, so they produce identical samples by construction.
- Clip edits cannot be auditioned during playback; editing workflows stop the transport.
- A routing change made during playback still needs #54.
