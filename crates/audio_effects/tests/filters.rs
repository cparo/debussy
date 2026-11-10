//! The high-pass, low-pass and parametric equaliser run in a processing graph, every block inside
//! the real-time guard: what they pass and reject, and that moving their controls does not click.

use std::error::Error as StdError;
use std::f32::consts::TAU;

use audio_buffers::Layout;
use audio_effects::{
    BandControl, BandType, PassControl, Response, band_parameter_name, effect, pass_parameter_name,
};
use realtime_guard::{GuardAllocator, assert_realtime};
use signal_graph::{Config, Graph, NodeId, Runner};

#[global_allocator]
static ALLOCATOR: GuardAllocator = GuardAllocator;

type TestResult = Result<(), Box<dyn StdError>>;

const RATE: u32 = 48_000;
const BLOCK: usize = 128;
/// Frames in one 10 ms ramp at [`RATE`].
const RAMP: f32 = 480.0;

/// A graph of one effect between a stereo source and a stereo sink.
struct Rig {
    graph: Graph,
    runner: Runner,
    node: NodeId,
    source: NodeId,
    sink: NodeId,
    /// Frames rendered so far, so a sine continues across renders.
    clock: usize,
}

impl Rig {
    fn new(kind: &str) -> Result<Self, Box<dyn StdError>> {
        let (mut graph, runner) = Graph::new(Config::new(RATE, BLOCK))?;
        let node = graph.add_node(effect(kind).ok_or("unknown kind")?);
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
            clock: 0,
        })
    }

    fn set(&self, name: &str, value: f32) -> TestResult {
        self.graph
            .parameters(self.node)
            .ok_or("no node")?
            .iter()
            .find(|p| p.info().name == name)
            .ok_or("no parameter")?
            .set(value);
        Ok(())
    }

    fn get(&self, name: &str) -> Result<f32, Box<dyn StdError>> {
        Ok(self
            .graph
            .parameters(self.node)
            .ok_or("no node")?
            .iter()
            .find(|p| p.info().name == name)
            .ok_or("no parameter")?
            .get())
    }

    /// Runs `frames` frames of a sine of `frequency` and amplitude 0.5 in both channels, and
    /// returns the left output; checks the right matches it.
    fn render(&mut self, frequency: f32, frames: usize) -> Result<Vec<f32>, Box<dyn StdError>> {
        let start = self.clock;
        let input: Vec<f32> = (start..start + frames)
            .map(|n| 0.5 * (TAU * frequency * n as f32 / RATE as f32).sin())
            .collect();
        self.clock += frames;
        let (mut left, mut right) = (vec![0.0; frames], vec![0.0; frames]);
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
        block
            .channel_mut(0)
            .ok_or("no left")?
            .copy_from_slice(input);
        block
            .channel_mut(1)
            .ok_or("no right")?
            .copy_from_slice(input);
        self.runner.process();
        let block = self.runner.sink(self.sink).ok_or("no sink")?;
        left.copy_from_slice(block.channel(0).ok_or("no left")?);
        right.copy_from_slice(block.channel(1).ok_or("no right")?);
        Ok(())
    }
}

/// The peak level of `signal`, in dB relative to the test sine's amplitude of 0.5.
fn level_db(signal: &[f32]) -> f32 {
    let peak = signal.iter().fold(0.0_f32, |m, &x| m.max(x.abs()));
    20.0 * (peak / 0.5).log10()
}

/// The largest second difference, `|x[n+1] − 2·x[n] + x[n−1]|`: small for a low sine, large at a
/// jump or a kink, which is what a click is.
fn largest_step(signal: &[f32]) -> f32 {
    signal
        .windows(3)
        .fold(0.0_f32, |m, w| m.max((w[2] - 2.0 * w[1] + w[0]).abs()))
}

