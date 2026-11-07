//! The reader thread: keeps each stream's queue filled to the read-ahead target, serving the
//! stream with the least audio buffered first (earliest deadline), and follows seeks.
//!
//! This thread is not real-time: it reads files and sleeps when idle. The crate's `clippy.toml`
//! bans those operations for the real-time side, so this module allows them.
#![allow(clippy::disallowed_methods, clippy::disallowed_types)]

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use audio_buffers::{BlockPool, Consumer, Producer};

use crate::shared::{Chunk, ENDED, StreamShared, pack};
use crate::source::FrameSource;
use crate::streamer::{Command, Event, Plan};

/// Frames per block read from disk.
pub(crate) const BLOCK_FRAMES: usize = audio_buffers::MAX_FRAMES;
/// How long the reader sleeps when every stream is full.
const IDLE: Duration = Duration::from_millis(2);

/// The reader's state for one stream.
pub(crate) struct ReaderStream {
    pub(crate) source: Box<dyn FrameSource>,
    pub(crate) shared: Arc<StreamShared>,
    pub(crate) pool: BlockPool,
    pub(crate) chunks: Producer<Chunk>,
    pub(crate) returns: Consumer<Chunk>,
    /// The generation being read, and its plan.
    pub(crate) generation: u16,
    pub(crate) plan: Plan,
    /// The next source frame to read.
    pub(crate) position: u64,
    /// Where the source is positioned, if known; seeking only when this differs keeps the
    /// file's read buffer.
    pub(crate) source_position: Option<u64>,
    /// Frames queued in this generation.
    pub(crate) queued: u64,
    /// Whether this generation's plan has been read to its end, or reading failed.
    pub(crate) done: bool,
}

impl ReaderStream {
    /// Starts `generation` at `plan`.
    pub(crate) fn restart(&mut self, generation: u16, plan: Plan) {
        self.generation = generation;
        self.plan = plan;
        self.position = plan.start;
        self.queued = 0;
        self.done = false;
        self.shared
            .queued
            .store(pack(generation, 0), Ordering::Release);
    }

    /// Takes back the blocks the real-time side has finished with.
    fn reclaim(&mut self) {
        while let Some(chunk) = self.returns.try_pop() {
            // The pool was made with room for all of the stream's blocks.
            let _ = self.pool.give(chunk.block);
        }
    }

    /// Frames buffered ahead of the real-time side in this generation.
    fn buffered(&self) -> u64 {
        let (generation, played) =
            crate::shared::unpack(self.shared.played.load(Ordering::Acquire));
        let played = if generation == self.generation {
            played
        } else {
            0
        };
        self.queued.saturating_sub(played)
    }

    /// Reads and queues one block. Returns an error message if the source failed.
    fn read_block(&mut self) -> Result<(), String> {
        let end = self
            .plan
            .loop_region
            .map_or(self.source.frames(), |(_, end)| end);
        if self.position >= end {
            match self.plan.loop_region {
                Some((start, _)) => self.position = start,
                None => {
                    self.finish();
                    return Ok(());
                }
            }
        }
        let Some(mut block) = self.pool.take() else {
            return Ok(());
        };
        let wanted =
            usize::try_from(end - self.position).map_or(BLOCK_FRAMES, |r| r.min(BLOCK_FRAMES));
        let position = self.position;
        let seek = if self.source_position == Some(position) {
            Ok(())
        } else {
            self.source.seek(position)
        };
        self.source_position = None;
        let result = seek.and_then(|()| {
            // The block was made with BLOCK_FRAMES of capacity, and `wanted` is at most that.
            let _ = block.set_frames(wanted);
            let out = block.channel_mut(0).unwrap_or(&mut []);
            self.source.read(out)
        });
        let read = match result {
            Ok(read) => read,
            Err(error) => {
                let _ = self.pool.give(block);
                self.finish();
                return Err(error.to_string());
            }
        };
        if read == 0 {
            let _ = self.pool.give(block);
            self.finish();
            return Ok(());
        }
        let _ = block.set_frames(read);
        let chunk = Chunk {
            block,
            generation: self.generation,
        };
        if let Err(chunk) = self.chunks.try_push(chunk) {
            // The queue holds as many blocks as the pool, so this does not happen.
            let _ = self.pool.give(chunk.block);
            return Ok(());
        }
        self.position += read as u64;
        self.source_position = Some(self.position);
        self.queued += read as u64;
        self.shared
            .queued
            .store(pack(self.generation, self.queued), Ordering::Release);
        Ok(())
    }

    fn finish(&mut self) {
        self.done = true;
        self.shared.queued.store(
            pack(self.generation, self.queued | ENDED),
            Ordering::Release,
        );
    }
}

/// The reader thread's main loop. Returns when told to stop or when the control side is gone.
pub(crate) fn run(
    mut streams: Vec<ReaderStream>,
    mut commands: Consumer<Command>,
    mut events: Producer<Event>,
    read_ahead: u64,
) {
    loop {
        while let Some(command) = commands.try_pop() {
            match command {
                Command::Seek {
                    stream,
                    generation,
                    plan,
                } => {
                    if let Some(s) = streams.get_mut(stream) {
                        s.restart(generation, plan);
                    }
                }
                Command::Stop => return,
            }
        }
        for stream in &mut streams {
            stream.reclaim();
        }
        // Earliest deadline first: the stream with the least buffered, if under the target.
        let next = streams
            .iter_mut()
            .enumerate()
            .filter(|(_, s)| !s.done && s.buffered() < read_ahead && s.pool.available() > 0)
            .min_by_key(|(_, s)| s.buffered());
        match next {
            Some((index, stream)) => {
                if let Err(message) = stream.read_block() {
                    let _ = events.try_push(Event::ReadFailed {
                        stream: index,
                        message,
                    });
                }
            }
            None => std::thread::sleep(IDLE),
        }
    }
}
