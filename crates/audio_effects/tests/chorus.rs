//! The chorus run in a processing graph, every block inside the real-time guard: silence,
//! pass-through at no mix, stereo spread, and a change of voices while playing.

use std::error::Error as StdError;

use audio_buffers::Layout;
use audio_effects::effect;
use realtime_guard::{GuardAllocator, assert_realtime};
use signal_graph::{Config, Graph, NodeId, Parameter, Runner};

#[global_allocator]
static ALLOCATOR: GuardAllocator = GuardAllocator;

type TestResult = Result<(), Box<dyn StdError>>;

const RATE: u32 = 48_000;
const BLOCK: usize = 128;
const KIND: &str = "chorus";

/// A graph of one chorus node between a stereo source and sink.
struct Rig {
    _graph: Graph,
    /// The node's parameters, shared with it.
    parameters: Vec<Parameter>,
    runner: Runner,
    source: NodeId,
    sink: NodeId,
}

impl Rig {
    /// A chorus with the named parameters set before it is prepared.
    fn new(values: &[(&str, f32)]) -> Result<Self, Box<dyn StdError>> {
        let node = effect(KIND).ok_or("unknown kind")?;
        let parameters = node.parameters();
        for parameter in &parameters {
            if let Some(&(_, value)) = values.iter().find(|(n, _)| *n == parameter.info().name) {
                parameter.set(value);
            }
        }
        let (mut graph, runner) = Graph::new(Config::new(RATE, BLOCK))?;
        let node = graph.add_node(node);
        let source = graph.add_source(Layout::Stereo);
        let sink = graph.add_sink(Layout::Stereo);
        graph.connect(source, 0, node, 0)?;
        graph.connect(node, 0, sink, 0)?;
        graph.commit()?;
        Ok(Rig {
            _graph: graph,
            parameters,
            runner,
            source,
            sink,
        })
    }

    /// Sets parameter `name` to `value`, as the UI does while playing.
    fn set(&self, name: &str, value: f32) -> TestResult {
        self.parameters
            .iter()
            .find(|p| p.info().name == name)
            .ok_or("no such parameter")?
            .set(value);
        Ok(())
    }

    /// Runs `left` and `right` (of one length) through the node; returns its two outputs.
    fn render(
        &mut self,
        left: &[f32],
        right: &[f32],
    ) -> Result<(Vec<f32>, Vec<f32>), Box<dyn StdError>> {
        let (mut out_l, mut out_r) = (vec![0.0; left.len()], vec![0.0; left.len()]);
        for (((l, r), ol), or) in left
            .chunks(BLOCK)
            .zip(right.chunks(BLOCK))
            .zip(out_l.chunks_mut(BLOCK))
            .zip(out_r.chunks_mut(BLOCK))
        {
            assert_realtime(|| self.block(l, r, ol, or))?;
        }
        Ok((out_l, out_r))
    }

    fn block(
        &mut self,
        left: &[f32],
        right: &[f32],
        out_l: &mut [f32],
        out_r: &mut [f32],
    ) -> Result<(), &'static str> {
        self.runner
            .begin_block(left.len())
            .map_err(|_| "block refused")?;
        let block = self.runner.source_mut(self.source).ok_or("no source")?;
        block.channel_mut(0).ok_or("no left")?.copy_from_slice(left);
        block
            .channel_mut(1)
            .ok_or("no right")?
            .copy_from_slice(right);
        self.runner.process();
        let block = self.runner.sink(self.sink).ok_or("no sink")?;
        out_l.copy_from_slice(block.channel(0).ok_or("no left")?);
        out_r.copy_from_slice(block.channel(1).ok_or("no right")?);
        Ok(())
    }
}

/// `frames` frames of reproducible noise in −0.5 to 0.5.
fn noise(frames: usize, seed: u64) -> Vec<f32> {
    let mut n = dsp::Noise::new(seed);
    (0..frames).map(|_| n.uniform()).collect()
}

/// The largest magnitude in `signal`.
fn peak(signal: &[f32]) -> f32 {
    signal.iter().fold(0.0, |m, x| m.max(x.abs()))
}

#[test]
fn test_silence_in_gives_silence_out() -> TestResult {
    let mut rig = Rig::new(&[("voices", 4.0), ("depth", 10.0)])?;
    let silence = vec![0.0; RATE as usize];
    let (left, right) = rig.render(&silence, &silence)?;
    assert_eq!(peak(&left), 0.0);
    assert_eq!(peak(&right), 0.0);
    Ok(())
}

#[test]
fn test_no_mix_passes_the_input_through() -> TestResult {
    let mut rig = Rig::new(&[("mix", 0.0)])?;
    let (l, r) = (noise(9_600, 1), noise(9_600, 2));
    let (left, right) = rig.render(&l, &r)?;
    assert_eq!(left, l);
    assert_eq!(right, r);
    Ok(())
}

#[test]
fn test_spread_zero_keeps_a_mono_input_mono() -> TestResult {
    let mono = noise(24_000, 3);
    let mut rig = Rig::new(&[("spread", 0.0), ("voices", 3.0), ("mix", 100.0)])?;
    let (left, right) = rig.render(&mono, &mono)?;
    assert_eq!(left, right);
    let mut rig = Rig::new(&[("spread", 180.0), ("voices", 3.0), ("mix", 100.0)])?;
    let (left, right) = rig.render(&mono, &mono)?;
    let difference = left
        .iter()
        .zip(&right)
        .fold(0.0_f32, |m, (a, b)| m.max((a - b).abs()));
    assert!(difference > 0.05, "{difference}");
    Ok(())
}

#[test]
fn test_wet_level_stays_near_the_input_for_any_voices() -> TestResult {
    // Each voice is the input delayed, so the wet sum × 1/√n has about the input's power.
    let input = noise(48_000, 4);
    let power = |s: &[f32]| s.iter().map(|x| x * x).sum::<f32>() / s.len() as f32;
    for voices in [1.0, 2.0, 4.0] {
        let mut rig = Rig::new(&[("voices", voices), ("mix", 100.0)])?;
        let (left, _) = rig.render(&input, &input)?;
        let ratio = power(&left[4_800..]) / power(&input[4_800..]);
        assert!((0.5..=1.5).contains(&ratio), "{voices}: {ratio}");
    }
    Ok(())
}

#[test]
fn test_voices_change_while_playing() -> TestResult {
    let input = noise(9_600, 5);
    let mut rig = Rig::new(&[("voices", 1.0), ("mix", 100.0)])?;
    rig.render(&input, &input)?;
    rig.set("voices", 4.0)?;
    let (left, right) = rig.render(&input, &input)?;
    assert!(left.iter().chain(&right).all(|x| x.is_finite()));
    assert!(peak(&left) < 2.0);
    Ok(())
}
