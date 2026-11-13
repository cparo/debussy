//! The node interface: what a processor declares and how it is run
//! (`docs/architecture/signal-graph.md` "Effect nodes").

use audio_buffers::Block;

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
/// parameters must change is replaced with `Graph::replace_node`. Only [`Node::process`] runs on the real-time thread, and it must not
/// allocate, free, lock, block or do I/O (`docs/architecture/engine.md` "Real-time thread").
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

    /// Processes [`Outputs::frames`] frames: reads `inputs` and overwrites every sample of every
    /// output. A node may be called several times per block on consecutive stretches of it, when
    /// it is in a feedback loop.
    fn process(&mut self, inputs: &Inputs<'_>, outputs: &mut Outputs<'_>);
}

/// A node's input signals for one stretch of a block, already summed, delay-compensated and
/// converted to each port's layout.
pub struct Inputs<'a> {
    blocks: &'a [Block],
    start: usize,
    frames: usize,
}

impl<'a> Inputs<'a> {
    pub(crate) fn new(blocks: &'a [Block], start: usize, frames: usize) -> Self {
        Inputs {
            blocks,
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
        self.blocks.len()
    }

    /// Channel `channel` (0 is mono or left, 1 is right) of input `port`, or `None` if there is
    /// no such port or channel.
    pub fn channel(&self, port: usize, channel: usize) -> Option<&[f32]> {
        self.blocks
            .get(port)?
            .channel(channel)?
            .get(self.start..self.start + self.frames)
    }
}

/// A node's output buffers for one stretch of a block.
pub struct Outputs<'a> {
    blocks: &'a mut [Block],
    start: usize,
    frames: usize,
}

impl<'a> Outputs<'a> {
    pub(crate) fn new(blocks: &'a mut [Block], start: usize, frames: usize) -> Self {
        Outputs {
            blocks,
            start,
            frames,
        }
    }

    /// The number of frames to write.
    pub fn frames(&self) -> usize {
        self.frames
    }

    /// The number of output ports.
    pub fn ports(&self) -> usize {
        self.blocks.len()
    }

    /// Channel `channel` of output `port`, mutably, or `None` if there is no such port or channel.
    pub fn channel_mut(&mut self, port: usize, channel: usize) -> Option<&mut [f32]> {
        let range = self.start..self.start + self.frames;
        self.blocks
            .get_mut(port)?
            .channel_mut(channel)?
            .get_mut(range)
    }

    /// Both channels of stereo output `port` at once, or `None` if it is missing or mono.
    pub fn stereo_mut(&mut self, port: usize) -> Option<(&mut [f32], &mut [f32])> {
        let range = self.start..self.start + self.frames;
        let (left, right) = self.blocks.get_mut(port)?.stereo_mut()?;
        Some((left.get_mut(range.clone())?, right.get_mut(range)?))
    }
}
