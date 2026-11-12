//! The dynamics effect run in a processing graph, every block inside the real-time guard: at its
//! longest window, lookahead and oversampling, with its live controls moving.

use std::error::Error as StdError;
use std::f32::consts::TAU;

use audio_buffers::Layout;
use audio_effects::{DynamicsControl, effect};
use realtime_guard::{GuardAllocator, assert_realtime};
use signal_graph::{Config, Graph, NodeId, Runner};

#[global_allocator]
static ALLOCATOR: GuardAllocator = GuardAllocator;

type TestResult = Result<(), Box<dyn StdError>>;

const RATE: u32 = 48_000;
const BLOCK: usize = 128;

/// A graph of one dynamics effect between a stereo source and a stereo sink, its stopped-only
/// values set before it is added.
struct Rig {
    graph: Graph,
    runner: Runner,
    node: NodeId,
    source: NodeId,
    sink: NodeId,
}

impl Rig {
    fn new(fixed: &[(DynamicsControl, f32)]) -> Result<Self, Box<dyn StdError>> {
        let (mut graph, runner) = Graph::new(Config::new(RATE, BLOCK))?;
        let node = effect("dynamics").ok_or("unknown kind")?;
        for &(control, value) in fixed {
            node.parameters()
                .iter()
                .find(|p| p.info().name == control.name())
                .ok_or("no parameter")?
                .set(value);
        }
        let node = graph.add_node(node);
        let source = graph.add_source(Layout::Stereo);
        let sink = graph.add_sink(Layout::Stereo);
        graph.connect(source, 0, node, 0)?;
        graph.connect(node, 0, sink, 0)?;
        graph.commit()?;
        Ok(Rig {
            graph,
            runner,
            node,
            source,
            sink,
        })
    }

    fn set(&self, control: DynamicsControl, value: f32) -> TestResult {
        self.graph
            .parameters(self.node)
            .ok_or("no node")?
            .iter()
            .find(|p| p.info().name == control.name())
            .ok_or("no parameter")?
            .set(value);
        Ok(())
    }

    /// Runs `blocks` blocks of a 440 Hz sine of amplitude 0.5 in both channels, each block inside
    /// the guard, and returns the left output.
    fn render(&mut self, blocks: usize) -> Result<Vec<f32>, Box<dyn StdError>> {
        let mut output = Vec::with_capacity(blocks * BLOCK);
        let mut input = [0.0_f32; BLOCK];
        let mut left = [0.0_f32; BLOCK];
        for b in 0..blocks {
            for (i, x) in input.iter_mut().enumerate() {
                let n = (b * BLOCK + i) as f32;
                *x = 0.5 * (TAU * 440.0 * n / RATE as f32).sin();
            }
            assert_realtime(|| self.block(&input, &mut left))?;
            output.extend_from_slice(&left);
        }
        Ok(output)
    }

    fn block(&mut self, input: &[f32], left: &mut [f32]) -> Result<(), &'static str> {
        self.runner
            .begin_block(input.len())
            .map_err(|_| "block refused")?;
        let block = self.runner.source_mut(self.source).ok_or("no source")?;
        for channel in 0..2 {
            block
                .channel_mut(channel)
                .ok_or("no channel")?
                .copy_from_slice(input);
        }
        self.runner.process();
        let block = self.runner.sink(self.sink).ok_or("no sink")?;
        left.copy_from_slice(block.channel(0).ok_or("no left")?);
        Ok(())
    }
}

#[test]
fn test_dynamics_runs_real_time_at_its_largest_settings() -> TestResult {
    let mut rig = Rig::new(&[
        (DynamicsControl::Attack, 100.0),
        (DynamicsControl::Release, 5_000.0),
        (DynamicsControl::AttackExponent, 3.0),
        (DynamicsControl::Lookahead, 100.0),
        (DynamicsControl::Oversampling, 3.0),
    ])?;
    // 100 ms of lookahead and the 8× oversampler's 32 frames.
    assert_eq!(rig.graph.latency(), 4_832);
    let quiet = rig.render(40)?;
    assert!(quiet[..4_800].iter().all(|&x| x == 0.0));
    rig.set(DynamicsControl::Asymmetric, 1.0)?;
    rig.set(DynamicsControl::NegativeSteepness, 4.0)?;
    rig.set(DynamicsControl::LinkChannels, 0.0)?;
    rig.set(DynamicsControl::Preamp, 12.0)?;
    rig.set(DynamicsControl::PositiveExponent, 2.0)?;
    rig.set(DynamicsControl::PositiveCeiling, -6.0)?;
    let output = rig.render(40)?;
    assert!(output.iter().all(|x| x.is_finite()));
    assert!(output.iter().any(|&x| x != 0.0));
    Ok(())
}

#[test]
fn test_dynamics_waveshaper_runs_real_time() -> TestResult {
    let mut rig = Rig::new(&[
        (DynamicsControl::Attack, 0.0),
        (DynamicsControl::Release, 0.0),
        (DynamicsControl::Oversampling, 1.0),
    ])?;
    rig.set(DynamicsControl::PositiveSteepness, 10.0)?;
    rig.set(DynamicsControl::PositiveExponent, 0.2)?;
    let output = rig.render(20)?;
    // Driven hard, the sine is squared off near ±1; oversampled, the down-sampling filter may
    // ring a little past it.
    let peak = output.iter().fold(0.0_f32, |m, x| m.max(x.abs()));
    assert!(peak > 0.9 && peak <= 1.01, "{peak}");
    Ok(())
}
