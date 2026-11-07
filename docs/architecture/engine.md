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

Sample-rate conversion uses a polyphase Kaiser-windowed sinc that accepts any ratio. Import uses a
preset flat to 20 kHz within ±0.01 dB with at least 120 dB stopband attenuation; real-time device
conversion uses a shorter preset with at least 96 dB. At the device boundary the ratio adapts to
the FIFO fill level to absorb clock drift, and every converter reports its latency to delay
compensation ([ADR-0036](../adrs/0036-sample-rate-conversion-uses-windowed-sinc-with-two.md)).

## Buffer size

The engine processes audio in blocks of a user-adjustable size from 32 to 1024 frames, defaulting
to 128 frames. Every real-time code path must complete a block of the smallest size within that
block's duration ([ADR-0007](../adrs/0007-the-engine-targets-a-128-frame-default-buffer.md)).

## Audio storage

Project audio is stored on disk and streamed during playback; memory holds only bounded caches and
read-ahead buffers. Disk reads happen off the real-time thread, which receives filled,
preallocated buffers. The engine must support at least 32 tracks and projects several hours long ([ADR-0008](../adrs/0008-project-audio-is-streamed-from-disk.md)).

## Real-time thread

Code reachable from the audio callback must be tested inside the real-time guard, which fails the
test on any heap allocation or deallocation on the guarded thread. The guard lives in its own
test-support crate, which is the only place its `unsafe` global allocator may exist, and is used
only as a dev-dependency.

Each crate containing real-time code must carry a `clippy.toml` whose `disallowed-types` and
`disallowed-methods` ban locks (`Mutex`, `RwLock`, `Condvar`), file and network I/O, sleeping,
and printing ([ADR-0010](../adrs/0010-real-time-code-is-checked-by-an-allocator-guard.md)).
