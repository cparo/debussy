//! The mixer run in a processing graph, every block inside the real-time guard.

use std::error::Error as StdError;
use std::f32::consts::FRAC_1_SQRT_2;

use audio_buffers::Layout;
use audio_effects::{Control, Mixer, Mode, parameter_name};
use realtime_guard::{GuardAllocator, assert_realtime};
use signal_graph::{Config, Graph, NodeId, Parameter, Runner};

#[global_allocator]
static ALLOCATOR: GuardAllocator = GuardAllocator;

type TestResult = Result<(), Box<dyn StdError>>;

const RATE: u32 = 48_000;
const BLOCK: usize = 128;
/// Frames in one 10 ms ramp at [`RATE`].
const RAMP: usize = 480;

/// A graph of one mixer with a constant mono source per channel and a stereo sink.
struct Rig {
    graph: Graph,
    runner: Runner,
    mixer: NodeId,
    sources: Vec<NodeId>,
    sink: NodeId,
}

impl Rig {
    fn new(channels: usize) -> Result<Self, Box<dyn StdError>> {
        let (mut graph, runner) = Graph::new(Config::new(RATE, BLOCK))?;
        let mixer = graph.add_node(Box::new(Mixer::new(channels)?));
        let sink = graph.add_sink(Layout::Stereo);
        graph.connect(mixer, 0, sink, 0)?;
        let mut sources = Vec::new();
        for channel in 0..channels {
            let source = graph.add_source(Layout::Mono);
            graph.connect(source, 0, mixer, channel)?;
            sources.push(source);
        }
        graph.commit()?;
        Ok(Rig {
            graph,
            runner,
            mixer,
            sources,
            sink,
        })
    }

    fn parameter(&self, channel: usize, control: Control) -> Result<Parameter, &'static str> {
        let name = parameter_name(channel, control);
        self.graph
            .parameters(self.mixer)
            .ok_or("no mixer")?
            .iter()
            .find(|p| p.info().name == name)
            .cloned()
            .ok_or("no parameter")
    }

    fn set(&self, channel: usize, control: Control, value: f32) -> TestResult {
        self.parameter(channel, control)?.set(value);
        Ok(())
    }

    /// Runs `frames` frames with each source held at its value in `levels`, and returns the left
    /// and right output.
    fn render(
        &mut self,
        levels: &[f32],
        frames: usize,
    ) -> Result<(Vec<f32>, Vec<f32>), Box<dyn StdError>> {
        let (mut left, mut right) = (vec![0.0; frames], vec![0.0; frames]);
        for (l, r) in left.chunks_mut(BLOCK).zip(right.chunks_mut(BLOCK)) {
            assert_realtime(|| self.block(levels, l, r))?;
        }
        Ok((left, right))
    }

    fn block(
        &mut self,
        levels: &[f32],
        left: &mut [f32],
        right: &mut [f32],
    ) -> Result<(), &'static str> {
        self.runner
            .begin_block(left.len())
            .map_err(|_| "block refused")?;
        for (&source, &level) in self.sources.iter().zip(levels) {
            let block = self.runner.source_mut(source).ok_or("no source")?;
            block.channel_mut(0).ok_or("no channel")?.fill(level);
        }
        self.runner.process();
        let block = self.runner.sink(self.sink).ok_or("no sink")?;
        left.copy_from_slice(block.channel(0).ok_or("no left")?);
        right.copy_from_slice(block.channel(1).ok_or("no right")?);
        Ok(())
    }
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-5
}

#[test]
fn test_mixer_centres_and_pans_a_channel() -> TestResult {
    let mut rig = Rig::new(1)?;
    let (left, right) = rig.render(&[1.0], BLOCK)?;
    assert!(left.iter().chain(&right).all(|&x| close(x, FRAC_1_SQRT_2)));
    rig.set(0, Control::Pan, -1.0)?;
    let (left, right) = rig.render(&[1.0], 2 * RAMP)?;
    assert!(close(left[RAMP], 1.0) && close(right[RAMP], 0.0));
    // The pan moves smoothly: no jump bigger than one ramp step can make.
    assert!(left.windows(2).all(|w| (w[1] - w[0]).abs() < 0.01));
    Ok(())
}

#[test]
fn test_mixer_applies_gain_in_db() -> TestResult {
    let mut rig = Rig::new(1)?;
    rig.set(0, Control::Gain, 12.0)?;
    let (left, _) = rig.render(&[1.0], 2 * RAMP)?;
    assert!(close(left[RAMP], dsp::db_to_gain(12.0) * FRAC_1_SQRT_2));
    rig.set(0, Control::Gain, -96.0)?;
    let (left, _) = rig.render(&[1.0], 2 * RAMP)?;
    assert!(left[..RAMP - 1].iter().all(|&x| x > 0.0));
    assert!(left[RAMP..].iter().all(|&x| x == 0.0));
    Ok(())
}

#[test]
fn test_mixer_mute_ramps_to_silence_and_back() -> TestResult {
    let mut rig = Rig::new(1)?;
    rig.set(0, Control::Mode, Mode::Mute.value())?;
    let (left, _) = rig.render(&[1.0], 2 * RAMP)?;
    assert!(close(left[RAMP / 2 - 1], FRAC_1_SQRT_2 / 2.0));
    assert!(left[RAMP..].iter().all(|&x| x == 0.0));
    rig.set(0, Control::Mode, Mode::Play.value())?;
    let (left, _) = rig.render(&[1.0], 2 * RAMP)?;
    assert!(close(left[RAMP], FRAC_1_SQRT_2));
    Ok(())
}

