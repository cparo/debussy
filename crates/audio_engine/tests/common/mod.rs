//! Test doubles shared by the engine tests: in-memory stored audio, a simulated disk stream, a
//! latency node, and a small graph.

#![allow(dead_code)] // Each test binary uses a different part.

use std::error::Error;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

use audio_buffers::Layout;
use audio_engine::{ChannelClip, ClipAudio, Streams, TrackFeed, TrackRenderer};
use signal_graph::{Config, Graph, Inputs, Node, NodeId, Outputs, Port, Runner};

pub const RATE: u32 = 48_000;

/// Stored files held in memory. Reading allocates nothing.
#[derive(Clone)]
pub struct MemoryAudio(pub Arc<Vec<Vec<f32>>>);

impl ClipAudio for MemoryAudio {
    fn read(
        &mut self,
        file: usize,
        frame: u64,
        out: &mut [f32],
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let samples = self.0.get(file).ok_or("no such file")?;
        let start = frame as usize;
        let source = samples
            .get(start..start + out.len())
            .ok_or("past the end")?;
        out.copy_from_slice(source);
        Ok(())
    }
}

/// A reproducible signal in roughly [-1, 1].
pub fn noise(n: usize, seed: u64) -> Vec<f32> {
    let mut noise = dsp::Noise::new(seed);
    (0..n).map(|_| 2.0 * noise.uniform()).collect()
}

/// A clip at unity gain with no fades.
pub fn clip(file: usize, offset: u64, length: u64, position: u64) -> ChannelClip {
    ChannelClip {
        file,
        offset,
        length,
        position,
        gain: 1.0,
        fade_in: 0,
        fade_out: 0,
    }
}

/// The whole timeline of `clips`, `frames` long, rendered in one call.
pub fn rendered(
    clips: &[ChannelClip],
    audio: &MemoryAudio,
    frames: usize,
) -> Result<Vec<f32>, Box<dyn Error>> {
    let mut renderer = TrackRenderer::new(clips.to_vec(), RATE)?;
    let mut out = vec![0.0; frames];
    renderer.render(0, &mut out, &mut audio.clone())?;
    Ok(out)
}

/// Shared by every simulated stream: where the last seek went, and whether reads are ready.
#[derive(Default)]
pub struct StreamControl {
    target: AtomicU64,
    generation: AtomicU32,
    pub ready: AtomicBool,
}

/// The test's handle on the simulated streams.
pub struct TestStreams(pub Arc<StreamControl>);

impl Streams for TestStreams {
    fn seek(&mut self, frame: u64) -> Result<(), Box<dyn Error + Send + Sync>> {
        self.0.ready.store(false, Ordering::SeqCst);
        self.0.target.store(frame, Ordering::SeqCst);
        self.0.generation.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

/// A simulated disk stream of a timeline rendered ahead of time: not primed until the test says
/// so after each seek, then plays from the seek position.
pub struct StreamFeed {
    timeline: Arc<Vec<f32>>,
    control: Arc<StreamControl>,
    generation: u32,
    position: usize,
}

impl StreamFeed {
    pub fn new(timeline: Vec<f32>, control: &Arc<StreamControl>) -> Self {
        StreamFeed {
            timeline: Arc::new(timeline),
            control: Arc::clone(control),
            generation: 0,
            position: 0,
        }
    }
}

impl TrackFeed for StreamFeed {
    fn read(&mut self, out: &mut [f32]) -> usize {
        let generation = self.control.generation.load(Ordering::SeqCst);
        if generation != self.generation {
            self.generation = generation;
            self.position = self.control.target.load(Ordering::SeqCst) as usize;
        }
        let available = self
            .timeline
            .len()
            .saturating_sub(self.position)
            .min(out.len());
        out[..available].copy_from_slice(&self.timeline[self.position..][..available]);
        out[available..].fill(0.0);
        self.position += out.len();
        available
    }

    fn is_primed(&self, _frames: u64) -> bool {
        self.control.ready.load(Ordering::SeqCst)
    }
}

/// A mono node that delays its input by a fixed number of frames and reports it as latency.
pub struct Latency {
    frames: usize,
    line: Vec<f32>,
    at: usize,
}

impl Latency {
    pub fn new(frames: usize) -> Self {
        Latency {
            frames,
            line: Vec::new(),
            at: 0,
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
        self.frames
    }

    fn prepare(&mut self, _sample_rate: u32, _max_frames: usize) {
        self.line = vec![0.0; self.frames.max(1)];
        self.at = 0;
    }

    fn process(&mut self, inputs: &Inputs<'_>, outputs: &mut Outputs<'_>) {
        let frames = outputs.frames();
        let input = inputs.channel(0, 0).unwrap_or(&[]);
        let Some(output) = outputs.channel_mut(0, 0) else {
            return;
        };
        for (i, out) in output.iter_mut().take(frames).enumerate() {
            let x = input.get(i).copied().unwrap_or(0.0);
            if self.frames == 0 {
                *out = x;
            } else {
                *out = self.line[self.at];
                self.line[self.at] = x;
                self.at = (self.at + 1) % self.frames;
            }
        }
    }
}

/// A graph of one stereo track straight to the stereo output and one mono track through a
/// latency node of `latency` frames, panned centre into it.
pub struct TestGraph {
    pub graph: Graph,
    pub runner: Runner,
    pub stereo: NodeId,
    pub mono: NodeId,
    pub output: NodeId,
}

pub fn graph(latency: usize) -> Result<TestGraph, Box<dyn Error>> {
    let (mut graph, runner) = Graph::new(Config::new(RATE, 512))?;
    let stereo = graph.add_source(Layout::Stereo);
    let mono = graph.add_source(Layout::Mono);
    let delay = graph.add_node(Box::new(Latency::new(latency)));
    let output = graph.add_sink(Layout::Stereo);
    graph.connect(stereo, 0, output, 0)?;
    graph.connect(mono, 0, delay, 0)?;
    graph.connect(delay, 0, output, 0)?;
    graph.commit()?;
    Ok(TestGraph {
        graph,
        runner,
        stereo,
        mono,
        output,
    })
}
