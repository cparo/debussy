//! Recording: armed tracks, the device input they read, and where takes go
//! (`docs/architecture/recording.md` "Capture", "Input monitoring", "Take placement",
//! "MIDI recording").

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use ::midi::{Kind, LiveInput};
use signal_graph::{EVENT_CAPACITY, EventBuffer, Message, NodeId};

/// The most device input channels the engine reads; later ones are silent.
pub const MAX_INPUTS: usize = 64;

/// Where one channel of a take goes. Normally a disk recorder's real-time end.
pub trait TakeSink: Send {
    /// Appends `samples` to the take and returns how many it kept; the rest were dropped (an
    /// overrun) and their place in the take is filled with silence. Called on the real-time
    /// thread: must not block, lock or allocate.
    fn write(&mut self, samples: &[f32]) -> usize;

    /// Ends the take; later writes are ignored. Called on the real-time thread: must not block,
    /// lock or allocate.
    fn end(&mut self);
}

/// One armed track.
pub struct ArmedTrack {
    /// The track's source node in the processing graph.
    pub source: NodeId,
    /// The device input channel (from 0) for each of the source's channels, left first.
    pub inputs: Vec<usize>,
    /// Whether the input replaces the track's clip playback in its source.
    pub monitor: bool,
    /// Where each channel's take goes, one per entry of `inputs`; empty to monitor without
    /// recording. A sink takes one take only: arm again with new sinks before each recording.
    pub sinks: Vec<Box<dyn TakeSink>>,
}

/// Where an armed MIDI track's take goes. Normally a disk recorder's real-time end.
pub trait NoteSink: Send {
    /// Appends `message`, received at take frame `frame` (from the take's first frame), and
    /// returns whether it was kept; a message dropped is an overrun. Called on the real-time
    /// thread: must not block, lock or allocate.
    fn write(&mut self, frame: u64, message: Message) -> bool;

    /// Ends the take, `frames` frames long; later writes are ignored. Called on the real-time
    /// thread: must not block, lock or allocate.
    fn end(&mut self, frames: u64);
}

/// The most armed MIDI tracks whose sounding notes a single re-arm can release
/// (`docs/architecture/engine.md` "Live MIDI input").
pub(crate) const MAX_RELEASING: usize = 64;

/// Which live notes are sounding in an event source, by channel and key.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct HeldNotes([u128; 16]);

impl HeldNotes {
    /// Notes `message` on or off: a note-on with velocity 0 is a note-off. Other messages are
    /// ignored.
    pub(crate) fn apply(&mut self, message: Message) {
        let channel = usize::from(message.channel() & 0x0F);
        match message.kind() {
            Kind::NoteOn { note, velocity } if velocity > 0 => {
                self.0[channel] |= 1 << (note & 0x7F);
            }
            Kind::NoteOn { note, .. } | Kind::NoteOff { note, .. } => {
                self.0[channel] &= !(1 << (note & 0x7F));
            }
            _ => {}
        }
    }

    /// Whether no note is held.
    pub(crate) fn is_empty(&self) -> bool {
        self.0.iter().all(|&keys| keys == 0)
    }

    /// Forgets every held note.
    pub(crate) fn clear(&mut self) {
        self.0 = [0; 16];
    }

    /// Sends a note-off at frame 0 for every held note into `events`, forgetting each one sent.
    /// Returns how many the buffer refused; those stay held.
    pub(crate) fn release(&mut self, events: &mut EventBuffer) -> u64 {
        let mut refused = 0;
        for (channel, keys) in self.0.iter_mut().enumerate() {
            let mut rest = *keys;
            while rest != 0 {
                let note = rest.trailing_zeros() as u8;
                rest &= rest - 1;
                let Some(message) = Message::note_off(channel as u8, note, 0) else {
                    continue;
                };
                if events.push(signal_graph::Event { frame: 0, message }) {
                    *keys &= !(1 << note);
                } else {
                    refused += 1;
                }
            }
        }
        refused
    }
}

