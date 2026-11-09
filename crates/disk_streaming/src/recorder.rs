//! Recording to disk: the control side, [`Recorder`], and its writer thread, which appends each
//! take channel's queued blocks to a new WAV file (`docs/architecture/recording.md` "Writer
//! thread").
//!
//! This is not real-time code: it creates, writes and syncs files, sleeps when idle, and starts
//! and joins its thread. The crate's `clippy.toml` bans those operations for the real-time side,
//! which is in `take`, so this module allows them.
#![allow(clippy::disallowed_methods, clippy::disallowed_types)]

use std::fs::File;
use std::io::BufWriter;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;

use audio_buffers::{Block, BlockPool, Consumer, Layout, Producer, queue};
use wav_files::{Channels, SampleFormat, Spec, WavWriter};

use crate::error::Error;
use crate::reader::BLOCK_FRAMES;
use crate::take::{Stamped, Take, TakeShared};

/// How long the writer sleeps when no take has a block waiting.
const IDLE: Duration = Duration::from_millis(2);

/// Silence for filling gaps.
const SILENCE: [f32; BLOCK_FRAMES] = [0.0; BLOCK_FRAMES];

/// Settings for a [`Recorder`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecorderConfig {
    /// The project sample rate, which the files are written at.
    pub sample_rate: u32,
    /// How much of each take can wait in memory for the disk: 2 s by default (ADR-0078).
    pub buffer: Duration,
}

impl RecorderConfig {
    /// The default buffer of 2 s at `sample_rate`.
    pub fn new(sample_rate: u32) -> Self {
        Self {
            sample_rate,
            buffer: Duration::from_secs(2),
        }
    }
}

/// What the writer thread reports about a take, once per take.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TakeEvent {
    /// The take's file is complete.
    Finished {
        /// The take's index.
        take: usize,
        /// Frames in the file, dropped ones written as silence.
        frames: u64,
    },
    /// Writing the take's file failed; it is incomplete.
    Failed {
        /// The take's index.
        take: usize,
        /// What went wrong.
        message: String,
    },
}

/// The writer's state for one take channel.
struct WriterTake {
    file: Option<WavWriter<BufWriter<File>>>,
    shared: Arc<TakeShared>,
    blocks: Consumer<Stamped>,
    returns: Producer<Block>,
    /// Frames in the file so far.
    written: u64,
    error: Option<String>,
    done: bool,
}

impl WriterTake {
    /// Appends a block, first filling any gap before it with silence, and hands it back.
    fn append(&mut self, stamped: Stamped) {
        if stamped.start > self.written {
            self.silence(stamped.start - self.written);
        }
        let samples = stamped.block.channel(0).unwrap_or(&[]);
        self.write(samples);
        // The return queue holds every block of the take.
        let _ = self.returns.try_push(stamped.block);
    }

    /// Appends `frames` frames of silence.
    fn silence(&mut self, mut frames: u64) {
        while frames > 0 {
            let n = frames.min(BLOCK_FRAMES as u64) as usize;
            self.write(&SILENCE[..n]);
            frames -= n as u64;
        }
    }

    fn write(&mut self, samples: &[f32]) {
        // After a failure nothing more is written, but frames are still counted.
        if let Some(file) = &mut self.file
            && let Err(error) = file.write(&[samples])
        {
            self.fail(error.to_string());
        }
        self.written += samples.len() as u64;
    }

    fn fail(&mut self, message: String) {
        self.file = None;
        self.error.get_or_insert(message);
    }

    /// Pads the take to `frames`, finishes the file and reports it.
    fn finish(&mut self, index: usize, frames: u64) -> TakeEvent {
        self.done = true;
        if frames > self.written {
            self.silence(frames - self.written);
        }
        if let Some(file) = self.file.take() {
            let result = file
                .finish()
                .map_err(|e| e.to_string())
                .and_then(|w| w.into_inner().map_err(|e| e.error().to_string()))
                .and_then(|f| f.sync_all().map_err(|e| e.to_string()));
            if let Err(message) = result {
                self.fail(message);
            }
        }
        match self.error.take() {
            Some(message) => TakeEvent::Failed {
                take: index,
                message,
            },
            None => TakeEvent::Finished {
                take: index,
                frames: self.written,
            },
        }
    }
}

