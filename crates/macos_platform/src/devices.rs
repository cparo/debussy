//! [`MacDevices`]: audio devices through `cpal`, behind `platform_interfaces`
//! (`docs/architecture/platform.md` "Audio device I/O", ADR-0009, ADR-0050, ADR-0052).
//!
//! `cpal` opens input and output as separate streams. A duplex stream here is both, on the same
//! device and so the same clock: the input callback pushes samples into a [`SampleRing`] and the
//! output callback pops them and runs the [`AudioCallback`] with both. Input-only and
//! output-only streams run the callback directly. Callbacks allocate, lock and block nothing.

use std::sync::Arc;
use std::sync::atomic::{AtomicU8, AtomicU32, Ordering};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{BufferSize, ErrorKind, SampleFormat, SupportedBufferSize, SupportedStreamConfigRange};
use platform_interfaces::{
    AudioCallback, AudioDevices, AudioStream, Buffers, DeviceId, DeviceInfo, Error, Latency,
    StreamConfig,
};

use crate::ring::SampleRing;

/// Standard sample rates offered when a device's ranges include them, in hertz.
const STANDARD_RATES: [u32; 13] = [
    8_000, 11_025, 16_000, 22_050, 32_000, 44_100, 48_000, 88_200, 96_000, 176_400, 192_000,
    352_800, 384_000,
];

/// Buffers of input the ring holds between the input and output callbacks.
const RING_BUFFERS: usize = 4;

/// The macOS audio devices.
#[derive(Debug, Default, Clone, Copy)]
pub struct MacDevices;

impl MacDevices {
    /// The devices of the system's default audio host (CoreAudio).
    pub fn new() -> Self {
        MacDevices
    }
}

/// One supported configuration range, reduced to what [`describe`] needs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Range {
    pub channels: u16,
    pub min_rate: u32,
    pub max_rate: u32,
    pub buffer: Option<(u32, u32)>,
}

impl Range {
    fn of(range: &SupportedStreamConfigRange) -> Option<Range> {
        (range.sample_format() == SampleFormat::F32).then(|| Range {
            channels: range.channels(),
            min_rate: range.min_sample_rate(),
            max_rate: range.max_sample_rate(),
            buffer: match range.buffer_size() {
                SupportedBufferSize::Range { min, max } => Some((*min, *max)),
                SupportedBufferSize::Unknown => None,
            },
        })
    }
}

/// A device's info from its `f32` input and output ranges. Buffer sizes are the widest the
/// ranges state, or `(0, u32::MAX)` if none states one.
pub(crate) fn describe(
    id: String,
    name: String,
    inputs: &[Range],
    outputs: &[Range],
    is_default: bool,
) -> DeviceInfo {
    let all = || inputs.iter().chain(outputs);
    let sample_rates = STANDARD_RATES
        .into_iter()
        .filter(|&rate| all().any(|r| r.min_rate <= rate && rate <= r.max_rate))
        .collect();
    let buffers: Vec<(u32, u32)> = all().filter_map(|r| r.buffer).collect();
    let buffer_frames = if buffers.is_empty() {
        (0, u32::MAX)
    } else {
        (
            buffers.iter().map(|b| b.0).min().unwrap_or(0),
            buffers.iter().map(|b| b.1).max().unwrap_or(u32::MAX),
        )
    };
    DeviceInfo {
        id: DeviceId(id),
        name,
        input_channels: inputs.iter().map(|r| r.channels).max().unwrap_or(0),
        output_channels: outputs.iter().map(|r| r.channels).max().unwrap_or(0),
        sample_rates,
        buffer_frames,
        is_default,
    }
}

fn platform(error: &cpal::Error) -> Error {
    Error::Platform(error.to_string())
}

fn open_error(error: &cpal::Error, device: &str) -> Error {
    match error.kind() {
        ErrorKind::DeviceNotAvailable => Error::DeviceNotFound(device.to_owned()),
        ErrorKind::UnsupportedConfig | ErrorKind::InvalidInput => {
            Error::UnsupportedConfig(error.to_string())
        }
        _ => platform(error),
    }
}

/// What the callbacks report to the stream handle, without locks.
#[derive(Debug, Default)]
struct Shared {
    /// 0: running; 1: the device went away; 2: another stream error.
    error: AtomicU8,
    /// Capture to callback, in frames.
    input_latency: AtomicU32,
    /// Callback to playback, in frames.
    output_latency: AtomicU32,
    /// Input frames waiting in the ring after the last output callback.
    ring_frames: AtomicU32,
}