/// One armed MIDI track: its live input, played into its event source and captured into its
/// take (`docs/architecture/recording.md` "MIDI recording").
///
/// Build and drop it off the real-time thread.
pub struct ArmedMidi {
    source: NodeId,
    input: Option<LiveInput>,
    monitor: bool,
    sink: Option<Box<dyn NoteSink>>,
    activity: Arc<AtomicU64>,
    held: HeldNotes,
    scratch: EventBuffer,
}

impl ArmedMidi {
    /// An armed MIDI track playing into event source `source`. `input` is the live input it
    /// reads; with `None` it takes over the input of the track it replaces on the same source
    /// (see [`Engine::arm`](crate::Engine::arm)), so re-arming need not reopen the device.
    /// With `monitor` the input is heard. `sink` takes the take; `None` to monitor without
    /// recording. `activity` counts every message received, for the interface's activity
    /// light. Allocates.
    pub fn new(
        source: NodeId,
        input: Option<LiveInput>,
        monitor: bool,
        sink: Option<Box<dyn NoteSink>>,
        activity: Arc<AtomicU64>,
    ) -> Self {
        ArmedMidi {
            source,
            input,
            monitor,
            sink,
            activity,
            held: HeldNotes::default(),
            scratch: EventBuffer::new(EVENT_CAPACITY),
        }
    }

    /// The event source it plays into.
    pub fn source(&self) -> NodeId {
        self.source
    }

    /// Whether it has a take.
    pub fn has_sink(&self) -> bool {
        self.sink.is_some()
    }

    /// Takes over `previous`'s input, if this one has none, and its held notes, which this one
    /// then releases. Allocation-free.
    pub(crate) fn carry_over(&mut self, previous: &mut ArmedMidi) {
        if self.input.is_none() {
            self.input = previous.input.take();
        }
        self.held = previous.held;
        previous.held.clear();
    }

    /// Takes the held notes, leaving none.
    pub(crate) fn take_held(&mut self) -> HeldNotes {
        std::mem::take(&mut self.held)
    }

    /// Ends the take, `frames` frames long, if there is one.
    pub(crate) fn end_take(&mut self, frames: u64) {
        if let Some(sink) = &mut self.sink {
            sink.end(frames);
        }
    }

    /// Drains the messages that arrived before this part of a block (see [`LiveInput::fill`];
    /// the part is `frames` frames whose callback ran at host time `host_nanos`), into `events`,
    /// the source's buffer, if monitored and the source is there, and into the take at take
    /// frame `capture` onward if given. Returns how many the event buffer refused. Real-time
    /// safe.
    pub(crate) fn feed(
        &mut self,
        events: Option<&mut EventBuffer>,
        host_nanos: u64,
        frames: u32,
        sample_rate: u32,
        capture: Option<u64>,
    ) -> u64 {
        self.scratch.clear();
        let Some(input) = &mut self.input else {
            return 0;
        };
        let moved = input.fill(&mut self.scratch, host_nanos, frames, sample_rate);
        if moved > 0 {
            self.activity.fetch_add(moved as u64, Ordering::Relaxed);
        }
        let mut refused = 0;
        let mut events = events.filter(|_| self.monitor);
        for event in self.scratch.iter() {
            if let Some(events) = events.as_deref_mut() {
                if events.push(*event) {
                    self.held.apply(event.message);
                } else {
                    refused += 1;
                }
            }
            if let (Some(start), Some(sink)) = (capture, &mut self.sink) {
                sink.write(start + u64::from(event.frame), event.message);
            }
        }
        refused
    }
}

/// Where the last recording's capture began, published by the real-time side.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TakeInfo {
    /// The timeline frame the take's first frame was captured at.
    pub start_frame: u64,
    /// The plan's output latency then, in frames.
    pub latency: u64,
}

impl TakeInfo {
    /// Where to place the take: its timeline position and the offset into its files of the
    /// first frame to play. `device_latency` is the device's input plus output latency. The
    /// take is shifted back by that and the plan latency; a shift past frame 0 becomes the
    /// offset, so no recorded frame is lost.
    pub fn placement(&self, device_latency: u64) -> (u64, u64) {
        self.shifted(device_latency)
    }

