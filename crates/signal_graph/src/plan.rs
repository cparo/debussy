//! A processing plan and how it runs on the real-time thread
//! (`docs/architecture/signal-graph.md` "Processing graph").
//!
//! A plan is built off the real-time thread by `compile`, with every buffer and delay line it
//! needs. Running it allocates nothing: buffers are only cleared, summed into and handed to nodes.
//!
//! A plan with curve bindings runs in sub-blocks split at timeline frames that are multiples of
//! [`CONTROL_PERIOD`], setting each bound parameter's target at every split
//! (`docs/architecture/automation.md` "Evaluation").

use std::sync::Arc;

use audio_buffers::{Block, Layout};
use dsp::FOLD_DOWN_GAIN;
use modulation::{Cursor, Curve};

use crate::delay::DelayLine;
use crate::node::{Inputs, Node, Outputs};
use crate::param::{Parameter, map_to_parameter};
use crate::types::NodeId;

/// The control period, in timeline frames: curves are evaluated at multiples of it, and bound
/// parameters move linearly in between.
pub const CONTROL_PERIOD: i64 = 32;

/// What a slot runs.
pub(crate) enum Kind {
    /// A processor. `None` until its node is carried over from the previous plan.
    Node(Option<Box<dyn Node>>),
    /// A feedback delay.
    Delay(DelayLine),
    /// A signal the engine writes into the plan's one output.
    Source,
    /// A signal the engine reads from the plan's one input.
    Sink,
    /// An automation curve, read through the plan's bindings; runs nothing.
    Curve,
}

/// A curve driving one parameter.
pub(crate) struct Binding {
    curve: Arc<Curve>,
    cursor: Cursor,
    parameter: Parameter,
    /// The bound node's arrival time: the curve is read this many frames behind the timeline.
    latency: i64,
}

impl Binding {
    /// `curve` driving `parameter` of a node whose inputs arrive `latency` frames late.
    pub(crate) fn new(curve: Arc<Curve>, parameter: Parameter, latency: usize) -> Self {
        Binding {
            curve,
            cursor: Cursor::default(),
            parameter,
            latency: i64::try_from(latency).unwrap_or(i64::MAX),
        }
    }

    /// Sets the parameter to the curve's value at timeline frame `frame`, aligned to the node,
    /// to be reached over `ramp` frames.
    fn drive(&mut self, frame: i64, ramp: usize) {
        let value = self
            .cursor
            .value_at(&self.curve, frame.saturating_sub(self.latency));
        self.parameter
            .automate(map_to_parameter(value, self.parameter.info()), ramp);
    }
}

/// A connection into a slot, as the plan runs it.
pub(crate) struct Feed {
    /// The slot it comes from.
    pub(crate) from: usize,
    /// The output port it comes from.
    pub(crate) output: usize,
    /// The input port it goes to.
    pub(crate) input: usize,
    /// Its compensation delay.
    pub(crate) line: DelayLine,
    /// The left and right gains when a mono signal enters a stereo input.
    pub(crate) pan: (f32, f32),
}

/// One node of a plan with its buffers.
pub(crate) struct Slot {
    pub(crate) id: NodeId,
    pub(crate) kind: Kind,
    /// One buffer per input port, holding the summed signal arriving there.
    pub(crate) inputs: Vec<Block>,
    /// One buffer per output port.
    pub(crate) outputs: Vec<Block>,
    pub(crate) feeds: Vec<Feed>,
    /// The slot in the previous plan whose node or delay state this one takes over.
    pub(crate) carry_from: Option<usize>,
}

/// One step of the run order.
pub(crate) enum Step {
    /// Run a slot over the whole block.
    Run(usize),
    /// Run a cycle in sub-blocks of at most `sub_block` frames.
    Cycle {
        delays: Vec<usize>,
        members: Vec<usize>,
        sub_block: usize,
    },
}

/// An ordered, preallocated schedule compiled from a graph.
pub(crate) struct Plan {
    pub(crate) slots: Vec<Slot>,
    pub(crate) steps: Vec<Step>,
    /// The latency from every source to every sink, in frames.
    pub(crate) latency: usize,
    /// The curves driving parameters.
    bindings: Vec<Binding>,
    /// The frames in the current block.
    frames: usize,
    /// The timeline frame of the block's first frame, as the sources read it.
    position: i64,
    /// Whether the timeline advances through the block.
    playing: bool,
}

