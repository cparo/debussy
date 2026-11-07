//! The test doubles behave as the traits promise, and a callback runs under the real-time guard.

use std::error::Error;
use std::path::Path;
use std::time::Duration;

use platform_interfaces::test_doubles::{FakeCodecs, FakeDevices, FakeFile};
use platform_interfaces::{
    AudioCallback, AudioDevices, AudioStream, Buffers, CodecFormat, Codecs, DeviceId,
    EncodeSettings, Error as PlatformError, FlacDepth, SourceInfo, StreamConfig,
};
use realtime_guard::{GuardAllocator, assert_realtime};

#[global_allocator]
static ALLOCATOR: GuardAllocator = GuardAllocator;

fn config() -> StreamConfig {
    StreamConfig {
        device: DeviceId("fake".into()),
        sample_rate: 48_000,
        buffer_frames: 128,
        input_channels: 2,
        output_channels: 2,
    }
}

/// Copies input to output with the channels swapped and halved, under the real-time guard.
struct SwapHalf;

impl AudioCallback for SwapHalf {
    fn process(&mut self, buffers: Buffers<'_>) {
        assert_realtime(|| {
            for (out, inp) in buffers
                .output
                .chunks_exact_mut(buffers.output_channels)
                .zip(buffers.input.chunks_exact(buffers.input_channels))
            {
                out[0] = inp[1] * 0.5;
                out[1] = inp[0] * 0.5;
            }
        });
    }
}

fn ramp(frame: u64, channel: usize) -> f32 {
    if channel == 0 {
        (frame % 1000) as f32
    } else {
        -((frame % 1000) as f32)
    }
}

#[test]
fn test_fake_device_runs_callback_under_the_guard() -> Result<(), Box<dyn Error>> {
    let devices = FakeDevices::new().with_input(ramp);
    let listed = devices.devices()?;
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, DeviceId("fake".into()));

    let stream = devices.open_fake(&config(), Box::new(SwapHalf))?;
    assert!(stream.wait_for_callbacks(10, Duration::from_secs(10)));
    let output = stream.captured_output();
    assert!(output.len() >= 10 * 128 * 2);
    for (frame, pair) in output.chunks_exact(2).take(1280).enumerate() {
        let v = (frame % 1000) as f32;
        assert_eq!(pair, [-v * 0.5, v * 0.5], "frame {frame}");
    }
    assert_eq!(stream.latency().input_frames, 64);
    Ok(())
}

#[test]
fn test_fake_device_rejects_bad_configs() {
    let devices = FakeDevices::new();
    let bad = [
        StreamConfig {
            device: DeviceId("other".into()),
            ..config()
        },
        StreamConfig {
            sample_rate: 22_050,
            ..config()
        },
        StreamConfig {
            buffer_frames: 4096,
            ..config()
        },
        StreamConfig {
            output_channels: 8,
            ..config()
        },
    ];
    for config in bad {
        assert!(
            devices.open(&config, Box::new(SwapHalf)).is_err(),
            "{config:?}"
        );
    }
}

#[test]
fn test_unplugging_stops_the_stream_with_an_error() -> Result<(), Box<dyn Error>> {
    let devices = FakeDevices::new();
    let mut stream = devices.open(&config(), Box::new(SwapHalf))?;
    devices.unplug();
    let mut error = None;
    for _ in 0..10_000 {
        error = stream.take_error();
        if error.is_some() {
            break;
        }
        std::thread::yield_now();
    }
    assert!(
        matches!(error, Some(PlatformError::StreamStopped(_))),
        "{error:?}"
    );
    assert!(stream.take_error().is_none(), "reported once");
    assert!(devices.devices()?.is_empty());
    assert!(matches!(
        devices.open(&config(), Box::new(SwapHalf)),
        Err(PlatformError::DeviceNotFound(_))
    ));
    Ok(())
}

#[test]
fn test_fake_codecs_round_trip() -> Result<(), Box<dyn Error>> {
    let codecs = FakeCodecs::new();
    let info = SourceInfo {
        format: CodecFormat::Mp3,
        sample_rate: 44_100,
        channels: 2,
        frames: Some(3),
    };
    codecs.add_file(
        "in.mp3",
        FakeFile {
            info,
            samples: vec![1.0, -1.0, 2.0, -2.0, 3.0, -3.0],
        },
    );

    let mut decoder = codecs.decode(Path::new("in.mp3"))?;
    assert_eq!(decoder.info(), info);
    let mut buffer = [0.0_f32; 5]; // room for 2 whole frames
    assert_eq!(decoder.read(&mut buffer)?, 2);
    assert_eq!(buffer[..4], [1.0, -1.0, 2.0, -2.0]);
    assert_eq!(decoder.read(&mut buffer)?, 1);
    assert_eq!(decoder.read(&mut buffer)?, 0);

    assert!(!codecs.can_encode(CodecFormat::Mp3));
    let settings = EncodeSettings::Flac {
        depth: FlacDepth::Int24,
    };
    let mut encoder = codecs.encode(Path::new("out.flac"), settings, 48_000, 1)?;
    encoder.write(&[0.5, 0.25])?;
    encoder.write(&[0.125])?;
    encoder.finish()?;
    let out = codecs.file(Path::new("out.flac")).ok_or("not encoded")?;
    assert_eq!(out.samples, [0.5, 0.25, 0.125]);
    assert_eq!(out.info.frames, Some(3));
    assert_eq!(out.info.format, CodecFormat::Flac);

    assert!(codecs.decode(Path::new("missing.flac")).is_err());
    Ok(())
}