/// Renders `frequency` while `change` is applied, and checks the output never bends more sharply
/// than a sine of that frequency and the output's own peak could, allowing for the corners of a
/// linear 10 ms ramp of its level. A sine of amplitude `a` has a second difference of at most
/// `a·ω²`; a level ramp's start or end adds up to about `a / RAMP`. A click (a jump, a kink or a
/// burst of ringing above the sine) adds far more, and is returned as an error.
fn assert_no_click(
    rig: &mut Rig,
    frequency: f32,
    change: impl FnOnce(&Rig) -> TestResult,
) -> TestResult {
    rig.render(frequency, 4_800)?;
    change(rig)?;
    let during = rig.render(frequency, 4_800)?;
    let omega = TAU * frequency / RATE as f32;
    let peak = during.iter().fold(0.0_f32, |m, &x| m.max(x.abs()));
    let allowed = 1.2 * peak * omega * omega + 1.2 * peak / RAMP + 1e-5;
    let step = largest_step(&during);
    if step > allowed {
        return Err(format!("click: step {step} above {allowed}").into());
    }
    Ok(())
}

#[test]
fn test_high_pass_passes_highs_and_rejects_lows() -> TestResult {
    let mut rig = Rig::new("high-pass")?;
    rig.set("cutoff", 1_000.0)?;
    rig.set("order", 4.0)?;
    rig.render(5_000.0, 4_800)?;
    assert!(level_db(&rig.render(5_000.0, 4_800)?).abs() < 0.1);
    rig.render(250.0, 9_600)?;
    // Two octaves below a 4-pole cutoff: about 48 dB down.
    let low = level_db(&rig.render(250.0, 9_600)?);
    assert!((-50.0..-45.0).contains(&low), "{low}");
    Ok(())
}

#[test]
fn test_low_pass_passes_lows_and_rejects_highs() -> TestResult {
    let mut rig = Rig::new("low-pass")?;
    rig.set("cutoff", 1_000.0)?;
    rig.render(200.0, 9_600)?;
    assert!(level_db(&rig.render(200.0, 9_600)?).abs() < 0.1);
    rig.render(4_000.0, 4_800)?;
    // Two octaves above a 2-pole cutoff: about 24 dB down.
    let high = level_db(&rig.render(4_000.0, 4_800)?);
    assert!((-26.0..-22.0).contains(&high), "{high}");
    Ok(())
}

#[test]
fn test_order_change_applies_while_stopped() -> TestResult {
    let mut rig = Rig::new("low-pass")?;
    rig.set("cutoff", 1_000.0)?;
    rig.set("order", 8.0)?;
    rig.render(2_000.0, 9_600)?;
    // An octave above an 8-pole cutoff: about 48 dB down.
    let high = level_db(&rig.render(2_000.0, 4_800)?);
    assert!((-50.0..-46.0).contains(&high), "{high}");
    Ok(())
}

#[test]
fn test_cutoff_sweep_does_not_click() -> TestResult {
    let mut rig = Rig::new("high-pass")?;
    rig.set("order", 8.0)?;
    assert_no_click(&mut rig, 1_000.0, |rig| rig.set("cutoff", 900.0))?;
    assert_no_click(&mut rig, 1_000.0, |rig| rig.set("cutoff", 20.0))?;
    let mut rig = Rig::new("low-pass")?;
    assert_no_click(&mut rig, 1_000.0, |rig| rig.set("cutoff", 1_200.0))
}

#[test]
fn test_click_check_catches_an_instant_order_change() -> TestResult {
    // The order changes only while stopped, so it switches at once, from rest; while playing
    // that would click, and the check must see it.
    let mut rig = Rig::new("high-pass")?;
    rig.set("cutoff", 900.0)?;
    assert!(assert_no_click(&mut rig, 1_000.0, |rig| rig.set("order", 8.0)).is_err());
    Ok(())
}

#[test]
fn test_parametric_band_reaches_its_gain_at_the_centre() -> TestResult {
    let mut rig = Rig::new("parametric-eq")?;
    assert!(level_db(&rig.render(1_000.0, 4_800)?).abs() < 1e-4);
    rig.set(&band_parameter_name(1, BandControl::Enabled), 1.0)?;
    rig.set(&band_parameter_name(1, BandControl::Frequency), 1_000.0)?;
    rig.set(&band_parameter_name(1, BandControl::Gain), 9.0)?;
    rig.set(&band_parameter_name(1, BandControl::Q), 2.0)?;
    rig.render(1_000.0, 9_600)?;
    let centre = level_db(&rig.render(1_000.0, 4_800)?);
    assert!((centre - 9.0).abs() < 0.05, "{centre}");
    rig.render(100.0, 9_600)?;
    assert!(level_db(&rig.render(100.0, 9_600)?).abs() < 0.2);
    Ok(())
}

