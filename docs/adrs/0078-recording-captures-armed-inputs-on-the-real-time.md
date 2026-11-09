# ADR-0078 — Recording captures armed inputs on the real-time thread and writes takes on a separate thread

- **Status:** Accepted
- **Decided by:** agent, ratified by Chris Paro
- **Date:** 2026-11-09
- **Issue:** #67
- **Architecture:** [Capture](../architecture/recording.md#capture)

## Context

Recording (#67, split from #50 by ADR-0046) needs armed tracks' inputs captured into stored files
(ADR-0014), placed shifted by the device latencies (ADR-0022), and monitored through the graph
(ADR-0021, ADR-0032), with no disk writes on the real-time thread (ADR-0010). The device layer
already delivers duplex interleaved input and reports its latencies including the duplex ring
(ADR-0052); `wav_files` streams samples to disk; `audio_buffers` has block pools and lock-free
queues. The engine took no input and had no arm state.

## Decision

- **Input.** `Processor::process` takes the device's interleaved input and channel count. It is
  deinterleaved into buffers allocated when the engine is built, for up to `MAX_INPUTS` channels
  of the graph's largest block.
- **Arming.** `Engine::arm` sends the armed tracks: each one's graph source, the device input
  channel per track channel, the monitor toggle, and one `TakeSink` per channel. It is refused
  while playing, like `set_tracks`, and the old set returns over the retire queue. `TakeSink` is
  defined by `audio_engine` and implemented by `disk_streaming`, as `TrackFeed` is (ADR-0047).
- **Monitoring.** A monitored armed track's input replaces its clip playback whenever it is
  armed, not only while recording, so the performer can set levels. This extends ADR-0021's
  "while recording". An armed, unmonitored track is silent while recording: the new take will
  cover its old clips.
- **Capture.** `Engine::record` starts like play. Capture begins on the block where the transport
  changes from starting to playing, after the 100 ms prime, so the take's first frame is the
  timeline frame then; the engine publishes that frame and the plan latency. Stop or pause ends
  the take through `TakeSink::end`, which is real-time safe. Seeking while recording is refused.
- **Writer.** `disk_streaming`'s `Recorder` runs one `disk-recording` thread. Each take channel
  has a pool of 2 s of 1024-frame blocks, the same bound as the read-ahead (ADR-0034). The
  real-time end stamps each full block with its take-relative start frame and queues it; the
  writer appends it to a WAV file and returns it. If the pool is empty, the frames are dropped,
  an overrun is counted, and the writer pads the gap with silence from the stamps. When a take
  ends the writer drains, finishes the file and reports `Finished { frames }` or `Failed`.
- **Placement.** A take starts at `start − (input latency + output latency + plan latency)`. A
  negative start is clamped to 0 and the deficit becomes the clip's offset. When a pass's files
  have all finished, the layer above adds one clip per armed track as one batched edit, on top.
  An empty take is discarded.
- **Session-only state.** Arm, monitor and input channels are not saved; the saved format does not
  change. Existing tracks play while recording. Recording runs plainly from the playhead.

## Alternatives considered

- **Write files on the real-time thread, or through a growable buffer** — file I/O and
  allocation are banned there (ADR-0010).
- **Stop the transport on an overrun** — a lost take is worse than a short gap; underruns already
  follow this rule (ADR-0034).
- **Start capture at the Record press, before priming** — the take would hold up to 100 ms that
  lines up with nothing played; starting with playback gives one exact timeline frame.
- **Monitor only while recording, as ADR-0021 first said** — the performer could not set levels
  before recording.
- **Keep unmonitored armed tracks playing their clips while recording** — the performer would
  hear the part being replaced.
- **Save arm and monitor state in the project** — a persisted format change for state that
  belongs to the session and the machine's inputs.
- **One interleaved file per take** — ADR-0014 stores one mono file per channel.

## Consequences

- Monitored input is undelayed only while no node reports latency, since today plan latency is
  always 0. Keeping ADR-0032 once latency-adding effects exist (#58 and later) needs a
  `signal_graph` change (#109).
- After a crash mid-take the unreferenced `.wav` is deleted by bundle cleanup on the next open
  (ADR-0049). Take recovery is #110.
- Punch-in/out and loop recording are #111. The click track and the rest of #57
  remain.
- Monitor can only be toggled while stopped or paused, since arming is refused while playing.
- Inputs beyond `MAX_INPUTS` (64) cannot be recorded.
