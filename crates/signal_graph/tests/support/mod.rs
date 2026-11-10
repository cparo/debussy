//! Test nodes and a render helper shared by the integration tests.

#![allow(dead_code)] // each test binary uses a different part of this module

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use audio_buffers::Layout;
use realtime_guard::assert_realtime;
use signal_graph::{
    Inputs, Node, NodeId, Outputs, Parameter, ParameterInfo, Port, Runner, Smoothed,
};

/// A mono gain with one smoothed parameter.
pub struct Gain {
    parameter: Parameter,
    smoothed: Option<Smoothed>,
}

impl Gain {
    pub fn new(gain: f32) -> Result<Self, signal_graph::Error> {
        let parameter = Parameter::new(ParameterInfo::new("gain", 0.0, 4.0, gain))?;
        Ok(Gain {
            parameter,
            smoothed: None,
        })
    }
}

impl Node for Gain {
    fn inputs(&self) -> Vec<Port> {
        vec![Port::audio("in", Layout::Mono)]
    }
    fn outputs(&self) -> Vec<Port> {
        vec![Port::audio("out", Layout::Mono)]
    }
    fn parameters(&self) -> Vec<Parameter> {
        vec![self.parameter.clone()]
    }
    fn prepare(&mut self, sample_rate: u32, _max_frames: usize) {
        self.smoothed = Some(Smoothed::new(self.parameter.clone(), sample_rate));
    }
    fn process(&mut self, inputs: &Inputs<'_>, outputs: &mut Outputs<'_>) {
        let (Some(smoothed), Some(input), Some(output)) = (
            self.smoothed.as_mut(),
            inputs.channel(0, 0),
            outputs.channel_mut(0, 0),
        ) else {
            return;
        };
        smoothed.update();
        for (out, &sample) in output.iter_mut().zip(input) {
            *out = sample * smoothed.next_value();
        }
    }
}

/// A mono node that delays its input by `latency` frames and reports it.
pub struct Latency {
    ring: Vec<f32>,
    position: usize,
}

impl Latency {
    pub fn new(latency: usize) -> Self {
        Latency {
            ring: vec![0.0; latency],
            position: 0,
        }
    }
}

impl Node for Latency {
    fn inputs(&self) -> Vec<Port> {
        vec![Port::audio("in", Layout::Mono)]
    }
    fn outputs(&self) -> Vec<Port> {
        vec![Port::audio("out", Layout::Mono)]
    }
    fn latency(&self) -> usize {
        self.ring.len()
    }
    fn process(&mut self, inputs: &Inputs<'_>, outputs: &mut Outputs<'_>) {
        let (Some(input), Some(output)) = (inputs.channel(0, 0), outputs.channel_mut(0, 0)) else {
            return;
        };
        for (out, &sample) in output.iter_mut().zip(input) {
            *out = std::mem::replace(&mut self.ring[self.position], sample);
            self.position = (self.position + 1) % self.ring.len();
        }
    }
}

/// A mono source that counts samples, 0, 1, 2…, and sets a flag when dropped.
pub struct Counter {
    count: f32,
    dropped: Arc<AtomicBool>,
}

impl Counter {
    pub fn new() -> (Self, Arc<AtomicBool>) {
        let dropped = Arc::new(AtomicBool::new(false));
        let counter = Counter {
            count: 0.0,
            dropped: Arc::clone(&dropped),
        };
        (counter, dropped)
    }
}

impl Drop for Counter {
    fn drop(&mut self) {
        self.dropped.store(true, Ordering::SeqCst);
    }
}

impl Node for Counter {
    fn inputs(&self) -> Vec<Port> {
        Vec::new()
    }
    fn outputs(&self) -> Vec<Port> {
        vec![Port::audio("out", Layout::Mono)]
    }
    fn process(&mut self, _inputs: &Inputs<'_>, outputs: &mut Outputs<'_>) {
        if let Some(output) = outputs.channel_mut(0, 0) {
            for out in output {
                *out = self.count;
                self.count += 1.0;
            }
        }
    }
}

/// Runs `input` through the runner from mono `source` (if any) to mono `sink`, in blocks of
/// `block` frames, each block inside the real-time guard.
pub fn render(
    runner: &mut Runner,
    source: Option<NodeId>,
    sink: NodeId,
    input: &[f32],
    block: usize,
) -> Result<Vec<f32>, &'static str> {
    let mut output = vec![0.0; input.len()];
    for (chunk, out) in input.chunks(block).zip(output.chunks_mut(block)) {
        assert_realtime(|| run_block(runner, source, sink, chunk, out))?;
    }
    Ok(output)
}

/// One block of [`render`].
fn run_block(
    runner: &mut Runner,
    source: Option<NodeId>,
    sink: NodeId,
    input: &[f32],
    output: &mut [f32],
) -> Result<(), &'static str> {
    runner
        .begin_block(input.len())
        .map_err(|_| "block refused")?;
    if let Some(source) = source {
        let block = runner.source_mut(source).ok_or("no source")?;
        block
            .channel_mut(0)
            .ok_or("no channel")?
            .copy_from_slice(input);
    }
    runner.process();
    let block = runner.sink(sink).ok_or("no sink")?;
    output.copy_from_slice(block.channel(0).ok_or("no channel")?);
    Ok(())
}

/// An impulse at frame 0 followed by silence.
pub fn impulse(frames: usize) -> Vec<f32> {
    let mut signal = vec![0.0; frames];
    signal[0] = 1.0;
    signal
}
