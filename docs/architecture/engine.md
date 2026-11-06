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

## Buffer size

The engine processes audio in blocks of a user-adjustable size from 32 to 1024 frames, defaulting
to 128 frames. Every real-time code path must complete a block of the smallest size within that
block's duration ([ADR-0007](../adrs/0007-the-engine-targets-a-128-frame-default-buffer.md)).

## Audio storage

Project audio is stored on disk and streamed during playback; memory holds only bounded caches and
read-ahead buffers. Disk reads happen off the real-time thread, which receives filled,
preallocated buffers. The engine must support at least 32 tracks and projects several hours long ([ADR-0008](../adrs/0008-project-audio-is-streamed-from-disk.md)).