/// The writer thread's main loop: returns once every take has finished, or when told to stop,
/// after finishing every file with what has arrived.
fn run(mut takes: Vec<WriterTake>, mut events: Producer<TakeEvent>, stop: Arc<AtomicBool>) {
    loop {
        let stopping = stop.load(Ordering::Acquire);
        let mut idle = true;
        for (index, take) in takes.iter_mut().enumerate().filter(|(_, t)| !t.done) {
            // Read before draining: every block queued before the take ended is then drained.
            let ended = take.shared.ended.load(Ordering::Acquire);
            while let Some(stamped) = take.blocks.try_pop() {
                take.append(stamped);
                idle = false;
            }
            if ended || stopping {
                let frames = if ended {
                    take.shared.frames.load(Ordering::Relaxed)
                } else {
                    take.written
                };
                // The queue holds an event for every take.
                let _ = events.try_push(take.finish(index, frames));
            }
        }
        if takes.iter().all(|t| t.done) {
            return;
        }
        if idle {
            std::thread::sleep(IDLE);
        }
    }
}

/// The control side of recording one pass. Dropping it finishes every file with what has
/// arrived and stops the writer thread.
#[derive(Debug)]
pub struct Recorder {
    shared: Vec<Arc<TakeShared>>,
    events: Consumer<TakeEvent>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Recorder {
    /// Creates a new mono 32-bit float WAV file at each of `paths`, one per take channel, and
    /// starts the writer thread. Returns the control side and one [`Take`] per path, in order,
    /// for the real-time thread.
    ///
    /// # Errors
    ///
    /// [`Error::Io`] if a file exists or cannot be created (any already created are removed),
    /// [`Error::Wav`], [`Error::Buffers`], or [`Error::Thread`].
    pub fn start(paths: &[PathBuf], config: RecorderConfig) -> Result<(Self, Vec<Take>), Error> {
        let frames = (config.buffer.as_secs_f64() * f64::from(config.sample_rate)).ceil() as u64;
        // Enough blocks for the buffer, plus one being filled and one being written.
        let blocks = usize::try_from(frames.div_ceil(BLOCK_FRAMES as u64))
            .unwrap_or(usize::MAX / 2)
            .saturating_add(2);
        let spec = Spec {
            channels: Channels::Mono,
            sample_rate: config.sample_rate,
            format: SampleFormat::Float32,
        };
        let mut created: Vec<&PathBuf> = Vec::with_capacity(paths.len());
        let result = (|| -> Result<_, Error> {
            let mut shared = Vec::with_capacity(paths.len());
            let mut writers = Vec::with_capacity(paths.len());
            let mut takes = Vec::with_capacity(paths.len());
            for path in paths {
                let file = File::create_new(path)?;
                created.push(path);
                let file = WavWriter::new(BufWriter::new(file), spec)?;
                let take_shared = Arc::new(TakeShared::default());
                let (blocks_tx, blocks_rx) = queue::channel(blocks)?;
                let (returns_tx, returns_rx) = queue::channel(blocks)?;
                writers.push(WriterTake {
                    file: Some(file),
                    shared: Arc::clone(&take_shared),
                    blocks: blocks_rx,
                    returns: returns_tx,
                    written: 0,
                    error: None,
                    done: false,
                });
                let pool = BlockPool::new(Layout::Mono, BLOCK_FRAMES, blocks)?;
                takes.push(Take::new(
                    Arc::clone(&take_shared),
                    pool,
                    blocks_tx,
                    returns_rx,
                ));
                shared.push(take_shared);
            }
            let (events_tx, events) = queue::channel(paths.len().max(1))?;
            let stop = Arc::new(AtomicBool::new(false));
            let thread_stop = Arc::clone(&stop);
            let thread = std::thread::Builder::new()
                .name("disk-recording".into())
                .spawn(move || run(writers, events_tx, thread_stop))
                .map_err(Error::Thread)?;
            let recorder = Recorder {
                shared,
                events,
                stop,
                thread: Some(thread),
            };
            Ok((recorder, takes))
        })();
        if result.is_err() {
            for path in created {
                let _ = std::fs::remove_file(path);
            }
        }
        result
    }

    /// The number of take channels.
    pub fn takes(&self) -> usize {
        self.shared.len()
    }

    /// Overruns counted on take `take` so far, for the UI's notice; `None` for an unknown take.
    pub fn overruns(&self, take: usize) -> Option<u64> {
        self.shared
            .get(take)
            .map(|s| s.overruns.load(Ordering::Relaxed))
    }

    /// The next event from the writer thread, if any.
    pub fn poll_event(&mut self) -> Option<TakeEvent> {
        self.events.try_pop()
    }
}

impl Drop for Recorder {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
