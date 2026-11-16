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

## MIDI clip playback

Each MIDI track's clips are flattened onto the timeline off the real-time thread: where clips
overlap the top clip plays, and a lower clip plays only where no clip above covers it; a note plays
if it starts inside a stretch of its clip that shows and is cut off at that stretch's end; a
control change plays if it falls inside one. The engine holds one player per MIDI track, replaced
only while the transport is stopped or paused, which writes the track's events into its graph event
source each block, stamped with their frames in the block; at one frame, note-offs come before
control changes and control changes before note-ons.

Whenever a block does not follow on from the last one played (play, locate, a render's first block,
or any other jump of the playhead), the player first sends a note-off for every sounding note, then
chases at the block's first frame: each controller's last value before that point, then note-ons
for the notes sounding there. While the transport is not playing it sends note-offs for every
sounding note. A note-off the event buffer refuses is sent at the next block; every refused event
is counted, and the count is readable from the control side
([ADR-0145](../adrs/0145-midi-clips-play-through-a-per-track-real-time-note.md)).

## Live MIDI input

Live MIDI input reaches the audio thread through a bounded lock-free queue per input, filled on
the platform's MIDI thread, and is stamped with frames one block late: a message that arrived at
host time `t` lands at frame `(t − (callback host time − block length)) × sample rate / 10⁹` of the
block whose callback runs at that host time, clamped to the block. A message older than that lands
at frame 0; one at or after the callback's host time waits, with everything queued behind it, for
the next block; one more than a second ahead lands at the block's last frame. At most the event
buffer's spare capacity is taken each block; the rest waits
([ADR-0146](../adrs/0146-midi-devices-use-coremidi-through-objc2-core-midi.md)).

An armed MIDI track's input is drained every block, monitored or not. While monitored, its
messages enter the track's event source after the clip player's events for the block; in a block
split by the end of a count-in, each part is stamped from its own host time. The engine keeps
which live notes are sounding per event source: stop and pause send them note-offs in the next
block after the reset, and so does a re-arm that leaves the track out; a re-arm that keeps the
track on the same event source keeps its input and its sounding notes. Messages the event buffer
refuses are counted with the clip players'
([ADR-0150](../adrs/0150-midi-tracks-are-recorded-and-monitored-through-a.md)).

## Transport

Play starts from the playhead. Pause holds the playhead; stop returns it to where playback last
started. Clip and track edits are accepted only while the transport is stopped or paused; the
engine takes the project's clips when the transport starts or jumps. Record starts like play and
captures the armed tracks' inputs, after an optional count-in during which the transport is
counting in and the playhead holds; the playhead cannot be moved while recording (see
`docs/architecture/recording.md` "Capture" and "Count-in").

Stop and pause silence the plan at once: the engine resets it, so every node's state and every
feedback and compensation delay is cleared, and from the next block the output carries only
what the sources give, such as monitored input; every sounding MIDI note gets its note-off in that
block, as on any jump of the playhead (see "MIDI clip playback") ([ADR-0137](../adrs/0137-stop-and-pause-silence-the-plan-s-tails-at-once.md)).

A mono listening switch in the engine's output stage, after the plan, sends `(L + R) × 0.7071`
to both output channels. It is a monitoring setting and is not saved in the project ([ADR-0046](../adrs/0046-the-engine-renders-clips-on-the-reader-thread-and.md)).

## Offline render

Offline render runs the same plan and clip renderer as playback, block by block, without the
reader thread or real-time deadlines. It drops the plan's first `latency` frames, so output frame 0
is the first frame of the requested range, and renders exactly that range. For the same project
and range its samples equal playback's ([ADR-0046](../adrs/0046-the-engine-renders-clips-on-the-reader-thread-and.md)).
