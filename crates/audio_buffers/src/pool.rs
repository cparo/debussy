//! A free list of preallocated blocks, so a thread can take and return blocks without allocating.
//!
//! A pool belongs to one thread. Blocks move between threads through queues; a block taken from
//! one thread's pool can be returned to another's.

use crate::block::{Block, Layout};
use crate::error::Error;

/// A fixed-size store of blocks of one layout and capacity.
#[derive(Debug)]
pub struct BlockPool {
    free: Vec<Block>,
    layout: Layout,
    block_capacity: usize,
}

impl BlockPool {
    /// A pool holding `count` silent blocks, with room for exactly that many. Allocates; call off
    /// the real-time thread.
    ///
    /// # Errors
    ///
    /// [`Error::ZeroCapacity`] if `count` is 0, or the error from [`Block::new`].
    pub fn new(layout: Layout, block_capacity: usize, count: usize) -> Result<Self, Error> {
        if count == 0 {
            return Err(Error::ZeroCapacity);
        }
        let mut free = Vec::with_capacity(count);
        for _ in 0..count {
            free.push(Block::new(layout, block_capacity)?);
        }
        Ok(Self {
            free,
            layout,
            block_capacity,
        })
    }

    /// Takes a free block, or `None` if every block is out. Does not allocate.
    ///
    /// The block keeps whatever samples and frame count it had when returned; call
    /// [`Block::clear`] if it must be silent.
    pub fn take(&mut self) -> Option<Block> {
        self.free.pop()
    }

    /// Returns a block to the pool. Does not allocate.
    ///
    /// # Errors
    ///
    /// Gives the block back if the pool is already full, or if the block's layout or capacity
    /// differs from the pool's. The caller must then keep it or send it to be dropped off the
    /// real-time thread.
    pub fn give(&mut self, block: Block) -> Result<(), Block> {
        let fits = block.layout() == self.layout && block.capacity() == self.block_capacity;
        if !fits || self.free.len() == self.free.capacity() {
            return Err(block);
        }
        self.free.push(block);
        Ok(())
    }

    /// The number of free blocks.
    pub fn available(&self) -> usize {
        self.free.len()
    }

    /// The number of blocks the pool can hold.
    pub fn capacity(&self) -> usize {
        self.free.capacity()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error as StdError;

    #[test]
    fn test_new_rejects_zero_count() {
        assert!(matches!(
            BlockPool::new(Layout::Mono, 64, 0),
            Err(Error::ZeroCapacity)
        ));
        assert!(matches!(
            BlockPool::new(Layout::Mono, 8, 1),
            Err(Error::CapacityOutOfRange(8))
        ));
    }

    #[test]
    fn test_take_and_give_round_trip() -> Result<(), Box<dyn StdError>> {
        let mut pool = BlockPool::new(Layout::Stereo, 128, 2)?;
        let a = pool.take().ok_or("empty")?;
        let b = pool.take().ok_or("empty")?;
        assert!(pool.take().is_none());
        assert_eq!(pool.available(), 0);
        pool.give(a).map_err(|_| "rejected")?;
        pool.give(b).map_err(|_| "rejected")?;
        assert_eq!(pool.available(), 2);
        Ok(())
    }

    #[test]
    fn test_give_rejects_when_full_or_mismatched() -> Result<(), Box<dyn StdError>> {
        let mut pool = BlockPool::new(Layout::Mono, 64, 1)?;
        assert!(pool.give(Block::new(Layout::Mono, 64)?).is_err(), "full");
        let _taken = pool.take().ok_or("empty")?;
        assert!(
            pool.give(Block::new(Layout::Stereo, 64)?).is_err(),
            "layout"
        );
        assert!(
            pool.give(Block::new(Layout::Mono, 128)?).is_err(),
            "capacity"
        );
        assert_eq!(pool.capacity(), 1);
        Ok(())
    }
}
