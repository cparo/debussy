//! The reverb run in a processing graph, every block inside the real-time guard: the decay time
//! measured from the impulse response for both algorithms, the bound at the longest decay, the
//! algorithm switch, damping, and the pre-delay.

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

/// A graph of one reverb node between a stereo source and sink.
struct Rig {
    _graph: Graph,
    /// The node's parameters, shared with it.
    parameters: Vec<Parameter>,
    runner: Runner,
    source: NodeId,
    sink: NodeId,
}

impl Rig {
    /// A reverb with the named parameters set before it is prepared.
    fn new(values: &[(&str, f32)]) -> Result<Self, Box<dyn StdError>> {
        let node = effect("reverb").ok_or("unknown kind")?;
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

    /// Sets parameter `name` to `value`.
    fn set(&self, name: &str, value: f32) -> TestResult {
        self.parameters
            .iter()
            .find(|p| p.info().name == name)
            .ok_or("no such parameter")?
            .set(value);
        Ok(())
    }

    /// Runs `input` into both channels; returns the two outputs.
    fn render(&mut self, input: &[f32]) -> Result<(Vec<f32>, Vec<f32>), Box<dyn StdError>> {
        let (mut left, mut right) = (vec![0.0; input.len()], vec![0.0; input.len()]);
        for ((x, l), r) in input
            .chunks(BLOCK)
            .zip(left.chunks_mut(BLOCK))
            .zip(right.chunks_mut(BLOCK))
        {
            assert_realtime(|| self.block(x, l, r))?;
        }
        Ok((left, right))
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

/// A unit impulse at frame 0 of `frames` frames.
fn impulse(frames: usize) -> Vec<f32> {
    let mut signal = vec![0.0; frames];
    signal[0] = 1.0;
    signal
}

/// The decay time of an impulse response, in seconds: twice the time its backward-integrated
/// energy (Schroeder's curve) takes to fall from −5 to −35 dB.
fn rt60(left: &[f32], right: &[f32]) -> Result<f32, Box<dyn StdError>> {
    let mut remaining: Vec<f64> = left
        .iter()
        .zip(right)
        .map(|(l, r)| f64::from(l * l + r * r))
        .collect();
    for i in (0..remaining.len() - 1).rev() {
        remaining[i] += remaining[i + 1];
    }
    let total = remaining[0];
    let crossing = |db: f64| {
        remaining
            .iter()
            .position(|&e| 10.0 * (e / total).log10() <= db)
            .ok_or(format!("never falls {db} dB"))
    };
    let (from, to) = (crossing(-5.0)?, crossing(-35.0)?);
    #[allow(clippy::cast_precision_loss)] // frame counts are small
    let seconds = 2.0 * (to - from) as f32 / RATE as f32;
    Ok(seconds)
}

#[test]
fn test_reverb_decay_time_matches_decay_for_both_algorithms() -> TestResult {
    for algorithm in [0.0, 1.0] {
        for decay in [0.5, 1.0, 3.0] {
            let mut rig = Rig::new(&[
                ("algorithm", algorithm),
                ("decay", decay),
                ("damping", 20_000.0),
                ("pre-delay", 0.0),
                ("mix", 100.0),
            ])?;
            // Long enough for the whole fall plus a margin of tail, so the curve's end does not
            // cut it short.
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let frames = (2.0 * decay * RATE as f32) as usize;
            let (left, right) = rig.render(&impulse(frames))?;
            let measured = rt60(&left, &right)?;
            let error = (measured - decay).abs() / decay;
            assert!(
                error < 0.15,
                "algorithm {algorithm}, decay {decay}: measured {measured}"
            );
        }
    }
    Ok(())
}

#[test]
fn test_reverb_longest_decay_stays_bounded() -> TestResult {
    for algorithm in [0.0, 1.0] {
        let mut rig = Rig::new(&[
            ("algorithm", algorithm),
            ("decay", 20.0),
            ("damping", 20_000.0),
            ("mix", 100.0),
        ])?;
        // Five seconds of a full-scale square wave, then three of silence.
        let mut input: Vec<f32> = (0..5 * RATE as usize)
            .map(|n| if (n / 55) % 2 == 0 { 1.0 } else { -1.0 })
            .collect();
        input.resize(8 * RATE as usize, 0.0);
        let (left, right) = rig.render(&input)?;
        let peak = left.iter().chain(&right).fold(0.0_f32, |m, x| {
            if x.is_finite() {
                m.max(x.abs())
            } else {
                f32::INFINITY
            }
        });
        assert!(peak < 20.0, "algorithm {algorithm}: peak {peak}");
        // The tail is still ringing but falling.
        let energy = |s: &[f32]| s.iter().map(|x| x * x).sum::<f32>();
        let second = RATE as usize;
        let early = energy(&left[5 * second..6 * second]);
        let late = energy(&left[7 * second..8 * second]);
        assert!(
            late < early && late > 0.0,
            "algorithm {algorithm}: {early} {late}"
        );
    }
    Ok(())
}

#[test]
fn test_reverb_algorithm_switch_starts_the_new_engine_silent() -> TestResult {
    let mut switched = Rig::new(&[("mix", 100.0)])?;
    let mut fresh = Rig::new(&[("mix", 100.0), ("algorithm", 1.0)])?;
    // A tail in the room, then a switch to the plate.
    switched.render(&impulse(RATE as usize / 2))?;
    switched.set("algorithm", 1.0)?;
    let input = impulse(RATE as usize / 2);
    let (left, right) = switched.render(&input)?;
    let (want_left, want_right) = fresh.render(&input)?;
    // Nothing of the room's tail survives, and the plate starts as a new one does.
    assert_eq!(left, want_left);
    assert_eq!(right, want_right);
    Ok(())
}

#[test]
fn test_reverb_damping_darkens_the_tail() -> TestResult {
    // The energy of the tail's first difference, a rough measure of its high frequencies, over
    // its plain energy.
    let brightness = |damping: f32, algorithm: f32| -> Result<f32, Box<dyn StdError>> {
        let mut rig = Rig::new(&[
            ("algorithm", algorithm),
            ("decay", 2.0),
            ("damping", damping),
            ("mix", 100.0),
        ])?;
        let (left, _) = rig.render(&impulse(RATE as usize))?;
        let tail = &left[RATE as usize / 2..];
        let energy: f32 = tail.iter().map(|x| x * x).sum();
        let edges: f32 = tail.windows(2).map(|w| (w[1] - w[0]).powi(2)).sum();
        Ok(edges / energy)
    };
    for algorithm in [0.0, 1.0] {
        let bright = brightness(20_000.0, algorithm)?;
        let dark = brightness(1_000.0, algorithm)?;
        assert!(
            dark < bright * 0.5,
            "algorithm {algorithm}: {bright} {dark}"
        );
    }
    Ok(())
}

#[test]
fn test_reverb_pre_delay_holds_back_the_tail() -> TestResult {
    let onset = |pre_delay: f32| -> Result<usize, Box<dyn StdError>> {
        let mut rig = Rig::new(&[("pre-delay", pre_delay), ("mix", 100.0)])?;
        let (left, right) = rig.render(&impulse(RATE as usize / 4))?;
        left.iter()
            .zip(&right)
            .position(|(l, r)| l.abs() + r.abs() > 1e-6)
            .ok_or_else(|| "silent".into())
    };
    // 50 ms is 2400 frames later.
    assert_eq!(onset(50.0)? - onset(0.0)?, 2_399);
    Ok(())
}
