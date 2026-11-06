//! Every operation a real-time thread may call, run inside the real-time guard.

use std::error::Error;

use audio_buffers::{Block, BlockPool, Layout, queue};
use realtime_guard::{GuardAllocator, assert_realtime};

#[global_allocator]
static ALLOCATOR: GuardAllocator = GuardAllocator;

#[test]
fn test_block_operations_do_not_allocate() -> Result<(), Box<dyn Error>> {
    let mut block = Block::new(Layout::Stereo, 1024)?;
    let peak = assert_realtime(|| -> Result<f32, audio_buffers::Error> {
        block.set_frames(128)?;
        if let Some((left, right)) = block.stereo_mut() {
            left.fill(0.25);
            right.fill(-0.5);
        }
        if let Some(left) = block.channel_mut(0) {
            left[0] = 1.0;
        }
        let peak = (0..block.channels())
            .filter_map(|i| block.channel(i))
            .flatten()
            .fold(0.0_f32, |p, s| p.max(s.abs()));
        block.clear();
        Ok(peak)
    })?;
    assert_eq!(peak, 1.0);
    Ok(())
}

#[test]
fn test_pool_take_and_give_do_not_allocate() -> Result<(), Box<dyn Error>> {
    let mut pool = BlockPool::new(Layout::Mono, 128, 4)?;
    let mut spare = Some(Block::new(Layout::Mono, 128)?);
    let mut rejected = None;
    assert_realtime(|| {
        let a = pool.take();
        let b = pool.take();
        for block in [a, b].into_iter().flatten() {
            if let Err(block) = pool.give(block) {
                rejected = Some(block);
            }
        }
        // Full again, so the spare comes back rather than being stored or dropped.
        if let Some(block) = spare.take() {
            rejected = pool.give(block).err();
        }
    });
    assert!(rejected.is_some());
    assert_eq!(pool.available(), 4);
    Ok(())
}

#[test]
fn test_queue_push_pop_do_not_allocate() -> Result<(), Box<dyn Error>> {
    let (mut tx, mut rx) = queue::channel::<Block>(2)?;
    let mut blocks = [
        Some(Block::new(Layout::Mono, 32)?),
        Some(Block::new(Layout::Mono, 32)?),
    ];
    let mut extra = Some(Block::new(Layout::Mono, 32)?);
    let mut received = Vec::with_capacity(2);
    assert_realtime(|| {
        for block in blocks.iter_mut().filter_map(Option::take) {
            let _ = tx.try_push(block);
        }
        // Full: the block comes back instead of being dropped.
        if let Some(block) = extra.take() {
            extra = tx.try_push(block).err();
        }
        while let Some(block) = rx.try_pop() {
            received.push(block);
        }
        assert!(rx.try_pop().is_none());
    });
    assert_eq!(received.len(), 2);
    assert!(extra.is_some());
    Ok(())
}

#[test]
fn test_retiring_blocks_frees_them_off_the_realtime_thread() -> Result<(), Box<dyn Error>> {
    let (mut retire, mut collector) = queue::channel::<Block>(4)?;
    let mut owned = Some(Block::new(Layout::Stereo, 256)?);
    assert_realtime(|| {
        if let Some(block) = owned.take() {
            owned = retire.try_push(block).err();
        }
    });
    assert!(owned.is_none());
    // The non-real-time side drops what it receives.
    assert_eq!(collector.drain().count(), 1);
    Ok(())
}
