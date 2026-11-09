# Recording

## Input monitoring

Each armed track has a monitor toggle, off by default. When on, the track's live input passes
through its part of the processing graph to the outputs ([ADR-0021](../adrs/0021-inputs-can-be-monitored-through-the-software-per.md)).
It does so whenever the track is armed, whether the transport is stopped, playing or recording:
the input channels replace the track's clip playback in its graph source. An armed track without
monitor plays silence while recording and its clips otherwise ([ADR-0078](../adrs/0078-recording-captures-armed-inputs-on-the-real-time.md)).

Monitored input is not delayed for delay compensation; its latency is the device round trip alone ([ADR-0032](../adrs/0032-monitored-input-is-not-delayed-for-delay.md)).

## Click track

The click is mixed into both output channels in the engine's output stage, after the plan and
after the mono fold. It is never part of the plan, the patch board, export or offline render. It
sounds while the transport is playing (and so recording) with the click on, and during a
[count-in](#count-in). Its timeline position
is the playhead minus the plan latency, so it lines up with the playback heard and with recorded
takes as `Take placement` places them. Each output frame sums the sound started at every beat
whose start lies within the sound's length before that frame, so the click depends only on the
timeline position, never on block sizes or seeks. Beats fall on the project grid (see
`docs/architecture/project.md` "Musical time"); the downbeat plays at the click level and other
beats 6 dB below it. The control side sends a new `audio_engine::ClickTrack` when the engine is
built and when the tempo or sound changes; the replaced one is dropped off the real-time thread.

The sounds are three bundled CC0 samples, Block, Click and Stick, in
`crates/engine_bridge/assets/clicks/`. `engine_bridge` decodes the chosen one, folds it to mono,
converts it to the project rate at import quality, peak-normalises it to −1 dBFS and trims its
tail below −60 dBFS. The click on/off switch is session state, like mono listening; the sound and
level (−24 to +6 dB, default 0) are per-machine settings (see `docs/architecture/project.md`
"Application settings") ([ADR-0083](../adrs/0083-the-click-is-mixed-at-the-output-stage-from.md)).

## Recording alignment

Recorded audio is placed on the timeline shifted by the input and output devices' reported
latencies, so that a take lines up with the playback the performer heard ([ADR-0022](../adrs/0022-the-engine-compensates-recording-and-effect.md)).

## Capture

The engine takes the device's interleaved input with each block and deinterleaves it into buffers
allocated when the engine is built; inputs beyond `audio_engine::MAX_INPUTS` are silent. A track
is armed with its graph source, the device input channel for each of its channels, its monitor
toggle, and one take sink per channel. Arming is accepted only while the transport is stopped or
paused, and the replaced set is dropped off the real-time thread. While any track is armed, the
real-time side raises an atomic peak per device input each block, which the control side reads
and clears for the input meters.

Record starts like play, from the playhead, with every other track playing, after any count-in
(see [Count-in](#count-in)). Capture begins with the first frame played, so a take's first frame
is the timeline frame the playhead held then. Stop or pause ends it. The engine publishes that start
frame and the plan's latency at the start for the control side. The real-time thread hands each
block's input to the sinks and does no file I/O. Seeking is refused while recording. Recording
runs from the playhead only; punch and loop recording are not built ([ADR-0078](../adrs/0078-recording-captures-armed-inputs-on-the-real-time.md)).

## Count-in

Record may count in for 0, 1 or 2 bars of the project's meter at its tempo, 1 by default; play
never does. The count-in is session state. Once every feed is primed the transport is
`CountingIn`: the tracks are silent (armed ones as when recording, monitored input still heard),
the playhead holds, and a signed position runs from the playhead minus the count-in to the
playhead. The click sounds at that position minus the plan latency whether or not it is on,
falling on the project grid extended before frame 0, so a count-in from mid-bar stays on the
beat. In the block where the count-in ends, the real-time side runs the count-in's frames, then
the rest of the block as playing with capture starting, so a take's first frame is exactly the
playhead. Stop or pause during the count-in ends the takes with nothing captured, and the empty
take is discarded ([ADR-0084](../adrs/0084-recording-s-count-in-splits-the-block-it-ends-in.md)).

## Writer thread

`disk_streaming`'s recorder writes each take channel to its own new mono 32-bit float WAV file
(see `docs/architecture/project.md` "Stored audio") on one non-real-time thread, `disk-recording`.
Each channel has a pool of 2 seconds of 1024-frame blocks. The real-time side fills a block,
stamps it with its take-relative start frame, and queues it; the writer appends it and returns
the empty block.

When the pool is empty because the disk fell behind, the real-time side drops the frames and
counts an overrun in an atomic counter. The writer fills the gap with silence from the stamps, so
the take stays aligned. Overruns are reported like underruns and never stop the transport. When
the take ends, the writer drains its queue, finishes the file, and reports the frames written or
the failure ([ADR-0078](../adrs/0078-recording-captures-armed-inputs-on-the-real-time.md)).

## Take placement

A take's timeline position is its start frame minus the device's input latency, its output
latency, and the plan latency at the start. If that is negative, the clip starts at frame 0 and
its offset into the file is the deficit, so the file keeps every sample. When every file of a
pass has finished, one clip per armed track is added in a single edit: one undo step and one
autosave. The new clip goes on the top layer. An empty take is discarded and its files removed
([ADR-0078](../adrs/0078-recording-captures-armed-inputs-on-the-real-time.md)).