impl Shared {
    /// Records an error that stops the stream. Glitches the stream survives are not errors here:
    /// an overload or underrun (`Xrun`, which CoreAudio reports whenever the machine falls
    /// behind) and a default-device change (which only streams following the default device
    /// see).
    fn on_error(&self, error: &cpal::Error) {
        let code = match error.kind() {
            ErrorKind::Xrun | ErrorKind::DeviceChanged => return,
            ErrorKind::DeviceNotAvailable => 1,
            _ => 2,
        };
        self.error.store(code, Ordering::Release);
    }
}

/// Frames in `duration` at `rate`, saturating.
fn frames(duration: std::time::Duration, rate: u32) -> u32 {
    (duration.as_secs_f64() * f64::from(rate))
        .round()
        .min(f64::from(u32::MAX)) as u32
}

/// A running stream: one or two cpal streams and what their callbacks report.
struct MacStream {
    config: StreamConfig,
    shared: Arc<Shared>,
    // Held to keep the streams running; dropping them stops the streams and drops the callbacks.
    _input: Option<cpal::Stream>,
    _output: Option<cpal::Stream>,
}

impl AudioStream for MacStream {
    fn config(&self) -> &StreamConfig {
        &self.config
    }

    fn latency(&self) -> Latency {
        Latency {
            input_frames: self
                .shared
                .input_latency
                .load(Ordering::Acquire)
                .saturating_add(self.shared.ring_frames.load(Ordering::Acquire)),
            output_frames: self.shared.output_latency.load(Ordering::Acquire),
        }
    }

    fn take_error(&mut self) -> Option<Error> {
        match self.shared.error.swap(0, Ordering::AcqRel) {
            0 => None,
            1 => Some(Error::StreamStopped(
                "the device was disconnected".to_owned(),
            )),
            _ => Some(Error::StreamStopped(
                "the device reported an error".to_owned(),
            )),
        }
    }
}

impl AudioDevices for MacDevices {
    fn devices(&self) -> Result<Vec<DeviceInfo>, Error> {
        let host = cpal::default_host();
        let default_ids = [
            host.default_input_device().and_then(|d| d.id().ok()),
            host.default_output_device().and_then(|d| d.id().ok()),
        ];
        let mut infos = Vec::new();
        for device in host.devices().map_err(|e| platform(&e))? {
            // A device that vanished while listing is skipped.
            let Ok(id) = device.id() else {
                continue;
            };
            let name = device
                .description()
                .map(|d| d.name().to_owned())
                .unwrap_or_else(|_| id.to_string());
            let ranges = |configs: Result<Vec<SupportedStreamConfigRange>, cpal::Error>| {
                configs
                    .unwrap_or_default()
                    .iter()
                    .filter_map(Range::of)
                    .collect::<Vec<_>>()
            };
            let inputs = ranges(device.supported_input_configs().map(Iterator::collect));
            let outputs = ranges(device.supported_output_configs().map(Iterator::collect));
            let is_default = default_ids.iter().flatten().any(|d| *d == id);
            infos.push(describe(
                id.to_string(),
                name,
                &inputs,
                &outputs,
                is_default,
            ));
        }
        Ok(infos)
    }

