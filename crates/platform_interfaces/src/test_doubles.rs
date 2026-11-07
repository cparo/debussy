//! In-memory implementations of the platform traits, for other crates' tests (CLAUDE.md
//! "Testing": mock external dependencies with trait objects).
//!
//! - [`FakeDevices`] has one duplex device. A stream runs its callback on its own thread, as fast
//!   as it can, with input from a generator function, and captures the output.
//! - [`FakeCodecs`] decodes files registered with it and keeps what is encoded, all in memory.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::codec::{CodecFormat, Codecs, Decoder, EncodeSettings, Encoder, SourceInfo};
use crate::device::{
    AudioCallback, AudioDevices, AudioStream, Buffers, DeviceId, DeviceInfo, Latency, StreamConfig,
};
use crate::error::Error;

/// Generates the fake device's input: the sample for `(frame, channel)`, counting frames from the
/// stream's start.
pub type InputSignal = fn(u64, usize) -> f32;

fn silence(_: u64, _: usize) -> f32 {
    0.0
}

/// A fake platform with one duplex device, `"fake"`: 2 in, 2 out, at 44.1, 48, 88.2 or 96 kHz,
/// with buffers of 32 to 1024 frames.
#[derive(Debug, Clone)]
pub struct FakeDevices {
    info: DeviceInfo,
    latency: Latency,
    input: InputSignal,
    unplugged: Arc<AtomicBool>,
}

impl Default for FakeDevices {
    fn default() -> Self {
        Self::new()
    }
}

