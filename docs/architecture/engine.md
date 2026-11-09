# Audio Engine

## Sample format

Audio inside the engine (buffers, mixing, effects, analysis) must be 32-bit float (`f32`),
nominally in the range -1.0 to 1.0. Conversion to and from other formats happens only at import,
export, and device boundaries ([ADR-0005](../adrs/0005-the-engine-processes-audio-as-32-bit-float.md)).

## Sample rate

Each project has one sample rate, chosen when it is created: 44.1, 48, 88.2 or 96 kHz. All audio
in a project is stored at that rate; imported audio is resampled to it once, on import. When the
output or input device cannot run at the project rate, the engine converts at the device
boundary ([ADR-0006](../adrs/0006-each-project-has-one-fixed-sample-rate.md)).

Sample-rate conversion uses exact rational polyphase filtering: precomputed phases of one
Kaiser-windowed sinc, stepped by a phase accumulator, with no coefficient interpolation and a ratio
fixed when the converter is made. Import uses a preset flat to `0.4535 × f_min` within ±0.01 dB with
at least 120 dB stopband attenuation; real-time device conversion uses a shorter preset with at
least 96 dB. There is no clock-drift correction: input and output must share a clock. Every
converter reports its latency to delay compensation ([ADR-0036](../adrs/0036-sample-rate-conversion-is-exact-rational-polyphase.md)).

## Buffer size

The engine processes audio in blocks of a user-adjustable size from 32 to 1024 frames, defaulting
to 128 frames. Every real-time code path must complete a block of the smallest size within that
block's duration ([ADR-0007](../adrs/0007-the-engine-targets-a-128-frame-default-buffer.md)).

## Audio storage

Project audio is stored on disk and streamed during playback; memory holds only bounded caches and
read-ahead buffers. Disk reads happen off the real-time thread, which receives filled,
preallocated buffers. The engine must support at least 32 tracks and projects several hours long ([ADR-0008](../adrs/0008-project-audio-is-streamed-from-disk.md)).

## Disk streaming

One non-real-time reader thread keeps 2 seconds of each playing track's audio read ahead of the
playhead, serving tracks in earliest-deadline order. Blocks reach the real-time thread through the
lock-free queues of `audio_buffers`.

When the transport starts or jumps, playback begins only once every track has 100 ms buffered at
the new position; until then the real-time thread outputs silence, never waits, and hands back
the blocks read for the old position. Loop starts are read ahead so a loop wraps without a gap.

A buffer that is late plays as silence. The real-time thread counts each underrun in a per-track
atomic counter, and the UI shows a non-modal notice naming the track. Underruns never stop the
transport or recording ([ADR-0034](../adrs/0034-disk-streaming-reads-2-s-ahead-on-one-thread.md)).

## Real-time thread

Code reachable from the audio callback must be tested inside the real-time guard, which fails the
test on any heap allocation or deallocation on the guarded thread. The guard lives in its own
test-support crate, which is the only place its `unsafe` global allocator may exist, and is used
only as a dev-dependency.

Each crate containing real-time code must carry a `clippy.toml` whose `disallowed-types` and
`disallowed-methods` ban locks (`Mutex`, `RwLock`, `Condvar`), file and network I/O, sleeping,
and printing ([ADR-0010](../adrs/0010-real-time-code-is-checked-by-an-allocator-guard.md)).

## Clip playback

Each track channel is one disk stream (see "Disk streaming"). Its source renders the track's clips
from the stored files on the reader thread, not the real-time thread: where clips overlap the top
clip plays, and at each edge of an overlap a 10 ms equal-power crossfade, inside the top clip,
passes between it and the clip beneath, shortened to half the top clip's length if that is less.
Clip gain and the clips' linear fades are applied there too. The real-time thread copies each
track's streams into that track's graph source ([ADR-0046](../adrs/0046-the-engine-renders-clips-on-the-reader-thread-and.md)).

`audio_engine` holds the clip renderer and defines the interfaces it is fed through: a feed per
track channel, a handle that moves every feed, and a reader of stored audio. It does no I/O and
does not see `project_model`. `disk_streaming` implements those interfaces over its streams and the
stored WAV files ([ADR-0047](../adrs/0047-audio-engine-defines-its-feeds-and-disk-streaming.md)).

## Transport

Play starts from the playhead. Pause holds the playhead; stop returns it to where playback last
started. Clip and track edits are accepted only while the transport is stopped or paused; the
engine takes the project's clips when the transport starts or jumps. Record starts like play and
captures the armed tracks' inputs, after an optional count-in during which the transport is
counting in and the playhead holds; the playhead cannot be moved while recording (see
`docs/architecture/recording.md` "Capture" and "Count-in").

A mono listening switch in the engine's output stage, after the plan, sends `(L + R) × 0.7071`
to both output channels. It is a monitoring setting and is not saved in the project ([ADR-0046](../adrs/0046-the-engine-renders-clips-on-the-reader-thread-and.md)).

## Offline render

Offline render runs the same plan and clip renderer as playback, block by block, without the
reader thread or real-time deadlines. It drops the plan's first `latency` frames, so output frame 0
is the first frame of the requested range, and renders exactly that range. For the same project
and range its samples equal playback's ([ADR-0046](../adrs/0046-the-engine-renders-clips-on-the-reader-thread-and.md)).
