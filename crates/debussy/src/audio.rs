//! The audio device: the engine's [`Processor`] running on the chosen device, and switching
//! devices, or opening its inputs while a track is armed, without rebuilding the engine
//! (`docs/architecture/platform.md` "Audio device I/O", ADR-0059, ADR-0079).
//!
//! The processor reaches a callback over a lock-free queue only once its stream has opened, so a
//! device that fails to open never takes it. A stream whose device has gone never calls back, so
//! its processor cannot be handed back: the layer above builds a new engine and gives this its
//! processor with [`AudioOutput::adopt`] (ADR-0081). To switch, the control side asks the running
//! callback to hand the processor back over a second queue, stops that stream, and opens the
//! new one.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow, bail};
use audio_buffers::{Consumer, Producer, queue};
use audio_engine::{MAX_INPUTS, Processor};
use platform_interfaces::{
    AudioCallback, AudioDevices, AudioStream, Buffers, DeviceId, DeviceInfo, Latency, StreamConfig,
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
        let inputs = buffers.input_channels;
        let mut done = 0;
        while done < buffers.frames {
            let n = (buffers.frames - done).min(self.left.len());
            let (left, right) = (&mut self.left[..n], &mut self.right[..n]);
            // Input the device did not fill is passed as empty, which the engine takes as
            // silence.
            let input = buffers
                .input
                .get(done * inputs..(done + n) * inputs)
                .unwrap_or(&[]);
            // A failed block is silence, which `process` has already written.
            let _ = processor.process(input, inputs, left, right);
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
    /// Why the stream stopped, once it has (its device gone); it never calls back again.
    lost: Option<String>,
    /// Whether `lost` has been reported through [`AudioOutput::take_error`].
    reported: bool,
}

/// The engine's real-time side on an audio device.
pub struct AudioOutput {
    devices: Box<dyn AudioDevices>,
    sample_rate: u32,
    running: Option<Running>,
    /// The processor while no stream runs it.
    parked: Option<Processor>,
    /// Whether to open the device's inputs: while a track is armed.
    input: bool,
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
            input: false,
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
        if self.is_lost() {
            bail!("the audio engine was lost with its device; it must be rebuilt");
        }
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

    /// Opens the device's inputs as well as its outputs (`true`), or its outputs only, by
    /// reopening it. Does nothing if that is already so. Call only while the transport is
    /// stopped: the output drops out briefly.
    ///
    /// # Errors
    ///
    /// As [`AudioOutput::switch`]; the setting is kept for the next device opened.
    pub fn set_input(&mut self, input: bool) -> Result<()> {
        if self.input == input {
            return Ok(());
        }
        self.input = input;
        match self.current().map(str::to_owned) {
            Some(id) => self.switch(&id),
            None => Ok(()),
        }
    }

    /// The input channels open on the running stream; 0 when output only.
    pub fn input_channels(&self) -> u16 {
        self.running
            .as_ref()
            .map_or(0, |r| r.stream.config().input_channels)
    }

    /// The running stream's input and output latency, in frames; 0 when none runs.
    pub fn latency(&self) -> u64 {
        let Latency {
            input_frames,
            output_frames,
        } = self
            .running
            .as_ref()
            .map(|r| r.stream.latency())
            .unwrap_or_default();
        u64::from(input_frames) + u64::from(output_frames)
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
            input_channels: if self.input {
                // The engine reads at most this many; MAX_INPUTS is 64, so it fits.
                device.input_channels.min(MAX_INPUTS as u16)
            } else {
                0
            },
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
                    lost: None,
                    reported: false,
                });
                Ok(())
            }
            Err(error) => Err(anyhow!("opening {}: {error}", device.name)),
        }
    }

    /// Whether the running stream has stopped with an error, such as its device unplugged.
    /// Reports each stop once.
    pub fn take_error(&mut self) -> Option<String> {
        self.poll();
        let running = self.running.as_mut()?;
        if running.reported {
            return None;
        }
        running.reported = true;
        running.lost.clone()
    }

    /// Whether the running stream has stopped, taking the processor with it; it can run again
    /// only through [`AudioOutput::adopt`].
    pub fn is_lost(&mut self) -> bool {
        self.poll();
        self.running.as_ref().is_some_and(|r| r.lost.is_some())
    }

    /// Drops the running stream, with whatever processor it holds, and starts `processor` on
    /// the device with id `preferred`, or the default output device if there is none such.
    ///
    /// # Errors
    ///
    /// As [`AudioOutput::start`]; the processor is kept for a later [`AudioOutput::switch`].
    pub fn adopt(&mut self, processor: Processor, preferred: Option<&str>) -> Result<()> {
        // A lost stream never calls back, so this does not wait on the real-time thread.
        self.running = None;
        self.parked = Some(processor);
        let id = self.choose(preferred)?;
        self.open(&id)
    }

    /// Records an error that has stopped the running stream.
    fn poll(&mut self) {
        if let Some(running) = &mut self.running
            && running.lost.is_none()
            && let Some(error) = running.stream.take_error()
        {
            running.lost = Some(error.to_string());
        }
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error;

    use audio_buffers::Layout;
    use audio_engine::Engine;
    use platform_interfaces::test_doubles::FakeDevices;
    use signal_graph::{Config, Graph};

    use super::*;

    /// A processor for an empty project.
    fn processor() -> Result<Processor, Box<dyn Error>> {
        let (mut graph, runner) = Graph::new(Config::new(48_000, 1024))?;
        let output = graph.add_sink(Layout::Stereo);
        graph.commit()?;
        // The graph only plans; the runner keeps what it needs.
        drop(graph);
        let (_engine, processor) = Engine::new(runner, output, 48_000)?;
        Ok(processor)
    }

    #[test]
    fn test_unplugged_device_is_lost_and_reported_once() -> Result<(), Box<dyn Error>> {
        let devices = FakeDevices::new();
        let (mut audio, started) =
            AudioOutput::start(Box::new(devices.clone()), 48_000, processor()?, None);
        started.map_err(|e| e.to_string())?;
        assert!(!audio.is_lost());
        devices.unplug();
        let deadline = Instant::now() + Duration::from_secs(5);
        while !audio.is_lost() {
            if Instant::now() > deadline {
                return Err("the unplug was never seen".into());
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(audio.take_error().is_some());
        assert_eq!(audio.take_error(), None);
        // Switching does not wait for a handback that cannot come.
        let started = Instant::now();
        assert!(audio.switch("fake").is_err());
        assert!(started.elapsed() < HANDBACK_WAIT);
        // A rebuilt engine is kept for later when no device can take it.
        assert!(audio.adopt(processor()?, None).is_err());
        assert!(audio.parked.is_some() && audio.current().is_none());
        Ok(())
    }
}
