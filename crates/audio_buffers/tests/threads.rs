//! One producer thread and one consumer thread, transferring many items through the queue.
//!
//! These tests join threads, which `clippy.toml` bans on the real-time thread; the joins here run
//! on the test's own thread.
#![allow(clippy::disallowed_methods)]

use std::error::Error;
use std::hint::spin_loop;
use std::thread;

use audio_buffers::{Block, BlockPool, Layout, queue};
use realtime_guard::{GuardAllocator, assert_realtime};

#[global_allocator]
static ALLOCATOR: GuardAllocator = GuardAllocator;

#[test]
fn test_queue_transfers_items_in_order_across_threads() -> Result<(), Box<dyn Error>> {
    const COUNT: u64 = 1_000_000;
    let (mut tx, mut rx) = queue::channel::<u64>(64)?;
    let producer = thread::spawn(move || {
        for i in 0..COUNT {
            let mut item = i;
            while let Err(back) = tx.try_push(item) {
                item = back;
                spin_loop();
            }
        }
    });
    let mut expected = 0;
    while expected < COUNT {
        match rx.try_pop() {
            Some(item) => {
                if item != expected {
                    return Err(format!("got {item}, expected {expected}").into());
                }
                expected += 1;
            }
            None => spin_loop(),
        }
    }
    assert!(rx.try_pop().is_none());
    producer.join().map_err(|_| "producer panicked")?;
    Ok(())
}

/// A disk-reader–style thread fills pooled blocks and sends them to a real-time consumer, which
/// checks them under the guard and sends them back for reuse.
#[test]
fn test_blocks_round_trip_between_threads() -> Result<(), Box<dyn Error>> {
    const BLOCKS: u32 = 20_000;
    const FRAMES: usize = 128;
    let (mut to_realtime, mut from_reader) = queue::channel::<Block>(8)?;
    let (mut to_reader, mut from_realtime) = queue::channel::<Block>(8)?;
    let mut pool = BlockPool::new(Layout::Stereo, FRAMES, 8)?;

    let reader = thread::spawn(move || -> Result<(), audio_buffers::Error> {
        let mut sent = 0;
        while sent < BLOCKS {
            while let Some(block) = from_realtime.try_pop() {
                // The pool was made with room for every block it handed out.
                let _ = pool.give(block);
            }
            let Some(mut block) = pool.take() else {
                spin_loop();
                continue;
            };
            block.set_frames(FRAMES - (sent as usize % 4))?;
            if let Some((left, right)) = block.stereo_mut() {
                left.fill(sent as f32);
                right.fill(-(sent as f32));
            }
            // The queue is as large as the pool, so it is never full here.
            let _ = to_realtime.try_push(block);
            sent += 1;
        }
        Ok(())
    });

    let mut problem = None;
    assert_realtime(|| {
        let mut received = 0;
        while received < BLOCKS {
            let Some(block) = from_reader.try_pop() else {
                spin_loop();
                continue;
            };
            let value = received as f32;
            let frames_ok = block.frames() == FRAMES - (received as usize % 4);
            let left_ok = block
                .channel(0)
                .is_some_and(|l| l.iter().all(|&s| s == value));
            let right_ok = block
                .channel(1)
                .is_some_and(|r| r.iter().all(|&s| s == -value));
            if !(frames_ok && left_ok && right_ok) && problem.is_none() {
                problem = Some(received);
            }
            let mut block = block;
            while let Err(back) = to_reader.try_push(block) {
                block = back;
                spin_loop();
            }
            received += 1;
        }
    });
    reader.join().map_err(|_| "reader panicked")??;
    match problem {
        Some(index) => Err(format!("block {index} arrived with the wrong contents").into()),
        None => Ok(()),
    }
}