    /// Where to place a MIDI take, as [`TakeInfo::placement`] does for audio. `midi_latency` is
    /// the device's output latency plus one device block, the delay with which live input is
    /// stamped (`docs/architecture/recording.md` "MIDI recording"): a note is placed where the
    /// player heard the timeline when they played it.
    pub fn midi_placement(&self, midi_latency: u64) -> (u64, u64) {
        self.shifted(midi_latency)
    }

    /// The take shifted back by `device` frames and the plan latency, as a position and offset.
    fn shifted(&self, device: u64) -> (u64, u64) {
        let shift = device.saturating_add(self.latency);
        match self.start_frame.checked_sub(shift) {
            Some(position) => (position, 0),
            None => (0, shift - self.start_frame),
        }
    }
}

/// The deinterleaved device input of the current block, allocated once.
pub(crate) struct Input {
    channels: Vec<Box<[f32]>>,
    /// Device channels holding this block's input; later ones are silent.
    live: usize,
    /// The first frame of the block's input still to be used: past a count-in.
    start: usize,
    silence: Box<[f32]>,
}

impl Input {
    /// Room for [`MAX_INPUTS`] channels of `max_frames`.
    pub(crate) fn new(max_frames: usize) -> Self {
        Input {
            channels: (0..MAX_INPUTS)
                .map(|_| vec![0.0; max_frames].into_boxed_slice())
                .collect(),
            live: 0,
            start: 0,
            silence: vec![0.0; max_frames].into_boxed_slice(),
        }
    }

    /// Takes `frames` frames of `interleaved` input with `channels` channels. Input shorter than
    /// that is silence.
    pub(crate) fn load(&mut self, interleaved: &[f32], channels: usize, frames: usize) {
        let complete = channels > 0 && interleaved.len() >= frames.saturating_mul(channels);
        let live = if complete {
            channels.min(MAX_INPUTS)
        } else {
            0
        };
        for (c, buffer) in self.channels.iter_mut().take(live).enumerate() {
            let Some(buffer) = buffer.get_mut(..frames) else {
                continue;
            };
            for (f, sample) in buffer.iter_mut().enumerate() {
                *sample = interleaved[f * channels + c];
            }
        }
        self.live = live;
        self.start = 0;
    }

    /// Moves past `frames` frames of the block's input: later calls see the rest of it.
    pub(crate) fn skip(&mut self, frames: usize) {
        self.start = self.start.saturating_add(frames);
    }

    /// Device channels holding this block's input.
    pub(crate) fn live(&self) -> usize {
        self.live
    }

    /// The largest absolute sample in device channel `channel`'s first `frames` frames.
    pub(crate) fn peak(&self, channel: usize, frames: usize) -> f32 {
        self.channel(channel, frames)
            .iter()
            .fold(0.0, |peak: f32, s| peak.max(s.abs()))
    }

    /// Device channel `channel`'s next `frames` frames, silence if it has none.
    pub(crate) fn channel(&self, channel: usize, frames: usize) -> &[f32] {
        let buffer = if channel < self.live {
            self.channels.get(channel).unwrap_or(&self.silence)
        } else {
            &self.silence
        };
        let end = self.start.saturating_add(frames);
        buffer.get(self.start..end).unwrap_or(&[])
    }
}

/// Whether `source` belongs to an armed track whose input or silence replaces its clips:
/// monitored, or recording.
pub(crate) fn replaces_clips(armed: &[ArmedTrack], source: NodeId, recording: bool) -> bool {
    armed
        .iter()
        .any(|a| a.source == source && (a.monitor || recording))
}

