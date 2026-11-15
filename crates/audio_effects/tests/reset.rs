//! Every effect falls silent at once when its plan is reset, as on stop or pause, inside the
//! real-time guard.

use std::error::Error as StdError;

use audio_buffers::Layout;
use audio_effects::{EFFECTS, Preset, effect, presets};
use realtime_guard::{GuardAllocator, assert_realtime};
use signal_graph::{Config, Graph, NodeId, Runner};

#[global_allocator]
static ALLOCATOR: GuardAllocator = GuardAllocator;

type TestResult = Result<(), Box<dyn StdError>>;

const RATE: u32 = 48_000;
const BLOCK: usize = 128;

/// A stereo source through one effect to a stereo sink.
struct Rig {
    _graph: Graph,
    runner: Runner,
    source: NodeId,
    sink: NodeId,
}

impl Rig {
    /// The effect `kind` at its defaults, or at `preset`'s values.
    fn new(kind: &str, preset: Option<&Preset>) -> Result<Self, Box<dyn StdError>> {
        let (mut graph, runner) = Graph::new(Config::new(RATE, BLOCK))?;
        let node = effect(kind).ok_or("no such effect")?;
        if let Some(preset) = preset {
            for parameter in node.parameters() {
                parameter.set(preset.value(parameter.info()));
            }
        }
        let node = graph.add_node(node);
        let source = graph.add_source(Layout::Stereo);
        let sink = graph.add_sink(Layout::Stereo);
        graph.connect(source, 0, node, 0)?;
        graph.connect(node, 0, sink, 0)?;
        graph.commit()?;
        Ok(Rig {
            _graph: graph,
            runner,
            source,
            sink,
        })
    }

    /// Runs `blocks` blocks of `signal(frame)` on both channels and returns the loudest output
    /// sample.
    fn render(
        &mut self,
        blocks: usize,
        signal: impl Fn(usize) -> f32,
    ) -> Result<f32, &'static str> {
        let mut loudest = 0.0_f32;
        for block in 0..blocks {
            loudest = loudest.max(assert_realtime(|| self.block(block * BLOCK, &signal))?);
        }
        Ok(loudest)
    }

    fn block(&mut self, start: usize, signal: &impl Fn(usize) -> f32) -> Result<f32, &'static str> {
        self.runner
            .begin_block(BLOCK)
            .map_err(|_| "block refused")?;
        let source = self.runner.source_mut(self.source).ok_or("no source")?;
        for channel in 0..2 {
            let samples = source.channel_mut(channel).ok_or("no channel")?;
            for (offset, sample) in samples.iter_mut().enumerate() {
                *sample = signal(start + offset);
            }
        }
        self.runner.process();
        let sink = self.runner.sink(self.sink).ok_or("no sink")?;
        let mut loudest = 0.0_f32;
        for channel in 0..2 {
            let samples = sink.channel(channel).ok_or("no channel")?;
            loudest = samples.iter().fold(loudest, |m, s| m.max(s.abs()));
        }
        Ok(loudest)
    }
}

/// A deterministic burst of noise in −0.5..0.5.
fn noise(frame: usize) -> f32 {
    let x = (frame as u32).wrapping_mul(2_654_435_761).rotate_left(13) ^ 0x9E37_79B9;
    (x as f32 / u32::MAX as f32) - 0.5
}

#[test]
fn test_reset_silences_every_effect_at_once() -> TestResult {
    for kind in EFFECTS {
        let configurations = std::iter::once(None).chain(presets(kind.id).map(Some));
        for preset in configurations {
            let label = format!("{} {:?}", kind.id, preset.map(|p| p.name));
            let mut rig = Rig::new(kind.id, preset)?;
            rig.render(40, noise)?;
            rig.runner.reset();
            let tail = rig.render(40, |_| 0.0)?;
            assert!(tail < 1e-6, "{label} still sounds at {tail} after a reset");
        }
    }
    Ok(())
}

#[test]
fn test_without_reset_tails_ring_on() -> TestResult {
    // The case the reset is for: a reverb keeps sounding after its input stops.
    let mut rig = Rig::new("reverb", None)?;
    rig.render(40, noise)?;
    assert!(rig.render(4, |_| 0.0)? > 1e-3);
    Ok(())
}
