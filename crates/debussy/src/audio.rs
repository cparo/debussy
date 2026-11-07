//! The audio device: the engine's [`Processor`] running on the chosen device, and switching
//! devices without rebuilding the engine (`docs/architecture/platform.md` "Audio device I/O",
//! ADR-0059).
//!
//! The processor reaches a callback over a lock-free queue only once its stream has opened, so a
//! device that fails to open never takes it. To switch, the control side asks the running
//! callback to hand the processor back over a second queue, stops that stream, and opens the
//! new one.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow, bail};
use audio_buffers::{Consumer, Producer, queue};
use audio_engine::Processor;
use platform_interfaces::{
    AudioCallback, AudioDevices, AudioStream, Buffers, DeviceId, DeviceInfo, StreamConfig,
};

use crate::project::MAX_BLOCK;

/// The buffer size asked of the device, in frames: the engine's default (ADR-0007) or the
/// nearest the device allows.
const PREFERRED_BUFFER: u32 = 128;

/// How long to wait for a running callback to hand the processor back.
const HANDBACK_WAIT: Duration = Duration::from_secs(2);

/// Runs the engine on the device's real-time thread. Allocates nothing per buffer.
struct EngineCallback {
    processor: Option<Processor>,
    incoming: Consumer<Processor>,
    release: Arc<AtomicBool>,
    back: Producer<Processor>,
    left: Vec<f32>,
    right: Vec<f32>,
}