impl Plan {
    /// A plan of `slots` run in `steps`, with curves driving parameters through `bindings`.
    pub(crate) fn new(
        slots: Vec<Slot>,
        steps: Vec<Step>,
        bindings: Vec<Binding>,
        latency: usize,
    ) -> Self {
        Plan {
            slots,
            steps,
            latency,
            bindings,
            frames: 0,
            position: 0,
            playing: false,
        }
    }

    /// Sets where the coming block lies on the timeline: its first frame, and whether the
    /// timeline advances through it.
    pub(crate) fn set_transport(&mut self, position: i64, playing: bool) {
        self.position = position;
        self.playing = playing;
    }

    /// The ids of the plan's slots, in slot order.
    pub(crate) fn ids(&self) -> Vec<NodeId> {
        self.slots.iter().map(|slot| slot.id).collect()
    }

    /// Takes over the nodes and feedback-delay state of `previous`, the plan this one replaces,
    /// and releases the parameters `previous` drove that this plan does not. Moves only;
    /// allocation-free.
    pub(crate) fn carry_over(&mut self, previous: &mut Plan) {
        for old in &previous.bindings {
            if !self
                .bindings
                .iter()
                .any(|b| b.parameter.same(&old.parameter))
            {
                old.parameter.release();
            }
        }
        self.position = previous.position;
        self.playing = previous.playing;
        for slot in &mut self.slots {
            let Some(old) = slot.carry_from.and_then(|k| previous.slots.get_mut(k)) else {
                continue;
            };
            if old.id != slot.id {
                continue;
            }
            match (&mut slot.kind, &mut old.kind) {
                (Kind::Node(new), Kind::Node(old)) if new.is_none() => std::mem::swap(new, old),
                (Kind::Delay(new), Kind::Delay(old)) => new.take_state(old),
                _ => {}
            }
        }
    }

    /// Sets the length of the coming block, which must fit every buffer.
    pub(crate) fn set_frames(&mut self, frames: usize) {
        self.frames = frames;
        for slot in &mut self.slots {
            for block in slot.inputs.iter_mut().chain(slot.outputs.iter_mut()) {
                // Every buffer holds the graph's largest block, which `frames` was checked
                // against.
                let _ = block.set_frames(frames);
            }
        }
    }

    /// The output buffer of source `id`.
    pub(crate) fn source_mut(&mut self, id: NodeId) -> Option<&mut Block> {
        let slot = self.slots.iter_mut().find(|slot| slot.id == id)?;
        match slot.kind {
            Kind::Source => slot.outputs.first_mut(),
            _ => None,
        }
    }

    /// The input buffer of sink `id`.
    pub(crate) fn sink(&self, id: NodeId) -> Option<&Block> {
        let slot = self.slots.iter().find(|slot| slot.id == id)?;
        match slot.kind {
            Kind::Sink => slot.inputs.first(),
            _ => None,
        }
    }

    /// Runs the plan over the current block. With bindings, while playing, the block is split
    /// at timeline frames that are multiples of [`CONTROL_PERIOD`]: before each piece every bound
    /// parameter is set to its curve's value at the next multiple, reached at that multiple, so
    /// the parameter moves linearly between control points whatever the block size. While
    /// stopped, each curve is read once at the block's position and reached over the block.
    pub(crate) fn process(&mut self) {
        let frames = self.frames;
        let Plan {
            slots,
            steps,
            bindings,
            position,
            playing,
            ..
        } = self;
        if bindings.is_empty() {
            run_steps(slots, steps, 0, frames);
            return;
        }
        if !*playing {
            for binding in bindings.iter_mut() {
                binding.drive(*position, frames);
            }
            run_steps(slots, steps, 0, frames);
            return;
        }
        let mut start = 0;
        while start < frames {
            let frame = position.saturating_add(start as i64);
            let next = (frame.div_euclid(CONTROL_PERIOD) + 1) * CONTROL_PERIOD;
            // At most `CONTROL_PERIOD`, so the casts are exact.
            let to_next = (next - frame) as usize;
            for binding in bindings.iter_mut() {
                binding.drive(next, to_next);
            }
            let length = to_next.min(frames - start);
            run_steps(slots, steps, start, length);
            start += length;
        }
    }
}