/// Ends every armed take; MIDI takes are `frames` frames long.
pub(crate) fn end_takes(armed: &mut [ArmedTrack], midi: &mut [ArmedMidi], frames: u64) {
    for sink in armed.iter_mut().flat_map(|a| a.sinks.iter_mut()) {
        sink.end();
    }
    for track in midi {
        track.end_take(frames);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_placement_shifts_back_by_every_latency() {
        let info = TakeInfo {
            start_frame: 10_000,
            latency: 300,
        };
        assert_eq!(info.placement(128), (9572, 0));
    }

    #[test]
    fn test_placement_before_zero_becomes_offset() {
        let info = TakeInfo {
            start_frame: 100,
            latency: 50,
        };
        assert_eq!(info.placement(128), (0, 78));
        assert_eq!(info.placement(50), (0, 0));
    }

    fn note_on(channel: u8, note: u8, velocity: u8) -> Result<Message, &'static str> {
        Message::note_on(channel, note, velocity).ok_or("bad note")
    }

    #[test]
    fn test_held_notes_follow_note_on_and_off() -> Result<(), Box<dyn std::error::Error>> {
        let mut held = HeldNotes::default();
        held.apply(note_on(0, 60, 100)?);
        held.apply(note_on(15, 127, 1)?);
        held.apply(note_on(3, 0, 50)?);
        // Velocity 0 and a note-off both release; other messages are ignored.
        held.apply(note_on(3, 0, 0)?);
        held.apply(Message::control_change(0, 60, 1).ok_or("bad cc")?);
        held.apply(Message::note_off(15, 127, 0).ok_or("bad note")?);
        let mut events = EventBuffer::new(8);
        assert_eq!(held.release(&mut events), 0);
        let released: Vec<&[u8]> = events.iter().map(|e| e.message.bytes()).collect();
        assert_eq!(released, [[0x80, 60, 0]]);
        assert!(held.is_empty());
        Ok(())
    }

    #[test]
    fn test_held_notes_release_keeps_what_the_buffer_refuses()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut held = HeldNotes::default();
        for note in 0..4 {
            held.apply(note_on(1, note, 100)?);
        }
        let mut events = EventBuffer::new(3);
        assert_eq!(held.release(&mut events), 1);
        assert!(!held.is_empty());
        events.clear();
        assert_eq!(held.release(&mut events), 0);
        assert_eq!(events.len(), 1);
        assert!(held.is_empty());
        Ok(())
    }

    #[test]
    fn test_midi_placement_shifts_back_by_output_side_latency() {
        let info = TakeInfo {
            start_frame: 1000,
            latency: 100,
        };
        assert_eq!(info.midi_placement(64 + 256), (580, 0));
        assert_eq!(info.midi_placement(1200), (0, 300));
    }

    #[test]
    fn test_load_deinterleaves_and_silences_missing_channels() {
        let mut input = Input::new(4);
        input.load(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0], 2, 3);
        assert_eq!(input.channel(0, 3), &[1.0, 3.0, 5.0]);
        assert_eq!(input.channel(1, 3), &[2.0, 4.0, 6.0]);
        assert_eq!(input.channel(2, 3), &[0.0; 3]);
        // Too short for the frames asked: silence.
        input.load(&[1.0, 2.0], 2, 3);
        assert_eq!(input.channel(0, 3), &[0.0; 3]);
        input.load(&[], 0, 3);
        assert_eq!(input.channel(1, 3), &[0.0; 3]);
    }

    #[test]
    fn test_skip_moves_past_frames_until_the_next_load() {
        let mut input = Input::new(4);
        input.load(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0], 2, 3);
        input.skip(2);
        assert_eq!(input.channel(0, 1), &[5.0]);
        assert_eq!(input.channel(1, 1), &[6.0]);
        input.load(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0], 2, 3);
        assert_eq!(input.channel(0, 3), &[1.0, 3.0, 5.0]);
    }

    #[test]
    fn test_peak_is_the_largest_magnitude() {
        let mut input = Input::new(4);
        input.load(&[0.1, -0.2, -0.7, 0.3, 0.5, 0.0], 2, 3);
        assert_eq!(
            (input.live(), input.peak(0, 3), input.peak(1, 3)),
            (2, 0.7, 0.3)
        );
        assert_eq!(input.peak(2, 3), 0.0);
    }
}
