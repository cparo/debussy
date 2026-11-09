//! The real-time end of one take channel. Nothing here blocks, locks or allocates.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use audio_buffers::{Block, BlockPool, Consumer, Producer};

/// A block of recorded audio and the take frame it starts at.
#[derive(Debug)]
pub(crate) struct Stamped {
    pub(crate) block: Block,
    pub(crate) start: u64,
}

/// State one take channel shares between the real-time side, the writer and the control side.
#[derive(Debug, Default)]
pub(crate) struct TakeShared {
    /// Writes that dropped frames because no block was free.
    pub(crate) overruns: AtomicU64,
    /// The take's length in frames, dropped ones included; set before `ended`.
    pub(crate) frames: AtomicU64,
    /// Whether the take has ended: every block it filled has been queued.
    pub(crate) ended: AtomicBool,
}

/// The real-time end of one take channel: fills blocks from its pool and queues them for the
/// writer thread, stamped with where they start in the take
/// (`docs/architecture/recording.md` "Writer thread").
///
/// Drop it off the real-time thread: it owns queue ends and blocks. Dropping it ends the take.
#[derive(Debug)]
pub struct Take {
    shared: Arc<TakeShared>,
    pool: BlockPool,
    blocks: Producer<Stamped>,
    returns: Consumer<Block>,
    /// The block being filled, and how many frames it holds.
    current: Option<(Stamped, usize)>,
    /// Take frames so far, dropped ones included.
    position: u64,
    ended: bool,
}

impl Take {
    pub(crate) fn new(
        shared: Arc<TakeShared>,
        pool: BlockPool,
        blocks: Producer<Stamped>,
        returns: Consumer<Block>,
    ) -> Self {
        Take {
            shared,
            pool,
            blocks,
            returns,
            current: None,
            position: 0,
            ended: false,
        }
    }

    /// Appends `samples` and returns how many were kept. If no block is free, because the disk
    /// has fallen behind, the rest are dropped and an overrun is counted; the writer fills
    /// their place with silence. Does nothing once ended. Real-time safe.
    pub fn write(&mut self, samples: &[f32]) -> usize {
        if self.ended {
            return 0;
        }
        while let Some(block) = self.returns.try_pop() {
            // The pool was made with room for every block of the take.
            let _ = self.pool.give(block);
        }
        let mut kept = 0;
        let mut rest = samples;
        while !rest.is_empty() {
            let (stamped, filled) = match self.current.take() {
                Some(current) => current,
                None => match self.pool.take() {
                    Some(mut block) => {
                        // A pool block's capacity is its full length.
                        let _ = block.set_frames(block.capacity());
                        let start = self.position;
                        (Stamped { block, start }, 0)
                    }
                    None => {
                        self.position += rest.len() as u64;
                        self.shared.overruns.fetch_add(1, Ordering::Relaxed);
                        return kept;
                    }
                },
            };
            let mut stamped = stamped;
            let out = stamped.block.channel_mut(0).unwrap_or(&mut []);
            let n = out.len().saturating_sub(filled).min(rest.len());
            out[filled..filled + n].copy_from_slice(&rest[..n]);
            rest = &rest[n..];
            kept += n;
            self.position += n as u64;
            let filled = filled + n;
            if filled == stamped.block.capacity() {
                self.queue(stamped, filled);
            } else {
                self.current = Some((stamped, filled));
            }
        }
        kept
    }

    /// Ends the take: queues the block being filled and tells the writer the take's length.
    /// Later writes are ignored. Real-time safe.
    pub fn end(&mut self) {
        if self.ended {
            return;
        }
        if let Some((stamped, filled)) = self.current.take() {
            self.queue(stamped, filled);
        }
        self.shared.frames.store(self.position, Ordering::Relaxed);
        self.shared.ended.store(true, Ordering::Release);
        self.ended = true;
    }

    /// Overruns counted so far.
    pub fn overruns(&self) -> u64 {
        self.shared.overruns.load(Ordering::Relaxed)
    }

    /// Queues a block holding `filled` frames for the writer.
    fn queue(&mut self, mut stamped: Stamped, filled: usize) {
        // `filled` is at most the block's capacity.
        let _ = stamped.block.set_frames(filled);
        if let Err(stamped) = self.blocks.try_push(stamped) {
            // The queue holds as many blocks as the pool, so this does not happen; if it did,
            // the frames are lost like an overrun's and the writer pads them.
            let _ = self.pool.give(stamped.block);
            self.shared.overruns.fetch_add(1, Ordering::Relaxed);
        }
    }
}

impl Drop for Take {
    fn drop(&mut self) {
        self.end();
    }
}