/// Runs every step over `start..start + frames`.
fn run_steps(slots: &mut [Slot], steps: &[Step], start: usize, frames: usize) {
    let end = start + frames;
    for step in steps {
        match step {
            Step::Run(index) => {
                gather(slots, *index, start, frames);
                run(&mut slots[*index], start, frames);
            }
            Step::Cycle {
                delays,
                members,
                sub_block,
            } => {
                let mut offset = start;
                while offset < end {
                    let length = (*sub_block).min(end - offset);
                    for &delay in delays {
                        emit(&mut slots[delay], offset, length);
                    }
                    for &member in members {
                        gather(slots, member, offset, length);
                        run(&mut slots[member], offset, length);
                    }
                    for &delay in delays {
                        gather(slots, delay, offset, length);
                        absorb(&mut slots[delay], offset, length);
                    }
                    offset += length;
                }
            }
        }
    }
}

/// Sums every feed into slot `index`'s inputs over `start..start + frames`.
fn gather(slots: &mut [Slot], index: usize, start: usize, frames: usize) {
    // Taking the vectors leaves empty ones, which neither allocate nor free.
    let mut inputs = std::mem::take(&mut slots[index].inputs);
    let mut feeds = std::mem::take(&mut slots[index].feeds);
    for block in &mut inputs {
        for channel in 0..block.channels() {
            if let Some(range) = block
                .channel_mut(channel)
                .and_then(|s| s.get_mut(start..start + frames))
            {
                range.fill(0.0);
            }
        }
    }
    for feed in &mut feeds {
        let source = &slots[feed.from].outputs[feed.output];
        accumulate(source, &mut inputs[feed.input], feed, start, frames);
    }
    slots[index].inputs = inputs;
    slots[index].feeds = feeds;
}

/// Adds `source`, delayed by the feed's line and converted to `target`'s layout, into `target`.
fn accumulate(source: &Block, target: &mut Block, feed: &mut Feed, start: usize, frames: usize) {
    let range = start..start + frames;
    let line = &mut feed.line;
    let (Some(first), second) = (source.channel(0), source.channel(1)) else {
        return;
    };
    match (source.layout(), target.layout()) {
        (Layout::Mono, Layout::Mono) | (Layout::Stereo, Layout::Mono) => {
            let Some(out) = target.channel_mut(0).and_then(|s| s.get_mut(range.clone())) else {
                return;
            };
            for (offset, sample) in out.iter_mut().enumerate() {
                let t = start + offset;
                let a = line.exchange(0, first[t]);
                *sample += match second {
                    Some(second) => (a + line.exchange(1, second[t])) * FOLD_DOWN_GAIN,
                    None => a,
                };
                line.advance();
            }
        }
        (_, Layout::Stereo) => {
            let Some((left, right)) = target.stereo_mut() else {
                return;
            };
            let (left, right) = (&mut left[range.clone()], &mut right[range]);
            for (offset, (l, r)) in left.iter_mut().zip(right.iter_mut()).enumerate() {
                let t = start + offset;
                let a = line.exchange(0, first[t]);
                match second {
                    Some(second) => {
                        *l += a;
                        *r += line.exchange(1, second[t]);
                    }
                    None => {
                        *l += a * feed.pan.0;
                        *r += a * feed.pan.1;
                    }
                }
                line.advance();
            }
        }
    }
}

/// Runs a slot over `start..start + frames`, its inputs already gathered.
fn run(slot: &mut Slot, start: usize, frames: usize) {
    match &mut slot.kind {
        Kind::Node(Some(node)) => {
            let inputs = Inputs::new(&slot.inputs, start, frames);
            let mut outputs = Outputs::new(&mut slot.outputs, start, frames);
            node.process(&inputs, &mut outputs);
        }
        Kind::Node(None) => {
            for block in &mut slot.outputs {
                block.clear();
            }
        }
        Kind::Delay(line) => {
            // Outside a cycle, a delay runs in pieces no longer than itself.
            let mut offset = start;
            while offset < start + frames {
                let length = line.length().min(start + frames - offset);
                if let (Some(input), Some(output)) = (slot.inputs.first(), slot.outputs.first_mut())
                {
                    line.emit(output, offset, length);
                    line.absorb(input, offset, length);
                }
                offset += length;
            }
        }
        Kind::Source | Kind::Sink | Kind::Curve => {}
    }
}

/// Writes a feedback delay's output for `start..start + frames` from what it already holds.
fn emit(slot: &mut Slot, start: usize, frames: usize) {
    if let (Kind::Delay(line), Some(output)) = (&slot.kind, slot.outputs.first_mut()) {
        line.emit(output, start, frames);
    }
}

/// Stores a feedback delay's gathered input for `start..start + frames`.
fn absorb(slot: &mut Slot, start: usize, frames: usize) {
    if let (Kind::Delay(line), Some(input)) = (&mut slot.kind, slot.inputs.first()) {
        line.absorb(input, start, frames);
    }
}
