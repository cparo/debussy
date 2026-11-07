//! State each stream shares between the control side, the reader thread and the real-time side.
//!
//! Counts that belong to one generation are packed with it into one `u64` (generation in the top
//! 16 bits, count in the low 48), so a reader of the value never pairs a count with the wrong
//! generation.

use std::sync::atomic::{AtomicU16, AtomicU64, Ordering};

use audio_buffers::Block;

/// A block of audio and the generation it was read for.
#[derive(Debug)]
pub(crate) struct Chunk {
    pub(crate) block: Block,
    pub(crate) generation: u16,
}

const COUNT_BITS: u32 = 48;
const COUNT_MASK: u64 = (1 << COUNT_BITS) - 1;
/// Set in a packed value's count when the generation's plan has been read to its end.
pub(crate) const ENDED: u64 = 1 << (COUNT_BITS - 1);

pub(crate) fn pack(generation: u16, count: u64) -> u64 {
    (u64::from(generation) << COUNT_BITS) | (count & COUNT_MASK)
}

pub(crate) fn unpack(value: u64) -> (u16, u64) {
    // The top 16 bits hold the generation, so this fits.
    ((value >> COUNT_BITS) as u16, value & COUNT_MASK)
}

/// One stream's shared state.
#[derive(Debug, Default)]
pub(crate) struct StreamShared {
    /// The current generation, set by the control side on each seek.
    pub(crate) generation: AtomicU16,
    /// Frames the reader has queued for a generation, with [`ENDED`] once it has queued them all.
    pub(crate) queued: AtomicU64,
    /// Frames the real-time side has played for a generation.
    pub(crate) played: AtomicU64,
    /// Underruns, counted by the real-time side.
    pub(crate) underruns: AtomicU64,
}

impl StreamShared {
    /// Frames queued but not yet played in the current generation, and whether the reader has
    /// reached the end of the generation's plan.
    pub(crate) fn buffered(&self) -> (u64, bool) {
        let generation = self.generation.load(Ordering::Acquire);
        let (queued_generation, queued) = unpack(self.queued.load(Ordering::Acquire));
        if queued_generation != generation {
            return (0, false);
        }
        let ended = queued & ENDED != 0;
        let queued = queued & !ENDED;
        let (played_generation, played) = unpack(self.played.load(Ordering::Acquire));
        let played = if played_generation == generation {
            played
        } else {
            0
        };
        (queued.saturating_sub(played), ended)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pack_round_trips() {
        for (generation, count) in [(0, 0), (1, 5), (u16::MAX, COUNT_MASK), (7, ENDED | 3)] {
            assert_eq!(unpack(pack(generation, count)), (generation, count));
        }
    }

    #[test]
    fn test_buffered_ignores_other_generations() {
        let shared = StreamShared::default();
        shared.generation.store(2, Ordering::Release);
        shared.queued.store(pack(1, 1000), Ordering::Release);
        assert_eq!(shared.buffered(), (0, false));
        shared
            .queued
            .store(pack(2, 1000 | ENDED), Ordering::Release);
        shared.played.store(pack(1, 900), Ordering::Release);
        assert_eq!(shared.buffered(), (1000, true));
        shared.played.store(pack(2, 400), Ordering::Release);
        assert_eq!(shared.buffered(), (600, true));
    }
}
