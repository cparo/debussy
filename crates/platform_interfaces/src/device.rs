//! Audio devices: enumeration and a duplex stream on one device (`docs/architecture/platform.md`
//! "Audio device I/O").

use crate::error::Error;

/// A device's stable identifier, as the platform names it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DeviceId(pub String);

/// What a device offers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceInfo {
    /// Its identifier.
    pub id: DeviceId,
    /// Its name, for display.
    pub name: String,
    /// Input channels; 0 for an output-only device.
    pub input_channels: u16,
    /// Output channels; 0 for an input-only device.
    pub output_channels: u16,
    /// Sample rates it can run at, in hertz, ascending.
    pub sample_rates: Vec<u32>,
    /// The smallest and largest buffer sizes it accepts, in frames.
    pub buffer_frames: (u32, u32),
    /// Whether it is the system's default device.
    pub is_default: bool,
}

/// How to open a stream. Input and output are on the same device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamConfig {
    /// The device.
    pub device: DeviceId,
    /// Frames per second.
    pub sample_rate: u32,
    /// Frames per callback.
    pub buffer_frames: u32,
    /// Input channels to open; 0 for none.
    pub input_channels: u16,
    /// Output channels to open; 0 for none.
    pub output_channels: u16,
}

/// The latencies a device reports, for recording alignment (ADR-0022).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Latency {
    /// From the input jack to the callback's input buffer, in frames.
    pub input_frames: u32,
    /// From the callback's output buffer to the output jack, in frames.
    pub output_frames: u32,
}

/// One callback's audio, interleaved.
#[derive(Debug)]
pub struct Buffers<'a> {
    /// `frames × input_channels` samples, interleaved; empty without input.
    pub input: &'a [f32],
    /// `frames × output_channels` samples, interleaved, to be filled.
    pub output: &'a mut [f32],
    /// Input channels per frame.
    pub input_channels: usize,
    /// Output channels per frame.
    pub output_channels: usize,
    /// Frames in this callback; at most the configured buffer size.
    pub frames: usize,
}

/// Code run on the platform's real-time audio thread once per buffer. It must not allocate,
/// lock, block, do I/O or print (`docs/architecture/engine.md` "Real-time thread").
pub trait AudioCallback: Send + 'static {
    /// Reads `buffers.input` and fills `buffers.output`.
    fn process(&mut self, buffers: Buffers<'_>);
}

/// A running stream. Dropping it stops the stream and drops its callback off the real-time
/// thread.
pub trait AudioStream: Send {
    /// The configuration it runs with.
    fn config(&self) -> &StreamConfig;
    /// The device's reported latencies.
    fn latency(&self) -> Latency;
    /// An error that has stopped the stream, if one has; [`Error::StreamStopped`] when its
    /// device disappears.
    fn take_error(&mut self) -> Option<Error>;
}

/// The platform's audio devices.
pub trait AudioDevices: Send + Sync {
    /// The devices present now.
    ///
    /// # Errors
    ///
    /// [`Error::Platform`] if the platform cannot list them.
    fn devices(&self) -> Result<Vec<DeviceInfo>, Error>;

    /// Opens and starts a duplex stream that calls `callback` on the real-time thread.
    ///
    /// # Errors
    ///
    /// [`Error::DeviceNotFound`], [`Error::UnsupportedConfig`], or [`Error::Platform`].
    fn open(
        &self,
        config: &StreamConfig,
        callback: Box<dyn AudioCallback>,
    ) -> Result<Box<dyn AudioStream>, Error>;
}
