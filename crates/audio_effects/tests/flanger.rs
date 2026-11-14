//! The flanger run in a processing graph, every block inside the real-time guard: silence,
//! pass-through at no mix, stereo spread, and bounded output at the largest feedback.

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
const KIND: &str = "flanger";

/// A graph of one flanger node between a stereo source and sink.
struct Rig {
    _graph: Graph,
    /// The node's parameters, shared with it.
    parameters: Vec<Parameter>,
    runner: Runner,
    source: NodeId,
    sink: NodeId,
}

impl Rig {
    /// A flanger with the named parameters set before it is prepared.
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
    let mut rig = Rig::new(&[("feedback", 95.0)])?;
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
    let mut rig = Rig::new(&[("spread", 0.0), ("mix", 100.0)])?;
    let (left, right) = rig.render(&mono, &mono)?;
    assert_eq!(left, right);
    let mut rig = Rig::new(&[("spread", 180.0), ("mix", 100.0)])?;
    let (left, right) = rig.render(&mono, &mono)?;
    let difference = left
        .iter()
        .zip(&right)
        .fold(0.0_f32, |m, (a, b)| m.max((a - b).abs()));
    assert!(difference > 0.05, "{difference}");
    Ok(())
}

#[test]
fn test_full_feedback_stays_bounded_for_ten_seconds() -> TestResult {
    // A comb at feedback g has a largest gain of 1/(1 − |g|): 20 at 95 %.
    let input = noise(10 * RATE as usize, 6);
    for feedback in [95.0, -95.0] {
        let mut rig = Rig::new(&[
            ("feedback", feedback),
            ("delay", 0.1),
            ("depth", 100.0),
            ("rate", 5.0),
            ("mix", 100.0),
        ])?;
        let (left, right) = rig.render(&input, &input)?;
        let peak = peak(&left).max(peak(&right));
        assert!(peak.is_finite() && peak < 40.0, "{feedback}: {peak}");
    }
    Ok(())
}

#[test]
fn test_feedback_change_while_playing_raises_the_level() -> TestResult {
    let input = noise(9_600, 7);
    let mut rig = Rig::new(&[("feedback", 0.0), ("mix", 100.0)])?;
    let (a, _) = rig.render(&input, &input)?;
    rig.set("feedback", 80.0)?;
    let (b, _) = rig.render(&input, &input)?;
    assert!(peak(&b) > peak(&a), "{} vs {}", peak(&b), peak(&a));
    Ok(())
}
