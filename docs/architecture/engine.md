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
the new position; until then the real-time thread outputs silence and never waits. Loop starts are
read ahead so a loop wraps without a gap.

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
