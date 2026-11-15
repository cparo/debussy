//! The node interface: what a processor declares and how it is run
//! (`docs/architecture/signal-graph.md` "Effect nodes").

use std::sync::atomic::AtomicU64;

use midi::{Event, Message};

use crate::buffers::{PortBuffer, count_overflow, frame};
use crate::param::Parameter;
use crate::types::Port;

/// A processor in the graph. Built-in effects implement it, and a third-party plugin wrapper
/// could: nothing here assumes the node is built in.
///
/// The graph calls [`Node::parameters`], then [`Node::set_tempo`], then [`Node::prepare`], then
/// [`Node::inputs`],
/// [`Node::outputs`] and [`Node::latency`], once each, off the real-time thread, when the node is
/// added; between the first two it may set parameter values (see `Graph::replace_node`). The
/// answers are fixed for as long as the node is in the graph. A node whose ports, latency or
/// parameters must change is replaced with `Graph::replace_node`. Only [`Node::process`] and
/// [`Node::reset`] run on the real-time thread, and they must not allocate, free, lock, block or
/// do I/O (`docs/architecture/engine.md` "Real-time thread").
pub trait Node: Send {
    /// The input ports, in order.
    fn inputs(&self) -> Vec<Port>;

    /// The output ports, in order.
    fn outputs(&self) -> Vec<Port>;

    /// The delay, in frames, between a sample entering the node and its effect leaving it. The
    /// graph delays parallel paths by it so they meet aligned.
    fn latency(&self) -> usize {
        0
    }

    /// The node's parameters, which the UI sets through the handles the graph keeps.
    fn parameters(&self) -> Vec<Parameter> {
        Vec::new()
    }

    /// Whether the node reads the project tempo. The graph replaces a node that does when the
    /// tempo changes (see `Graph::set_tempo`), so it is prepared again at the new tempo.
    fn uses_tempo(&self) -> bool {
        false
    }

    /// Gives the project tempo as the length of a whole note, in frames at the graph's rate.
    /// Called before [`Node::prepare`], off the real-time thread.
    fn set_tempo(&mut self, frames_per_whole_note: f64) {
        let _ = frames_per_whole_note;
    }

    /// Prepares to run at `sample_rate` with blocks of up to `max_frames`. May allocate.
    fn prepare(&mut self, sample_rate: u32, max_frames: usize) {
        let _ = (sample_rate, max_frames);
    }

    /// Clears what the node holds of past input, such as delay lines, filter memory and envelope
    /// levels, so it falls silent at once; parameters and their settings stay. Called on the
    /// real-time thread when the transport stops or pauses (`docs/architecture/engine.md`
    /// "Transport"). A node that holds no past input need not implement it.
    fn reset(&mut self) {}

    /// Processes [`Outputs::frames`] frames: reads `inputs`, overwrites every sample of every
    /// audio output, and sends the stretch's events from its event outputs, which start each
    /// block empty. A node may be called several times per block on consecutive stretches of it, when
    /// it is in a feedback loop.
    fn process(&mut self, inputs: &Inputs<'_>, outputs: &mut Outputs<'_>);
}

/// A node's input signals for one stretch of a block, already summed (audio) or merged
/// (events), delay-compensated and converted to each port's layout. Ports are numbered as the
/// node declared them; a control port has no signal.
pub struct Inputs<'a> {
    buffers: &'a [PortBuffer],
    start: usize,
    frames: usize,
}

impl<'a> Inputs<'a> {
    pub(crate) fn new(buffers: &'a [PortBuffer], start: usize, frames: usize) -> Self {
        Inputs {
            buffers,
            start,
            frames,
        }
    }

    /// The number of frames to process.
    pub fn frames(&self) -> usize {
        self.frames
    }

    /// The number of input ports.
    pub fn ports(&self) -> usize {
        self.buffers.len()
    }

    /// Channel `channel` (0 is mono or left, 1 is right) of audio input `port`, or `None` if there
    /// is no such audio port or channel.
    pub fn channel(&self, port: usize, channel: usize) -> Option<&[f32]> {
        self.buffers
            .get(port)?
            .audio()?
            .channel(channel)?
            .get(self.start..self.start + self.frames)
    }

    /// The events arriving at event input `port` in this stretch, in frame order, each frame
    /// counted from the stretch's start (`0..frames()`). Empty if there is no such event port.
    pub fn events(&self, port: usize) -> impl Iterator<Item = Event> + '_ {
        let range = frame(self.start)..frame(self.start + self.frames);
        let start = range.start;
        self.buffers
            .get(port)
            .and_then(PortBuffer::events)
            .map(|events| events.range(range))
            .unwrap_or_default()
            .iter()
            .map(move |event| Event {
                frame: event.frame - start,
                message: event.message,
            })
    }
}

/// A node's output buffers for one stretch of a block. Ports are numbered as the node declared
/// them.
pub struct Outputs<'a> {
    buffers: &'a mut [PortBuffer],
    start: usize,
    frames: usize,
    overflows: &'a AtomicU64,
}

impl<'a> Outputs<'a> {
    pub(crate) fn new(
        buffers: &'a mut [PortBuffer],
        start: usize,
        frames: usize,
        overflows: &'a AtomicU64,
    ) -> Self {
        Outputs {
            buffers,
            start,
            frames,
            overflows,
        }
    }

    /// The number of frames to write.
    pub fn frames(&self) -> usize {
        self.frames
    }

    /// The number of output ports.
    pub fn ports(&self) -> usize {
        self.buffers.len()
    }

    /// Channel `channel` of audio output `port`, mutably, or `None` if there is no such audio port
    /// or channel.
    pub fn channel_mut(&mut self, port: usize, channel: usize) -> Option<&mut [f32]> {
        let range = self.start..self.start + self.frames;
        self.buffers
            .get_mut(port)?
            .audio_mut()?
            .channel_mut(channel)?
            .get_mut(range)
    }

    /// Both channels of stereo audio output `port` at once, or `None` if it is missing or mono.
    pub fn stereo_mut(&mut self, port: usize) -> Option<(&mut [f32], &mut [f32])> {
        let range = self.start..self.start + self.frames;
        let (left, right) = self.buffers.get_mut(port)?.audio_mut()?.stereo_mut()?;
        Some((left.get_mut(range.clone())?, right.get_mut(range)?))
    }

    /// Sends `message` from event output `port` at `frame`, counted from the stretch's start, after
    /// any event already sent at that frame. Returns `false` if there is no such event port, if
    /// `frame` is not below [`Outputs::frames`], or if the port's buffer is full; a full buffer
    /// drops the message and counts it (see `Graph::event_overflows`).
    pub fn push_event(&mut self, port: usize, frame: u32, message: Message) -> bool {
        let Some(events) = self.buffers.get_mut(port).and_then(PortBuffer::events_mut) else {
            return false;
        };
        if frame as usize >= self.frames {
            return false;
        }
        let event = Event {
            frame: crate::buffers::frame(self.start).saturating_add(frame),
            message,
        };
        let pushed = events.push(event);
        if !pushed {
            count_overflow(self.overflows);
        }
        pushed
    }
}
