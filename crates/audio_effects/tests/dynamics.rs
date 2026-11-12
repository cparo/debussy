//! The dynamics effect run in a processing graph, every block inside the real-time guard: the
//! levels its presets settle at, the gate's floor, the limiter's ceiling, and its latency.

use std::error::Error as StdError;
use std::f32::consts::TAU;

use audio_buffers::Layout;
use audio_effects::{effect, preset};
use dsp::{Ceiling, Curve, gain_to_db, transfer_db};
use realtime_guard::{GuardAllocator, assert_realtime};
use signal_graph::{Config, Graph, NodeId, Runner};

#[global_allocator]
static ALLOCATOR: GuardAllocator = GuardAllocator;

type TestResult = Result<(), Box<dyn StdError>>;

const RATE: u32 = 48_000;
const BLOCK: usize = 128;

/// A graph of one dynamics node, started from a preset, between a stereo source and sink.
struct Rig {
    graph: Graph,
    runner: Runner,
    source: NodeId,
    sink: NodeId,
}

impl Rig {
    fn new(preset_name: &str) -> Result<Self, Box<dyn StdError>> {
        let preset = preset(preset_name).ok_or("no preset")?;
        let node = effect(preset.kind).ok_or("unknown kind")?;
        // Values set before the node is added are what it is prepared with.
        for parameter in node.parameters() {
            if let Some(&(_, value)) = preset
                .values
                .iter()
                .find(|(n, _)| *n == parameter.info().name)
            {
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
            graph,
            runner,
            source,
            sink,
        })
    }

    /// Runs `input` through both channels and returns the left output; checks the right
    /// matches it.
    fn render(&mut self, input: &[f32]) -> Result<Vec<f32>, Box<dyn StdError>> {
        let (mut left, mut right) = (vec![0.0; input.len()], vec![0.0; input.len()]);
        for ((x, l), r) in input
            .chunks(BLOCK)
            .zip(left.chunks_mut(BLOCK))
            .zip(right.chunks_mut(BLOCK))
        {
            assert_realtime(|| self.block(x, l, r))?;
        }
        assert_eq!(left, right);
        Ok(left)
    }

    fn block(
        &mut self,
        input: &[f32],
        left: &mut [f32],
        right: &mut [f32],
    ) -> Result<(), &'static str> {
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
        right.copy_from_slice(block.channel(1).ok_or("no right")?);
        Ok(())
    }
}

/// One second of a 1 kHz sine at `db` dBFS peak.
fn sine(db: f32) -> Vec<f32> {
    let amplitude = 10.0_f32.powf(db / 20.0);
    (0..RATE as usize)
        .map(|n| amplitude * (TAU * 1000.0 * n as f32 / RATE as f32).sin())
        .collect()
}

/// One second of a 1 kHz square wave at `db` dBFS: its absolute value is steady, so a peak
/// detector settles on its level exactly.
fn square(db: f32) -> Vec<f32> {
    let amplitude = 10.0_f32.powf(db / 20.0);
    (0..RATE as usize)
        .map(|n| if n % 48 < 24 { amplitude } else { -amplitude })
        .collect()
}

/// The peak of the last tenth of `signal`, in dBFS: its level once settled.
fn settled_db(signal: &[f32]) -> f32 {
    let tail = &signal[signal.len() * 9 / 10..];
    gain_to_db(tail.iter().fold(0.0_f32, |m, &x| m.max(x.abs())))
}

/// The curve at the parameters' defaults, with `changes` applied.
fn curve(changes: impl FnOnce(&mut Curve)) -> Curve {
    let mut curve = Curve {
        threshold: -18.0,
        slope_above: 0.25,
        ratio_below: 1.0,
        knee: 6.0,
        range: 96.0,
        makeup: 0.0,
    };
    changes(&mut curve);
    curve
}

#[test]
fn test_compressor_settles_on_the_curve() -> TestResult {
    let mut rig = Rig::new("Compressor")?;
    let output = rig.render(&square(-6.0))?;
    let expected = transfer_db(-6.0, &curve(|_| ()), None);
    assert!(
        (settled_db(&output) - expected).abs() < 0.5,
        "{}",
        settled_db(&output)
    );
    Ok(())
}

#[test]
fn test_expander_settles_on_the_curve() -> TestResult {
    let mut rig = Rig::new("Expander")?;
    let output = rig.render(&square(-50.0))?;
    let expected = transfer_db(
        -50.0,
        &curve(|c| {
            c.threshold = -40.0;
            c.ratio_below = 2.0;
            c.slope_above = 1.0;
        }),
        None,
    );
    assert!((expected - (-60.0)).abs() < 0.1);
    assert!(
        (settled_db(&output) - expected).abs() < 0.5,
        "{}",
        settled_db(&output)
    );
    Ok(())
}

#[test]
fn test_gate_mutes_below_threshold_down_to_range() -> TestResult {
    let mut rig = Rig::new("Gate")?;
    let quiet = rig.render(&sine(-60.0))?;
    // 80 dB of range under −60 dB.
    assert!(
        (settled_db(&quiet) - (-140.0)).abs() < 1.0,
        "{}",
        settled_db(&quiet)
    );
    let loud = rig.render(&sine(-20.0))?;
    assert!(
        (settled_db(&loud) - (-20.0)).abs() < 0.5,
        "{}",
        settled_db(&loud)
    );
    Ok(())
}

#[test]
fn test_limiter_holds_a_steady_signal_at_the_ceiling() -> TestResult {
    let mut rig = Rig::new("Limiter")?;
    let expected = transfer_db(
        0.0,
        &curve(|c| c.slope_above = 1.0),
        Some(&Ceiling {
            level: -1.0,
            knee: 0.0,
        }),
    );
    assert!((expected - (-1.0)).abs() < 1e-4);
    // A soft limit: the peak detector's ripple on a sine leaves it a little over.
    let output = rig.render(&sine(0.0))?;
    assert!(
        settled_db(&output) <= -1.0 + 0.25,
        "{}",
        settled_db(&output)
    );
    let output = rig.render(&sine(-10.0))?;
    assert!((settled_db(&output) - (-10.0)).abs() < 0.1);
    Ok(())
}

#[test]
fn test_limiter_lookahead_catches_a_burst() -> TestResult {
    let mut rig = Rig::new("Limiter")?;
    let mut input = vec![0.0; RATE as usize / 2];
    input.extend(sine(6.0).iter().take(RATE as usize / 10));
    let output = rig.render(&input)?;
    let peak = gain_to_db(output.iter().fold(0.0_f32, |m, &x| m.max(x.abs())));
    assert!(peak <= -1.0 + 0.25, "{peak}");
    Ok(())
}

#[test]
fn test_limiter_reports_and_applies_its_lookahead() -> TestResult {
    let mut rig = Rig::new("Limiter")?;
    // 5 ms at 48 kHz.
    assert_eq!(rig.graph.latency(), 240);
    let mut input = vec![0.0; 1024];
    input[10] = 0.01;
    let output = rig.render(&input)?;
    let peak = output
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.abs().total_cmp(&b.1.abs()))
        .ok_or("no output")?;
    assert_eq!(peak.0, 250);
    assert!((peak.1 - 0.01).abs() < 1e-6);
    Ok(())
}

#[test]
fn test_compressor_has_no_latency_and_silence_stays_silent() -> TestResult {
    let mut rig = Rig::new("Compressor")?;
    assert_eq!(rig.graph.latency(), 0);
    let output = rig.render(&vec![0.0; 4096])?;
    assert!(output.iter().all(|&x| x == 0.0));
    Ok(())
}
