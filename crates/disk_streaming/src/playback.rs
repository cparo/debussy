//! The real-time side of one stream. Nothing here blocks, locks or allocates.

use std::sync::Arc;
use std::sync::atomic::Ordering;

use audio_buffers::{Consumer, Producer};

use crate::shared::{Chunk, StreamShared, pack};

/// The real-time end of one stream: plays the blocks the reader queued, in order.
///
/// Drop it off the real-time thread: it owns queue ends and blocks.
#[derive(Debug)]
pub struct Playback {
    shared: Arc<StreamShared>,
    chunks: Consumer<Chunk>,
    returns: Producer<Chunk>,
    /// The block being played, and how far into it.
    current: Option<(Chunk, usize)>,
    /// A block the return queue refused, retried on the next read.
    stuck: Option<Chunk>,
    /// Frames played in `played_generation`.
    played: u64,
    played_generation: u16,
}

impl Playback {
    pub(crate) fn new(
        shared: Arc<StreamShared>,
        chunks: Consumer<Chunk>,
        returns: Producer<Chunk>,
    ) -> Self {
        Self {
            shared,
            chunks,
            returns,
            current: None,
            stuck: None,
            played: 0,
            played_generation: 0,
        }
    }

    /// Fills `out` with the stream's next frames and returns how many were audio; the rest are
    /// silence. Missing audio counts as an underrun unless the stream has reached the end of its
    /// plan. Real-time safe.
    pub fn read(&mut self, out: &mut [f32]) -> usize {
        if let Some(chunk) = self.stuck.take() {
            self.retire(chunk);
        }
        let generation = self.shared.generation.load(Ordering::Acquire);
        if generation != self.played_generation {
            self.played_generation = generation;
            self.played = 0;
        }
        let mut filled = 0;
        while filled < out.len() {
            let (chunk, offset) = match self.current.take() {
                Some(current) => current,
                None => match self.chunks.try_pop() {
                    Some(chunk) => (chunk, 0),
                    None => break,
                },
            };
            if chunk.generation != generation {
                self.retire(chunk);
                continue;
            }
            let samples = chunk.block.channel(0).unwrap_or(&[]);
            let available = samples.len().saturating_sub(offset);
            let n = available.min(out.len() - filled);
            out[filled..filled + n].copy_from_slice(&samples[offset..offset + n]);
            filled += n;
            if offset + n < samples.len() {
                self.current = Some((chunk, offset + n));
            } else {
                self.retire(chunk);
            }
        }
        out[filled..].fill(0.0);
        self.played += filled as u64;
        self.shared
            .played
            .store(pack(generation, self.played), Ordering::Release);
        if filled < out.len() && !self.shared.buffered().1 {
            self.shared.underruns.fetch_add(1, Ordering::Relaxed);
        }
        filled
    }

    /// Whether at least `frames` frames of the current generation are buffered, or the stream
    /// holds less than that in total and all of it is buffered. Real-time safe.
    pub fn is_primed(&self, frames: u64) -> bool {
        let (buffered, ended) = self.shared.buffered();
        buffered >= frames || ended
    }

    /// Hands back the blocks queued for an earlier seek, up to the first one for the current
    /// position, which is kept to play next. Until then those blocks hold the stream's pool, so
    /// the reader could not read at the new position. Real-time safe.
    pub fn discard_stale(&mut self) {
        if let Some(chunk) = self.stuck.take() {
            self.retire(chunk);
        }
        let generation = self.shared.generation.load(Ordering::Acquire);
        if let Some((chunk, _)) = self.current.take_if(|(c, _)| c.generation != generation) {
            self.retire(chunk);
        }
        if self.current.is_some() {
            return;
        }
        while let Some(chunk) = self.chunks.try_pop() {
            if chunk.generation == generation {
                self.current = Some((chunk, 0));
                return;
            }
            self.retire(chunk);
        }
    }

    /// Underruns counted so far.
    pub fn underruns(&self) -> u64 {
        self.shared.underruns.load(Ordering::Relaxed)
    }

    /// Hands a played or stale block back to the reader, never freeing it here. The return queue
    /// has room for every block the stream owns, so it should never refuse one; if it does, the
    /// block waits in `stuck` for the next read.
    fn retire(&mut self, chunk: Chunk) {
        if let Err(chunk) = self.returns.try_push(chunk) {
            match self.stuck {
                None => self.stuck = Some(chunk),
                // Unreachable while the queue is sized to the pool. Leaking is the only way to
                // dispose of a block on this thread without freeing memory.
                Some(_) => std::mem::forget(chunk),
            }
        }
    }
}