impl AudioCallback for EngineCallback {
    fn process(&mut self, buffers: Buffers<'_>) {
        // Collect a processor handed over, even if a release is already asked for, so the
        // release can hand it straight back.
        if self.processor.is_none() {
            self.processor = self.incoming.try_pop();
        }
        if self.release.load(Ordering::Acquire)
            && let Some(processor) = self.processor.take()
            && let Err(processor) = self.back.try_push(processor)
        {
            // The queue holds one and is empty; keep it rather than lose it.
            self.processor = Some(processor);
        }
        let Some(processor) = &mut self.processor else {
            buffers.output.fill(0.0);
            return;
        };
        let channels = buffers.output_channels.max(1);
        let mut done = 0;
        while done < buffers.frames {
            let n = (buffers.frames - done).min(self.left.len());
            let (left, right) = (&mut self.left[..n], &mut self.right[..n]);
            // A failed block is silence, which `process` has already written.
            let _ = processor.process(left, right);
            let out = &mut buffers.output[done * channels..(done + n) * channels];
            for (frame, (l, r)) in out
                .chunks_exact_mut(channels)
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

/// A running stream and the way to get its processor back.
struct Running {
    stream: Box<dyn AudioStream>,
    release: Arc<AtomicBool>,
    back: Consumer<Processor>,
    device: String,
}

/// The engine's real-time side on an audio device.
pub struct AudioOutput {
    devices: Box<dyn AudioDevices>,
    sample_rate: u32,
    running: Option<Running>,
    /// The processor while no stream runs it.
    parked: Option<Processor>,
}

impl AudioOutput {
    /// Starts `processor` on the device with id `preferred`, or the default output device if
    /// there is none such.
    ///
    /// # Errors
    ///
    /// If no device can run at `sample_rate`. The processor is kept, so a later
    /// [`AudioOutput::switch`] can start it.
    pub fn start(
        devices: Box<dyn AudioDevices>,
        sample_rate: u32,
        processor: Processor,
        preferred: Option<&str>,
    ) -> (AudioOutput, Result<()>) {
        let mut output = AudioOutput {
            devices,
            sample_rate,
            running: None,
            parked: Some(processor),
        };
        let result = output.choose(preferred).and_then(|id| output.open(&id));
        (output, result)
    }

    /// The devices present.
    pub fn devices(&self) -> Vec<DeviceInfo> {
        self.devices.devices().unwrap_or_default()
    }

    /// The id of the device in use.
    pub fn current(&self) -> Option<&str> {
        self.running.as_ref().map(|r| r.device.as_str())
    }

    /// Moves the engine to the device with id `id`; on failure, back to the previous one.
    ///
    /// # Errors
    ///
    /// If the new device cannot be opened (the old one is reopened), or the running stream
    /// does not hand the processor back.
    pub fn switch(&mut self, id: &str) -> Result<()> {
        let previous = self.current().map(str::to_owned);
        self.stop()?;
        match self.open(id) {
            Ok(()) => Ok(()),
            Err(error) => {
                if let Some(previous) = previous {
                    let _ = self.open(&previous);
                }
                Err(error)
            }
        }
    }

    /// `preferred` if present, else the default output device.
    fn choose(&self, preferred: Option<&str>) -> Result<String> {
        let devices = self.devices.devices()?;
        if let Some(id) = preferred
            && devices
                .iter()
                .any(|d| d.id.0 == id && d.output_channels > 0)
        {
            return Ok(id.to_owned());
        }
        devices
            .into_iter()
            .find(|d| d.is_default && d.output_channels > 0)
            .map(|d| d.id.0)
            .ok_or_else(|| anyhow!("no default audio output device"))
    }

    /// Takes the processor back from the running stream and stops it.
    fn stop(&mut self) -> Result<()> {
        let Some(mut running) = self.running.take() else {
            return Ok(());
        };
        running.release.store(true, Ordering::Release);
        let deadline = Instant::now() + HANDBACK_WAIT;
        loop {
            if let Some(processor) = running.back.try_pop() {
                self.parked = Some(processor);
                return Ok(());
            }
            if Instant::now() > deadline {
                running.release.store(false, Ordering::Release);
                let device = running.device.clone();
                self.running = Some(running);
                bail!("{device} did not respond; it is still in use");
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    /// Opens device `id` with the parked processor.
    fn open(&mut self, id: &str) -> Result<()> {
        let device = self
            .devices
            .devices()?
            .into_iter()
            .find(|d| d.id.0 == id && d.output_channels > 0)
            .ok_or_else(|| anyhow!("that audio device is gone"))?;
        if !device.sample_rates.contains(&self.sample_rate) {
            bail!(
                "{} cannot run at the project's {} Hz; conversion at the device is not built yet",
                device.name,
                self.sample_rate
            );
        }
        if self.parked.is_none() {
            bail!("the engine is not available");
        }
        let (min, max) = device.buffer_frames;
        let buffer = PREFERRED_BUFFER.clamp(min, max.min(MAX_BLOCK as u32));
        let config = StreamConfig {
            device: DeviceId(device.id.0.clone()),
            sample_rate: self.sample_rate,
            buffer_frames: buffer,
            input_channels: 0,
            output_channels: device.output_channels.min(2),
        };
        let (back_tx, back) = queue::channel(1)?;
        let (mut send, incoming) = queue::channel(1)?;
        let release = Arc::new(AtomicBool::new(false));
        let callback = EngineCallback {
            processor: None,
            incoming,
            release: Arc::clone(&release),
            back: back_tx,
            left: vec![0.0; buffer as usize],
            right: vec![0.0; buffer as usize],
        };
        match self.devices.open(&config, Box::new(callback)) {
            Ok(stream) => {
                // Hand the processor over now that the stream runs; the queue is empty.
                if let Some(processor) = self.parked.take()
                    && let Err(processor) = send.try_push(processor)
                {
                    self.parked = Some(processor);
                }
                self.running = Some(Running {
                    stream,
                    release,
                    back,
                    device: device.id.0,
                });
                Ok(())
            }
            Err(error) => Err(anyhow!("opening {}: {error}", device.name)),
        }
    }

    /// Whether the running stream has stopped with an error, such as its device unplugged.
    pub fn take_error(&mut self) -> Option<String> {
        self.running
            .as_mut()
            .and_then(|r| r.stream.take_error())
            .map(|e| e.to_string())
    }
}