impl FakeDevices {
    /// The fake device, with silent input and 64 frames of latency each way.
    pub fn new() -> Self {
        Self {
            info: DeviceInfo {
                id: DeviceId("fake".into()),
                name: "Fake Interface".into(),
                input_channels: 2,
                output_channels: 2,
                sample_rates: vec![44_100, 48_000, 88_200, 96_000],
                buffer_frames: (32, 1024),
                is_default: true,
            },
            latency: Latency {
                input_frames: 64,
                output_frames: 64,
            },
            input: silence,
            unplugged: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Uses `input` to generate the device's input.
    pub fn with_input(mut self, input: InputSignal) -> Self {
        self.input = input;
        self
    }

    /// Simulates unplugging the device: running streams stop with [`Error::StreamStopped`], and
    /// it disappears from [`AudioDevices::devices`].
    pub fn unplug(&self) {
        self.unplugged.store(true, Ordering::Release);
    }

    /// Like [`AudioDevices::open`], but returns the concrete [`FakeStream`] so a test can inspect
    /// it.
    ///
    /// # Errors
    ///
    /// [`Error::DeviceNotFound`] or [`Error::UnsupportedConfig`].
    pub fn open_fake(
        &self,
        config: &StreamConfig,
        mut callback: Box<dyn AudioCallback>,
    ) -> Result<FakeStream, Error> {
        if config.device != self.info.id || self.unplugged.load(Ordering::Acquire) {
            return Err(Error::DeviceNotFound(config.device.0.clone()));
        }
        let (min, max) = self.info.buffer_frames;
        if !self.info.sample_rates.contains(&config.sample_rate)
            || !(min..=max).contains(&config.buffer_frames)
            || config.input_channels > self.info.input_channels
            || config.output_channels > self.info.output_channels
        {
            return Err(Error::UnsupportedConfig(format!("{config:?}")));
        }
        let shared = Arc::new(FakeShared::default());
        let thread_shared = Arc::clone(&shared);
        let unplugged = Arc::clone(&self.unplugged);
        let input_signal = self.input;
        let frames = config.buffer_frames as usize;
        let (ins, outs) = (
            usize::from(config.input_channels),
            usize::from(config.output_channels),
        );
        let thread = std::thread::Builder::new()
            .name("fake-audio".into())
            .spawn(move || {
                let mut input = vec![0.0_f32; frames * ins];
                let mut output = vec![0.0_f32; frames * outs];
                let mut position = 0_u64;
                while !thread_shared.stop.load(Ordering::Acquire) {
                    if unplugged.load(Ordering::Acquire) {
                        thread_shared
                            .stopped_by_unplug
                            .store(true, Ordering::Release);
                        return;
                    }
                    for (i, sample) in input.iter_mut().enumerate() {
                        *sample = input_signal(position + (i / ins.max(1)) as u64, i % ins.max(1));
                    }
                    output.fill(0.0);
                    callback.process(Buffers {
                        input: &input,
                        output: &mut output,
                        input_channels: ins,
                        output_channels: outs,
                        frames,
                    });
                    thread_shared.capture(&output);
                    position += frames as u64;
                    thread_shared.callbacks.fetch_add(1, Ordering::AcqRel);
                    std::thread::yield_now();
                }
            })
            .map_err(|e| Error::Platform(e.to_string()))?;
        Ok(FakeStream {
            config: config.clone(),
            latency: self.latency,
            shared,
            thread: Some(thread),
            reported: false,
        })
    }
}

impl AudioDevices for FakeDevices {
    fn devices(&self) -> Result<Vec<DeviceInfo>, Error> {
        if self.unplugged.load(Ordering::Acquire) {
            return Ok(Vec::new());
        }
        Ok(vec![self.info.clone()])
    }

    fn open(
        &self,
        config: &StreamConfig,
        callback: Box<dyn AudioCallback>,
    ) -> Result<Box<dyn AudioStream>, Error> {
        Ok(Box::new(self.open_fake(config, callback)?))
    }
}

/// The most output frames a [`FakeStream`] keeps, per channel, at 1 s at 96 kHz.
const CAPTURE_LIMIT: usize = 96_000;

#[derive(Debug, Default)]
struct FakeShared {
    stop: AtomicBool,
    stopped_by_unplug: AtomicBool,
    callbacks: AtomicU64,
    captured: Mutex<Vec<f32>>,
}

impl FakeShared {
    fn capture(&self, output: &[f32]) {
        let mut captured = self.captured.lock().unwrap_or_else(PoisonError::into_inner);
        let room = (CAPTURE_LIMIT * 2).saturating_sub(captured.len());
        captured.extend_from_slice(&output[..output.len().min(room)]);
    }
}

/// A running fake stream. Dropping it stops the thread.
#[derive(Debug)]
pub struct FakeStream {
    config: StreamConfig,
    latency: Latency,
    shared: Arc<FakeShared>,
    thread: Option<JoinHandle<()>>,
    reported: bool,
}

impl FakeStream {
    /// Callbacks run so far.
    pub fn callbacks(&self) -> u64 {
        self.shared.callbacks.load(Ordering::Acquire)
    }

    /// Waits until at least `count` callbacks have run, or `timeout` passes; returns whether they
    /// did.
    pub fn wait_for_callbacks(&self, count: u64, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        while self.callbacks() < count {
            if Instant::now() > deadline {
                return false;
            }
            std::thread::yield_now();
        }
        true
    }

    /// The interleaved output captured so far (up to 1 s at 96 kHz per channel).
    pub fn captured_output(&self) -> Vec<f32> {
        self.shared
            .captured
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

impl AudioStream for FakeStream {
    fn config(&self) -> &StreamConfig {
        &self.config
    }

    fn latency(&self) -> Latency {
        self.latency
    }

    fn take_error(&mut self) -> Option<Error> {
        if !self.reported && self.shared.stopped_by_unplug.load(Ordering::Acquire) {
            self.reported = true;
            return Some(Error::StreamStopped("the device was unplugged".into()));
        }
        None
    }
}

impl Drop for FakeStream {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// An in-memory audio file for [`FakeCodecs`].
#[derive(Debug, Clone, PartialEq)]
pub struct FakeFile {
    /// What it holds.
    pub info: SourceInfo,
    /// Its samples, interleaved.
    pub samples: Vec<f32>,
}

/// Fake codecs: decode files registered with [`FakeCodecs::add_file`], and keep encoded files for
/// [`FakeCodecs::file`]. Clones share the same files.
#[derive(Debug, Clone, Default)]
pub struct FakeCodecs {
    files: Arc<Mutex<HashMap<PathBuf, FakeFile>>>,
}

impl FakeCodecs {
    /// No files.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a file to decode at `path`.
    pub fn add_file(&self, path: impl Into<PathBuf>, file: FakeFile) {
        self.lock().insert(path.into(), file);
    }

    /// The file at `path`: registered or encoded.
    pub fn file(&self, path: &Path) -> Option<FakeFile> {
        self.lock().get(path).cloned()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<PathBuf, FakeFile>> {
        self.files.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl Codecs for FakeCodecs {
    fn can_decode(&self, _format: CodecFormat) -> bool {
        true
    }

    fn can_encode(&self, format: CodecFormat) -> bool {
        format != CodecFormat::Mp3
    }

    fn decode(&self, path: &Path) -> Result<Box<dyn Decoder>, Error> {
        let file = self.file(path).ok_or_else(|| {
            Error::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                path.display().to_string(),
            ))
        })?;
        Ok(Box::new(FakeDecoder { file, position: 0 }))
    }

    fn encode(
        &self,
        path: &Path,
        settings: EncodeSettings,
        sample_rate: u32,
        channels: u16,
    ) -> Result<Box<dyn Encoder>, Error> {
        Ok(Box::new(FakeEncoder {
            codecs: self.clone(),
            path: path.to_path_buf(),
            info: SourceInfo {
                format: settings.format(),
                sample_rate,
                channels,
                frames: None,
            },
            samples: Vec::new(),
        }))
    }
}

struct FakeDecoder {
    file: FakeFile,
    position: usize,
}

impl Decoder for FakeDecoder {
    fn info(&self) -> SourceInfo {
        self.file.info
    }

    fn read(&mut self, interleaved: &mut [f32]) -> Result<usize, Error> {
        let channels = usize::from(self.file.info.channels.max(1));
        let whole = interleaved.len() / channels * channels;
        let rest = &self.file.samples[self.position.min(self.file.samples.len())..];
        let n = whole.min(rest.len() / channels * channels);
        interleaved[..n].copy_from_slice(&rest[..n]);
        self.position += n;
        Ok(n / channels)
    }
}

struct FakeEncoder {
    codecs: FakeCodecs,
    path: PathBuf,
    info: SourceInfo,
    samples: Vec<f32>,
}

impl Encoder for FakeEncoder {
    fn write(&mut self, interleaved: &[f32]) -> Result<(), Error> {
        self.samples.extend_from_slice(interleaved);
        Ok(())
    }

    fn finish(self: Box<Self>) -> Result<(), Error> {
        let channels = u64::from(self.info.channels.max(1));
        let info = SourceInfo {
            frames: Some(self.samples.len() as u64 / channels),
            ..self.info
        };
        self.codecs.add_file(
            self.path,
            FakeFile {
                info,
                samples: self.samples,
            },
        );
        Ok(())
    }
}