#[test]
fn test_parametric_enable_and_type_change_do_not_click() -> TestResult {
    let mut rig = Rig::new("parametric-eq")?;
    let name = |control| band_parameter_name(2, control);
    rig.set(&name(BandControl::Frequency), 300.0)?;
    rig.set(&name(BandControl::Gain), 12.0)?;
    assert_no_click(&mut rig, 300.0, |rig| {
        rig.set(&name(BandControl::Enabled), 1.0)
    })?;
    assert_no_click(&mut rig, 300.0, |rig| {
        rig.set(&name(BandControl::Type), BandType::Notch.value())
    })?;
    assert_no_click(&mut rig, 300.0, |rig| {
        rig.set(&name(BandControl::Type), BandType::LowShelf.value())
    })?;
    assert_no_click(&mut rig, 300.0, |rig| {
        rig.set(&name(BandControl::Frequency), 2_000.0)
    })?;
    assert_no_click(&mut rig, 300.0, |rig| {
        rig.set(&name(BandControl::Enabled), 0.0)
    })?;
    // Off again: transparent.
    assert!((level_db(&rig.render(300.0, 4_800)?)).abs() < 1e-4);
    Ok(())
}

#[test]
fn test_parametric_high_pass_rejects_lows_and_toggles_without_clicking() -> TestResult {
    let mut rig = Rig::new("parametric-eq")?;
    let name = |control| pass_parameter_name(Response::HighPass, control);
    rig.set(&name(PassControl::Cutoff), 1_000.0)?;
    rig.set(&name(PassControl::Order), 4.0)?;
    // Off: transparent, whatever the cutoff.
    assert!(level_db(&rig.render(250.0, 4_800)?).abs() < 1e-4);
    assert_no_click(&mut rig, 250.0, |rig| {
        rig.set(&name(PassControl::Enabled), 1.0)
    })?;
    rig.render(250.0, 9_600)?;
    // Two octaves below a 4-pole cutoff: about 48 dB down.
    let low = level_db(&rig.render(250.0, 9_600)?);
    assert!((-50.0..-45.0).contains(&low), "{low}");
    rig.render(5_000.0, 4_800)?;
    assert!(level_db(&rig.render(5_000.0, 4_800)?).abs() < 0.1);
    assert_no_click(&mut rig, 250.0, |rig| {
        rig.set(&name(PassControl::Enabled), 0.0)
    })?;
    // Off again: transparent.
    assert!(level_db(&rig.render(250.0, 4_800)?).abs() < 1e-4);
    Ok(())
}

#[test]
fn test_parametric_low_pass_stage_follows_the_bands() -> TestResult {
    let mut rig = Rig::new("parametric-eq")?;
    let name = |control| pass_parameter_name(Response::LowPass, control);
    rig.set(&name(PassControl::Cutoff), 1_000.0)?;
    rig.set(&name(PassControl::Enabled), 1.0)?;
    rig.render(4_000.0, 9_600)?;
    // Two octaves above a 2-pole cutoff: about 24 dB down.
    let high = level_db(&rig.render(4_000.0, 4_800)?);
    assert!((-26.0..-22.0).contains(&high), "{high}");
    Ok(())
}

#[test]
fn test_replacing_a_node_keeps_its_values() -> TestResult {
    let mut rig = Rig::new("parametric-eq")?;
    let gain = band_parameter_name(3, BandControl::Gain);
    rig.set(&gain, -6.5)?;
    rig.graph
        .replace_node(rig.node, effect("parametric-eq").ok_or("unknown kind")?)?;
    rig.graph.commit()?;
    assert_eq!(rig.get(&gain)?, -6.5);
    let mut rig = Rig::new("high-pass")?;
    rig.set("cutoff", 440.0)?;
    rig.graph
        .replace_node(rig.node, effect("high-pass").ok_or("unknown kind")?)?;
    rig.graph.commit()?;
    assert_eq!(rig.get("cutoff")?, 440.0);
    Ok(())
}
