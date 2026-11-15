//! The real-time side: taking new plans at a block boundary and running the current one.

use audio_buffers::{Block, Consumer, Producer};
use midi::EventBuffer;

use crate::error::Error;
use crate::plan::Plan;
use crate::types::{Config, NodeId};

/// Runs the graph's current plan on the real-time thread. Every method is allocation-free and
/// never blocks.
///
/// Each block: [`Runner::begin_block`], then write every source with [`Runner::source_mut`] or
/// [`Runner::event_source_mut`], then [`Runner::process`], then read every sink with
/// [`Runner::sink`] or [`Runner::event_sink`].
pub struct Runner {
    plan: Box<Plan>,
    incoming: Consumer<Box<Plan>>,
    retire: Producer<Box<Plan>>,
    /// A replaced plan the retire queue had no room for yet.
    waiting: Option<Box<Plan>>,
    max_frames: usize,
}

impl Runner {
    pub(crate) fn new(
        plan: Plan,
        incoming: Consumer<Box<Plan>>,
        retire: Producer<Box<Plan>>,
        config: Config,
    ) -> Self {
        Runner {
            plan: Box::new(plan),
            incoming,
            retire,
            waiting: None,
            max_frames: config.max_frames,
        }
    }

    /// The largest block the graph runs, in frames.
    pub fn max_frames(&self) -> usize {
        self.max_frames
    }

    /// Starts a block of `frames`: takes up any newly committed plans, carrying their nodes'
    /// state over, and sends the replaced ones back to be dropped.
    ///
    /// # Errors
    ///
    /// [`Error::FramesOutOfRange`] if `frames` is more than the graph's largest block.
    pub fn begin_block(&mut self, frames: usize) -> Result<(), Error> {
        if frames > self.max_frames {
            return Err(Error::FramesOutOfRange {
                frames,
                max: self.max_frames,
            });
        }
        self.swap_plans();
        self.plan.set_frames(frames);
        Ok(())
    }

    /// The buffer to write audio source `id`'s signal into for this block, or `None` if the
    /// current plan has no such audio source.
    pub fn source_mut(&mut self, id: NodeId) -> Option<&mut Block> {
        self.plan.source_mut(id)
    }

    /// The buffer to push event source `id`'s events into for this block, each stamped with its
    /// frame in the block, or `None` if the current plan has no such event source. It starts each
    /// block empty; a push it refuses because it is full is the caller's to count.
    pub fn event_source_mut(&mut self, id: NodeId) -> Option<&mut EventBuffer> {
        self.plan.event_source_mut(id)
    }

    /// Event sink `id`'s events for this block, stamped with frames in the block, or `None` if the
    /// current plan has no such event sink.
    pub fn event_sink(&self, id: NodeId) -> Option<&EventBuffer> {
        self.plan.event_sink(id)
    }

    /// Sets where the coming block lies on the timeline: `position`, the timeline frame the
    /// sources' first frame comes from, and whether the timeline advances through the block.
    /// Curves are read from it (`docs/architecture/automation.md` "Evaluation"); kept until set
    /// again, from frame 0, stopped.
    pub fn set_transport(&mut self, position: i64, playing: bool) {
        self.plan.set_transport(position, playing);
    }

    /// Silences the current plan's tails: node state, feedback delays, compensation delays and
    /// pending events, so its sinks carry only what the sources give from the next block. Allocation-free; for
    /// when the transport stops or pauses (`docs/architecture/engine.md` "Transport").
    pub fn reset(&mut self) {
        self.plan.reset();
    }

    /// Runs the current plan over the block.
    pub fn process(&mut self) {
        self.plan.process();
    }

    /// Audio sink `id`'s signal for this block, or `None` if the current plan has no such audio
    /// sink.
    pub fn sink(&self, id: NodeId) -> Option<&Block> {
        self.plan.sink(id)
    }

    /// The current plan's latency from sources to sinks, in frames.
    pub fn latency(&self) -> usize {
        self.plan.latency
    }

    /// Swaps in every waiting plan, in order. A replaced plan goes back over the retire queue; if
    /// that is full, it is held, and no further plan is taken until it has gone.
    fn swap_plans(&mut self) {
        loop {
            if let Some(old) = self.waiting.take()
                && let Err(old) = self.retire.try_push(old)
            {
                self.waiting = Some(old);
                return;
            }
            let Some(mut next) = self.incoming.try_pop() else {
                return;
            };
            next.carry_over(&mut self.plan);
            self.waiting = Some(std::mem::replace(&mut self.plan, next));
        }
    }
}
