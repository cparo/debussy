//! The real-time side: taking new plans at a block boundary and running the current one.

use audio_buffers::{Block, Consumer, Producer};

use crate::error::Error;
use crate::plan::Plan;
use crate::types::{Config, NodeId};

/// Runs the graph's current plan on the real-time thread. Every method is allocation-free and
/// never blocks.
///
/// Each block: [`Runner::begin_block`], then write every source with [`Runner::source_mut`], then
/// [`Runner::process`], then read every sink with [`Runner::sink`].
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

    /// The buffer to write source `id`'s signal into for this block, or `None` if the current
    /// plan has no such source.
    pub fn source_mut(&mut self, id: NodeId) -> Option<&mut Block> {
        self.plan.source_mut(id)
    }

    /// Runs the current plan over the block.
    pub fn process(&mut self) {
        self.plan.process();
    }

    /// Sink `id`'s signal for this block, or `None` if the current plan has no such sink.
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
