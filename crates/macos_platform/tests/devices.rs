//! Audio devices through cpal. Machines without audio devices (CI runners may have none) skip
//! the streaming test.

use std::error::Error;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use macos_platform::MacDevices;
use platform_interfaces::{
    AudioCallback, AudioDevices, Buffers, DeviceId, Error as PlatformError, StreamConfig,
};

#[test]
fn test_devices_lists_sane_info() -> Result<(), Box<dyn Error>> {
    for info in MacDevices::new().devices()? {
        assert!(!info.id.0.is_empty());
        assert!(
            info.input_channels > 0 || info.output_channels > 0,
            "{info:?}"
        );
        assert!(info.sample_rates.windows(2).all(|w| w[0] < w[1]));
        assert!(info.buffer_frames.0 <= info.buffer_frames.1);
    }
    Ok(())
}

#[test]
fn test_open_unknown_device_is_not_found() {
    let config = StreamConfig {
        device: DeviceId("no such device".into()),
        sample_rate: 48_000,
        buffer_frames: 256,
        input_channels: 0,
        output_channels: 2,
    };
    struct Silent;
    impl AudioCallback for Silent {
        fn process(&mut self, buffers: Buffers<'_>) {
            buffers.output.fill(0.0);
        }
    }
    assert!(matches!(
        MacDevices::new().open(&config, Box::new(Silent)),
        Err(PlatformError::DeviceNotFound(_))
    ));
}

/// Counts callbacks and checks every buffer's shape; outputs silence.
struct Counter {
    calls: Arc<AtomicU64>,
    bad: Arc<AtomicUsize>,
    max_frames: usize,
}

impl AudioCallback for Counter {
    fn process(&mut self, buffers: Buffers<'_>) {
        let ok = buffers.frames <= self.max_frames
            && buffers.output.len() == buffers.frames * buffers.output_channels
            && buffers.input.len() == buffers.frames * buffers.input_channels;
        if !ok {
            self.bad.fetch_add(1, Ordering::Relaxed);
        }
        buffers.output.fill(0.0);
        self.calls.fetch_add(1, Ordering::Relaxed);
    }
}

#[test]
fn test_default_output_streams_silence() -> Result<(), Box<dyn Error>> {
    let devices = MacDevices::new();
    let Some(device) = devices
        .devices()?
        .into_iter()
        .find(|d| d.is_default && d.output_channels >= 2)
    else {
        eprintln!("no default output device; skipping");
        return Ok(());
    };
    let Some(&rate) = device
        .sample_rates
        .iter()
        .find(|&&r| r == 48_000 || r == 44_100)
    else {
        eprintln!("default device offers neither 44.1 nor 48 kHz; skipping");
        return Ok(());
    };
    let buffer = 256.clamp(device.buffer_frames.0, device.buffer_frames.1);
    let calls = Arc::new(AtomicU64::new(0));
    let bad = Arc::new(AtomicUsize::new(0));
    let config = StreamConfig {
        device: device.id.clone(),
        sample_rate: rate,
        buffer_frames: buffer,
        input_channels: 0,
        output_channels: 2,
    };
    let mut stream = devices.open(
        &config,
        Box::new(Counter {
            calls: Arc::clone(&calls),
            bad: Arc::clone(&bad),
            max_frames: buffer as usize,
        }),
    )?;
    let start = Instant::now();
    while calls.load(Ordering::Relaxed) < 20 && start.elapsed() < Duration::from_secs(5) {
        std::thread::yield_now();
    }
    assert!(
        calls.load(Ordering::Relaxed) >= 20,
        "the callback did not run"
    );
    assert_eq!(bad.load(Ordering::Relaxed), 0);
    assert!(stream.take_error().is_none());
    assert_eq!(stream.config(), &config);
    let _ = stream.latency();
    Ok(())
}
