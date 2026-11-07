//! A processing plan and how it runs on the real-time thread
//! (`docs/architecture/signal-graph.md` "Processing graph").
//!
//! A plan is built off the real-time thread by `compile`, with every buffer and delay line it
//! needs. Running it allocates nothing: buffers are only cleared, summed into and handed to nodes.

use audio_buffers::{Block, Layout};
use dsp::FOLD_DOWN_GAIN;

use crate::delay::DelayLine;
use crate::node::{Inputs, Node, Outputs};
use crate::types::NodeId;

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
    /// The frames in the current block.
    frames: usize,
}

impl Plan {
    /// A plan of `slots` run in `steps`.
    pub(crate) fn new(slots: Vec<Slot>, steps: Vec<Step>, latency: usize) -> Self {
        Plan {
            slots,
            steps,
            latency,
            frames: 0,
        }
    }

    /// The ids of the plan's slots, in slot order.
    pub(crate) fn ids(&self) -> Vec<NodeId> {
        self.slots.iter().map(|slot| slot.id).collect()
    }

    /// Takes over the nodes and feedback-delay state of `previous`, the plan this one replaces.
    /// Moves only; allocation-free.
    pub(crate) fn carry_over(&mut self, previous: &mut Plan) {
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

    /// Runs the plan over the current block.
    pub(crate) fn process(&mut self) {
        let frames = self.frames;
        let Plan { slots, steps, .. } = self;
        for step in steps.iter() {
            match step {
                Step::Run(index) => {
                    gather(slots, *index, 0, frames);
                    run(&mut slots[*index], 0, frames);
                }
                Step::Cycle {
                    delays,
                    members,
                    sub_block,
                } => {
                    let mut start = 0;
                    while start < frames {
                        let length = (*sub_block).min(frames - start);
                        for &delay in delays {
                            emit(&mut slots[delay], start, length);
                        }
                        for &member in members {
                            gather(slots, member, start, length);
                            run(&mut slots[member], start, length);
                        }
                        for &delay in delays {
                            gather(slots, delay, start, length);
                            absorb(&mut slots[delay], start, length);
                        }
                        start += length;
                    }
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
        Kind::Source | Kind::Sink => {}
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