#[test]
fn test_mixer_solo_isolates_soloed_channels() -> TestResult {
    let mut rig = Rig::new(3)?;
    let levels = [1.0, 10.0, 100.0];
    let mono = |left: &[f32]| left[2 * RAMP] / FRAC_1_SQRT_2;
    rig.set(1, Control::Mode, Mode::Solo.value())?;
    let (left, _) = rig.render(&levels, 3 * RAMP)?;
    assert!((mono(&left) - 10.0).abs() < 1e-4);
    rig.set(0, Control::Mode, Mode::Solo.value())?;
    let (left, _) = rig.render(&levels, 3 * RAMP)?;
    assert!((mono(&left) - 11.0).abs() < 1e-4);
    // A muted channel stays silent while another is soloed, and plays once soloed itself.
    rig.set(0, Control::Mode, Mode::Play.value())?;
    rig.set(2, Control::Mode, Mode::Mute.value())?;
    let (left, _) = rig.render(&levels, 3 * RAMP)?;
    assert!((mono(&left) - 10.0).abs() < 1e-4);
    rig.set(2, Control::Mode, Mode::Solo.value())?;
    let (left, _) = rig.render(&levels, 3 * RAMP)?;
    assert!((mono(&left) - 110.0).abs() < 1e-3);
    // With no solo left, every playing channel plays.
    for channel in 0..3 {
        rig.set(channel, Control::Mode, Mode::Play.value())?;
    }
    let (left, _) = rig.render(&levels, 3 * RAMP)?;
    assert!((mono(&left) - 111.0).abs() < 1e-3);
    Ok(())
}

#[test]
fn test_mixer_rounds_fractional_modes() -> TestResult {
    let mut rig = Rig::new(1)?;
    rig.set(0, Control::Mode, 1.4)?;
    let (left, _) = rig.render(&[1.0], 2 * RAMP)?;
    assert_eq!(left[RAMP], 0.0);
    Ok(())
}

#[test]
fn test_mixer_solo_is_scoped_to_its_mixer() -> TestResult {
    let (mut graph, mut runner) = Graph::new(Config::new(RATE, BLOCK))?;
    let source = graph.add_source(Layout::Mono);
    let sink = graph.add_sink(Layout::Stereo);
    let mut mixers = Vec::new();
    for _ in 0..2 {
        let mixer = graph.add_node(Box::new(Mixer::new(2)?));
        graph.connect(source, 0, mixer, 0)?;
        graph.connect(mixer, 0, sink, 0)?;
        mixers.push(mixer);
    }
    graph.commit()?;
    // Soloing the unused channel 2 of the first mixer silences only that mixer's channel 1.
    let solo = &graph.parameters(mixers[0]).ok_or("no mixer")?[5];
    assert_eq!(solo.info().name, "channel 2 mode");
    solo.set(Mode::Solo.value());
    let mut last = 0.0;
    for _ in 0..(2 * RAMP).div_ceil(BLOCK) {
        last = assert_realtime(|| -> Result<f32, &'static str> {
            runner.begin_block(BLOCK).map_err(|_| "block refused")?;
            runner
                .source_mut(source)
                .ok_or("no source")?
                .channel_mut(0)
                .ok_or("no channel")?
                .fill(1.0);
            runner.process();
            let left = runner
                .sink(sink)
                .ok_or("no sink")?
                .channel(0)
                .ok_or("no left")?;
            Ok(left[BLOCK - 1])
        })?;
    }
    assert!(close(last, FRAC_1_SQRT_2));
    Ok(())
}

#[test]
fn test_mixer_replaced_with_more_channels_keeps_connections_and_values() -> TestResult {
    let mut rig = Rig::new(2)?;
    rig.set(1, Control::Gain, -6.0)?;
    rig.set(1, Control::Mode, Mode::Solo.value())?;
    let dropped = rig
        .graph
        .replace_node(rig.mixer, Box::new(Mixer::new(3)?))?;
    assert!(dropped.is_empty());
    let third = rig.graph.add_source(Layout::Mono);
    rig.graph.connect(third, 0, rig.mixer, 2)?;
    rig.sources.push(third);
    rig.graph.commit()?;
    assert_eq!(rig.parameter(1, Control::Gain)?.get(), -6.0);
    assert_eq!(rig.parameter(1, Control::Mode)?.get(), Mode::Solo.value());
    rig.set(2, Control::Mode, Mode::Solo.value())?;
    let (left, _) = rig.render(&[1.0, 10.0, 100.0], 3 * RAMP)?;
    let expected = (10.0 * dsp::db_to_gain(-6.0) + 100.0) * FRAC_1_SQRT_2;
    assert!((left[2 * RAMP] - expected).abs() < 1e-3);
    Ok(())
}

#[test]
fn test_mixer_output_folds_to_a_mono_input() -> TestResult {
    let (mut graph, mut runner) = Graph::new(Config::new(RATE, BLOCK))?;
    let source = graph.add_source(Layout::Mono);
    let mixer = graph.add_node(Box::new(Mixer::new(1)?));
    let sink = graph.add_sink(Layout::Mono);
    graph.connect(source, 0, mixer, 0)?;
    graph.connect(mixer, 0, sink, 0)?;
    graph.commit()?;
    let out = assert_realtime(|| -> Result<f32, &'static str> {
        runner.begin_block(BLOCK).map_err(|_| "block refused")?;
        runner
            .source_mut(source)
            .ok_or("no source")?
            .channel_mut(0)
            .ok_or("no channel")?
            .fill(1.0);
        runner.process();
        Ok(runner
            .sink(sink)
            .ok_or("no sink")?
            .channel(0)
            .ok_or("no channel")?[0])
    })?;
    // A centred channel folds back to unity (ADR-0030).
    assert!(close(out, 1.0));
    Ok(())
}
