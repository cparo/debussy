//! Recording to disk: the control side, [`Recorder`], and its writer thread, which appends each
//! take channel's queued blocks to a new WAV file, and collects each MIDI take's messages while
//! appending them to its journal (`docs/architecture/recording.md` "Writer thread", "MIDI
//! recording").
//!
//! This is not real-time code: it creates, writes and syncs files, sleeps when idle, and starts
//! and joins its thread. The crate's `clippy.toml` bans those operations for the real-time side,
//! which is in `take`, so this module allows them.
#![allow(clippy::disallowed_methods, clippy::disallowed_types)]

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use audio_buffers::{Block, BlockPool, Consumer, Layout, Producer, queue};
use midi::{journal_header, journal_record};
use wav_files::{Channels, SampleFormat, Spec, WavWriter};

use crate::error::Error;
use crate::midi_take::{CapturedMessage, MIDI_TAKE_CAPACITY, MidiTake, MidiTakeShared};
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
    /// How often the writer syncs each file while writing it: 1 s by default.
    pub sync_interval: Duration,
}

impl RecorderConfig {
    /// The default buffer of 2 s and sync interval of 1 s at `sample_rate`.
    pub fn new(sample_rate: u32) -> Self {
        Self {
            sample_rate,
            buffer: Duration::from_secs(2),
            sync_interval: Duration::from_secs(1),
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
    /// When the file was last synced, and how often to.
    synced: Instant,
    sync_interval: Duration,
    /// Whether frames have been written since the last sync.
    unsynced: bool,
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
        self.unsynced = true;
    }

    /// Flushes and syncs the file if frames are waiting and the interval has passed, so a crash
    /// loses at most about that much of the take (`docs/architecture/recording.md` "Writer
    /// thread"). A failure fails the take.
    fn sync_if_due(&mut self) {
        if !self.unsynced || self.synced.elapsed() < self.sync_interval {
            return;
        }
        self.unsynced = false;
        self.synced = Instant::now();
        if let Some(file) = &mut self.file {
            let buffered = file.get_mut();
            if let Err(error) = buffered
                .flush()
                .and_then(|()| buffered.get_ref().sync_data())
            {
                self.fail(error.to_string());
            }
        }
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

/// The writer's state for one MIDI take.
struct WriterMidi {
    shared: Arc<MidiTakeShared>,
    messages: Consumer<CapturedMessage>,
    /// The messages so far, in arrival order.
    collected: Vec<CapturedMessage>,
    /// The take's crash journal; dropped after a failure, the take going on in memory.
    journal: Option<BufWriter<File>>,
    /// When the journal was last synced, and how often to.
    synced: Instant,
    sync_interval: Duration,
    /// Whether records have been appended since the last sync.
    unsynced: bool,
    done: bool,
}

impl WriterMidi {
    /// Takes every message queued, appending each to the journal. Returns whether there were
    /// any.
    fn drain(&mut self) -> bool {
        let before = self.collected.len();
        while let Some((frame, bytes)) = self.messages.try_pop() {
            self.collected.push((frame, bytes));
            if let Some(journal) = &mut self.journal
                && journal.write_all(&journal_record(frame, bytes)).is_err()
            {
                self.journal = None;
            }
            self.unsynced = true;
        }
        self.collected.len() > before
    }

    /// Flushes and syncs the journal if records are waiting and the interval has passed, or at
    /// once if `now`, so a crash loses at most about that much of the take.
    fn sync(&mut self, now: bool) {
        if !self.unsynced || (!now && self.synced.elapsed() < self.sync_interval) {
            return;
        }
        self.unsynced = false;
        self.synced = Instant::now();
        if let Some(journal) = &mut self.journal
            && journal
                .flush()
                .and_then(|()| journal.get_ref().sync_data())
                .is_err()
        {
            self.journal = None;
        }
    }
}

/// Where the writer thread reports to.
struct Reports {
    events: Producer<TakeEvent>,
    /// Each finished MIDI take's messages, by take index; sent before its event.
    notes: Producer<(usize, Vec<CapturedMessage>)>,
}

/// The writer thread's main loop: returns once every take has finished, or when told to stop,
/// after finishing every file with what has arrived. MIDI takes are numbered after the audio
/// ones.
fn run(
    mut takes: Vec<WriterTake>,
    mut midi: Vec<WriterMidi>,
    mut reports: Reports,
    stop: Arc<AtomicBool>,
) {
    let events = &mut reports.events;
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
            take.sync_if_due();
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
        for (i, take) in midi.iter_mut().enumerate().filter(|(_, t)| !t.done) {
            let ended = take.shared.ended.load(Ordering::Acquire);
            idle &= !take.drain();
            take.sync(ended || stopping);
            if ended || stopping {
                take.done = true;
                take.journal = None;
                let frames = if ended {
                    take.shared.frames.load(Ordering::Relaxed)
                } else {
                    take.collected.iter().map(|(f, _)| f + 1).max().unwrap_or(0)
                };
                let index = takes.len() + i;
                // Both queues hold one entry for every take.
                let _ = reports
                    .notes
                    .try_push((index, std::mem::take(&mut take.collected)));
                let _ = events.try_push(TakeEvent::Finished {
                    take: index,
                    frames,
                });
            }
        }
        if takes.iter().all(|t| t.done) && midi.iter().all(|t| t.done) {
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
    midi: Vec<Arc<MidiTakeShared>>,
    events: Consumer<TakeEvent>,
    notes: Consumer<(usize, Vec<CapturedMessage>)>,
    /// Finished MIDI takes' messages not yet taken, by take index.
    collected: BTreeMap<usize, Vec<CapturedMessage>>,
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
        Recorder::start_with_midi(paths, &[], config).map(|(recorder, takes, _)| (recorder, takes))
    }

    /// As [`Recorder::start`], with a MIDI take as well for each of `journals`, numbered after
    /// the audio ones. The writer creates a crash journal at each path (ADR-0151), appends each
    /// message to it and syncs it as often as the WAV files, and collects the messages, which
    /// [`Recorder::take_messages`] returns once its [`TakeEvent`] has come. A journal that fails
    /// to write is abandoned; the take goes on in memory. Returns a [`MidiTake`] per MIDI take,
    /// in order, for the real-time thread.
    ///
    /// # Errors
    ///
    /// As [`Recorder::start`].
    pub fn start_with_midi(
        paths: &[PathBuf],
        journals: &[PathBuf],
        config: RecorderConfig,
    ) -> Result<(Self, Vec<Take>, Vec<MidiTake>), Error> {
        let midi = journals.len();
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
        let mut created: Vec<&PathBuf> = Vec::with_capacity(paths.len() + midi);
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
                    synced: Instant::now(),
                    sync_interval: config.sync_interval,
                    unsynced: false,
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
            let mut midi_shared = Vec::with_capacity(midi);
            let mut midi_writers = Vec::with_capacity(midi);
            let mut midi_takes = Vec::with_capacity(midi);
            for path in journals {
                let file = File::create_new(path)?;
                created.push(path);
                let mut journal = BufWriter::new(file);
                journal.write_all(&journal_header(config.sample_rate))?;
                let take_shared = Arc::new(MidiTakeShared::default());
                let (tx, rx) = queue::channel(MIDI_TAKE_CAPACITY)?;
                midi_writers.push(WriterMidi {
                    shared: Arc::clone(&take_shared),
                    messages: rx,
                    collected: Vec::new(),
                    journal: Some(journal),
                    synced: Instant::now(),
                    sync_interval: config.sync_interval,
                    // The header is synced with the first records.
                    unsynced: true,
                    done: false,
                });
                midi_takes.push(MidiTake::new(Arc::clone(&take_shared), tx));
                midi_shared.push(take_shared);
            }
            let (events_tx, events) = queue::channel((paths.len() + midi).max(1))?;
            let (notes_tx, notes) = queue::channel(midi.max(1))?;
            let reports = Reports {
                events: events_tx,
                notes: notes_tx,
            };
            let stop = Arc::new(AtomicBool::new(false));
            let thread_stop = Arc::clone(&stop);
            let thread = std::thread::Builder::new()
                .name("disk-recording".into())
                .spawn(move || run(writers, midi_writers, reports, thread_stop))
                .map_err(Error::Thread)?;
            let recorder = Recorder {
                shared,
                midi: midi_shared,
                events,
                notes,
                collected: BTreeMap::new(),
                stop,
                thread: Some(thread),
            };
            Ok((recorder, takes, midi_takes))
        })();
        if result.is_err() {
            for path in created {
                let _ = std::fs::remove_file(path);
            }
        }
        result
    }

    /// The number of takes: audio take channels, then MIDI takes.
    pub fn takes(&self) -> usize {
        self.shared.len() + self.midi.len()
    }

    /// Overruns counted on take `take` so far, for the UI's notice; `None` for an unknown take.
    pub fn overruns(&self, take: usize) -> Option<u64> {
        match take.checked_sub(self.shared.len()) {
            None => self.shared.get(take).map(|s| &s.overruns),
            Some(midi) => self.midi.get(midi).map(|s| &s.overruns),
        }
        .map(|overruns| overruns.load(Ordering::Relaxed))
    }

    /// MIDI take `take`'s messages in the order they arrived, once its [`TakeEvent`] has been
    /// polled; `None` before, for an audio take, or once taken.
    pub fn take_messages(&mut self, take: usize) -> Option<Vec<CapturedMessage>> {
        while let Some((index, messages)) = self.notes.try_pop() {
            self.collected.insert(index, messages);
        }
        self.collected.remove(&take)
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
