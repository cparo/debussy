//! The audio device: the default output device running the engine's [`Processor`] each buffer
//! (`docs/architecture/platform.md` "Audio device I/O").

use anyhow::{Context, Result, bail};
use audio_engine::Processor;
use platform_interfaces::{AudioCallback, AudioDevices, AudioStream, Buffers, StreamConfig};

use crate::project::MAX_BLOCK;

/// The buffer size asked of the device, in frames: the engine's default (ADR-0007) or the
/// nearest the device allows.
const PREFERRED_BUFFER: u32 = 128;

/// Runs the engine on the device's real-time thread. Allocates nothing per buffer.
struct EngineCallback {
    processor: Processor,
    left: Vec<f32>,
    right: Vec<f32>,
}

impl AudioCallback for EngineCallback {
    fn process(&mut self, buffers: Buffers<'_>) {
        let channels = buffers.output_channels;
        let mut done = 0;
        while done < buffers.frames {
            let n = (buffers.frames - done).min(self.left.len());
            let (left, right) = (&mut self.left[..n], &mut self.right[..n]);
            // A failed block is silence, which `process` has already written.
            let _ = self.processor.process(left, right);
            let out = &mut buffers.output[done * channels..(done + n) * channels];
            for (frame, (l, r)) in out
                .chunks_exact_mut(channels.max(1))
                .zip(left.iter().zip(right.iter()))
            {
                frame.fill(0.0);
                if let Some(first) = frame.first_mut() {
                    *first = *l;
                }
                if let Some(second) = frame.get_mut(1) {
                    *second = *r;
                }
            }
            done += n;
        }
    }
}

/// Opens the default output device at `sample_rate` and runs `processor` on it.
///
/// # Errors
///
/// If there is no default output, it cannot run at the project rate (conversion at the device
/// is not built yet), or it fails to open.
pub fn start(
    devices: &dyn AudioDevices,
    sample_rate: u32,
    processor: Processor,
) -> Result<Box<dyn AudioStream>> {
    let Some(device) = devices
        .devices()?
        .into_iter()
        .find(|d| d.is_default && d.output_channels > 0)
    else {
        bail!("no default audio output device");
    };
    if !device.sample_rates.contains(&sample_rate) {
        bail!(
            "{} cannot run at the project's {sample_rate} Hz (it offers {:?}); conversion at \
             the device is not built yet",
            device.name,
            device.sample_rates
        );
    }
    let (min, max) = device.buffer_frames;
    let buffer = PREFERRED_BUFFER.clamp(min, max.min(MAX_BLOCK as u32));
    let config = StreamConfig {
        device: device.id.clone(),
        sample_rate,
        buffer_frames: buffer,
        input_channels: 0,
        output_channels: device.output_channels.min(2),
    };
    let callback = EngineCallback {
        processor,
        left: vec![0.0; buffer as usize],
        right: vec![0.0; buffer as usize],
    };
    devices
        .open(&config, Box::new(callback))
        .with_context(|| format!("opening {}", device.name))
}