    fn open(
        &self,
        config: &StreamConfig,
        mut callback: Box<dyn AudioCallback>,
    ) -> Result<Box<dyn AudioStream>, Error> {
        let in_channels = usize::from(config.input_channels);
        let out_channels = usize::from(config.output_channels);
        if in_channels == 0 && out_channels == 0 {
            return Err(Error::UnsupportedConfig(
                "a stream with no channels".to_owned(),
            ));
        }
        if config.buffer_frames == 0 {
            return Err(Error::UnsupportedConfig("a buffer of 0 frames".to_owned()));
        }
        let host = cpal::default_host();
        let device = host
            .devices()
            .map_err(|e| platform(&e))?
            .find(|d| d.id().is_ok_and(|id| id.to_string() == config.device.0))
            .ok_or_else(|| Error::DeviceNotFound(config.device.0.clone()))?;
        let stream_config = |channels: u16| cpal::StreamConfig {
            channels,
            sample_rate: config.sample_rate,
            buffer_size: BufferSize::Fixed(config.buffer_frames),
        };
        let buffer = config.buffer_frames as usize;
        let rate = config.sample_rate;
        let shared = Arc::new(Shared::default());
        let device_name = config.device.0.as_str();
        let error_callback = |shared: &Arc<Shared>| {
            let shared = Arc::clone(shared);
            move |error: cpal::Error| shared.on_error(&error)
        };

        let (input, output) = if out_channels == 0 {
            // Input only: run the callback from the input stream.
            let mut no_output: [f32; 0] = [];
            let reporter = Arc::clone(&shared);
            let input = device
                .build_input_stream::<f32, _, _>(
                    stream_config(config.input_channels),
                    move |data: &[f32], info: &cpal::InputCallbackInfo| {
                        let t = info.timestamp();
                        reporter.input_latency.store(
                            frames(t.callback.duration_since(t.capture), rate),
                            Ordering::Release,
                        );
                        for chunk in data.chunks(buffer * in_channels) {
                            callback.process(Buffers {
                                input: chunk,
                                output: &mut no_output,
                                input_channels: in_channels,
                                output_channels: 0,
                                frames: chunk.len() / in_channels,
                            });
                        }
                    },
                    error_callback(&shared),
                    None,
                )
                .map_err(|e| open_error(&e, device_name))?;
            (Some(input), None)
        } else {
            let ring = (in_channels > 0)
                .then(|| Arc::new(SampleRing::new(RING_BUFFERS * buffer * in_channels)));
            let input = match &ring {
                Some(ring) => {
                    let ring = Arc::clone(ring);
                    let reporter = Arc::clone(&shared);
                    Some(
                        device
                            .build_input_stream::<f32, _, _>(
                                stream_config(config.input_channels),
                                move |data: &[f32], info: &cpal::InputCallbackInfo| {
                                    let t = info.timestamp();
                                    reporter.input_latency.store(
                                        frames(t.callback.duration_since(t.capture), rate),
                                        Ordering::Release,
                                    );
                                    // Input the output side has not taken in time is dropped.
                                    ring.push(data);
                                },
                                error_callback(&shared),
                                None,
                            )
                            .map_err(|e| open_error(&e, device_name))?,
                    )
                }
                None => None,
            };
            let mut staging = vec![0.0f32; buffer * in_channels];
            let reporter = Arc::clone(&shared);
            let output = device
                .build_output_stream::<f32, _, _>(
                    stream_config(config.output_channels),
                    move |data: &mut [f32], info: &cpal::OutputCallbackInfo| {
                        let t = info.timestamp();
                        reporter.output_latency.store(
                            frames(t.playback.duration_since(t.callback), rate),
                            Ordering::Release,
                        );
                        for chunk in data.chunks_mut(buffer * out_channels) {
                            let n = chunk.len() / out_channels;
                            let input = &mut staging[..n * in_channels];
                            if let Some(ring) = &ring {
                                let got = ring.pop(input);
                                input[got..].fill(0.0);
                            }
                            callback.process(Buffers {
                                input,
                                output: chunk,
                                input_channels: in_channels,
                                output_channels: out_channels,
                                frames: n,
                            });
                        }
                        if let Some(ring) = &ring {
                            let waiting = ring.len() / in_channels.max(1);
                            reporter
                                .ring_frames
                                .store(waiting as u32, Ordering::Release);
                        }
                    },
                    error_callback(&shared),
                    None,
                )
                .map_err(|e| open_error(&e, device_name))?;
            (input, Some(output))
        };
        for stream in input.iter().chain(output.iter()) {
            stream.play().map_err(|e| open_error(&e, device_name))?;
        }
        Ok(Box::new(MacStream {
            config: config.clone(),
            shared,
            _input: input,
            _output: output,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn range(channels: u16, min_rate: u32, max_rate: u32, buffer: Option<(u32, u32)>) -> Range {
        Range {
            channels,
            min_rate,
            max_rate,
            buffer,
        }
    }

    #[test]
    fn test_describe_merges_input_and_output_ranges() {
        let info = describe(
            "CoreAudio:x".into(),
            "Interface".into(),
            &[range(2, 44_100, 48_000, Some((32, 512)))],
            &[
                range(8, 96_000, 96_000, Some((64, 4096))),
                range(2, 44_100, 44_100, None),
            ],
            true,
        );
        assert_eq!(info.input_channels, 2);
        assert_eq!(info.output_channels, 8);
        assert_eq!(info.sample_rates, vec![44_100, 48_000, 96_000]);
        assert_eq!(info.buffer_frames, (32, 4096));
        assert!(info.is_default);
    }

    #[test]
    fn test_on_error_ignores_glitches_the_stream_survives() {
        let shared = Shared::default();
        shared.on_error(&ErrorKind::Xrun.into());
        shared.on_error(&ErrorKind::DeviceChanged.into());
        assert_eq!(shared.error.load(Ordering::Acquire), 0);
        shared.on_error(&ErrorKind::StreamInvalidated.into());
        assert_eq!(shared.error.load(Ordering::Acquire), 2);
        shared.on_error(&ErrorKind::DeviceNotAvailable.into());
        assert_eq!(shared.error.load(Ordering::Acquire), 1);
    }

    #[test]
    fn test_describe_without_buffer_ranges_states_none() {
        let info = describe(
            "a".into(),
            "b".into(),
            &[],
            &[range(2, 48_000, 48_000, None)],
            false,
        );
        assert_eq!(info.buffer_frames, (0, u32::MAX));
        assert_eq!(info.input_channels, 0);
    }
}
