//! The delay run in a processing graph, every block inside the real-time guard: echo times and
//! feedback, ping-pong, the right channel's offset, a time change while playing, saturation at
//! full feedback, and the low-pass in the feedback path.

use std::error::Error as StdError;
use std::f32::consts::TAU;

use audio_buffers::Layout;
use audio_effects::effect;
use dsp::{Complex, RealFft};
use realtime_guard::{GuardAllocator, assert_realtime};
use signal_graph::{Config, Graph, NodeId, Parameter, Runner};

#[global_allocator]
static ALLOCATOR: GuardAllocator = GuardAllocator;

type TestResult = Result<(), Box<dyn StdError>>;

const RATE: u32 = 48_000;
const BLOCK: usize = 128;
/// 100 ms at the rate.
const TAP: usize = 4_800;

/// A graph of one delay node between a stereo source and sink.
struct Rig {
    _graph: Graph,
    /// The node's parameters, shared with it.
    parameters: Vec<Parameter>,
    runner: Runner,
    source: NodeId,
    sink: NodeId,
}

impl Rig {
    /// A delay with the named parameters set before it is prepared.
    fn new(values: &[(&str, f32)]) -> Result<Self, Box<dyn StdError>> {
        let node = effect("delay").ok_or("unknown kind")?;
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

/// A unit impulse at frame 0 of `frames` frames.
fn impulse(frames: usize) -> Vec<f32> {
    let mut signal = vec![0.0; frames];
    signal[0] = 1.0;
    signal
}

/// The frames of `signal` whose magnitude is above `floor`, with their values.
fn peaks(signal: &[f32], floor: f32) -> Vec<(usize, f32)> {
    signal
        .iter()
        .enumerate()
        .filter(|&(_, x)| x.abs() > floor)
        .map(|(i, &x)| (i, x))
        .collect()
}

/// Asserts `got` is within `1e-5` of `want`, position by position.
fn assert_peaks(got: &[(usize, f32)], want: &[(usize, f32)]) {
    assert_eq!(got.len(), want.len(), "{got:?} vs {want:?}");
    for (&(i, x), &(j, y)) in got.iter().zip(want) {
        assert_eq!(i, j, "{got:?} vs {want:?}");
        assert!((x - y).abs() < 1e-5, "{got:?} vs {want:?}");
    }
}

/// The wet signal alone, at 100 ms.
const WET: [(&str, f32); 2] = [("time", 100.0), ("mix", 100.0)];

#[test]
fn test_delay_impulse_returns_at_the_time_then_at_the_feedback() -> TestResult {
    let mut rig = Rig::new(&[WET[0], WET[1], ("feedback", 50.0)])?;
    let input = impulse(3 * TAP + 10);
    let (left, right) = rig.render(&input, &input)?;
    let want = [(TAP, 1.0), (2 * TAP, 0.5), (3 * TAP, 0.25)];
    assert_peaks(&peaks(&left, 1e-6), &want);
    assert_peaks(&peaks(&right, 1e-6), &want);
    Ok(())
}

#[test]
fn test_delay_ping_pong_alternates_sides() -> TestResult {
    let mut rig = Rig::new(&[WET[0], WET[1], ("feedback", 50.0), ("mode", 1.0)])?;
    let input = impulse(4 * TAP + 10);
    let silence = vec![0.0; input.len()];
    // An impulse on the right only still starts on the left: the input is folded to mono.
    let (left, right) = rig.render(&silence, &input)?;
    let mono = dsp::FOLD_DOWN_GAIN;
    assert_peaks(&peaks(&left, 1e-6), &[(TAP, mono), (3 * TAP, mono * 0.25)]);
    assert_peaks(
        &peaks(&right, 1e-6),
        &[(2 * TAP, mono * 0.5), (4 * TAP, mono * 0.125)],
    );
    Ok(())
}

#[test]
fn test_delay_offset_shifts_the_right_channel() -> TestResult {
    let mut rig = Rig::new(&[WET[0], WET[1], ("feedback", 0.0), ("offset", 10.0)])?;
    let input = impulse(2 * TAP);
    let (left, right) = rig.render(&input, &input)?;
    assert_peaks(&peaks(&left, 1e-6), &[(TAP, 1.0)]);
    assert_peaks(&peaks(&right, 1e-6), &[(TAP + 480, 1.0)]);
    Ok(())
}

#[test]
fn test_delay_time_change_while_playing_does_not_jump() -> TestResult {
    let mut rig = Rig::new(&[WET[0], WET[1], ("feedback", 0.0)])?;
    // A 100 Hz sine at 0.5 moves at most 0.0066 a frame.
    #[allow(clippy::cast_precision_loss)] // frame counts are small
    let sine: Vec<f32> = (0..RATE as usize)
        .map(|n| 0.5 * (TAU * 100.0 * n as f32 / RATE as f32).sin())
        .collect();
    let half = sine.len() / 2;
    let (first, _) = rig.render(&sine[..half], &sine[..half])?;
    // Not a whole number of cycles, so the two taps are out of phase.
    rig.set("time", 237.5)?;
    let (second, _) = rig.render(&sine[half..], &sine[half..])?;
    let output: Vec<f32> = first.into_iter().chain(second).collect();
    let largest_step = output[TAP..]
        .windows(2)
        .map(|w| (w[1] - w[0]).abs())
        .fold(0.0_f32, f32::max);
    // The fade moves at most the gap between the taps (≤ 1) over 480 frames, on top of the sine.
    assert!(largest_step < 0.0066 + 1.0 / 480.0 + 1e-4, "{largest_step}");
    Ok(())
}

#[test]
fn test_delay_full_feedback_with_saturation_stays_bounded() -> TestResult {
    let mut rig = Rig::new(&[
        ("time", 10.0),
        ("mix", 100.0),
        ("feedback", 100.0),
        ("saturation", 1.0),
    ])?;
    // A full-scale square wave, repeating faster than the delay, for five seconds.
    let input: Vec<f32> = (0..5 * RATE as usize)
        .map(|n| if (n / 37) % 2 == 0 { 1.0 } else { -1.0 })
        .collect();
    let (left, right) = rig.render(&input, &input)?;
    // The line holds the input (≤ 1) plus saturated feedback (< 1).
    for x in left.iter().chain(&right) {
        assert!(x.is_finite() && x.abs() < 2.0, "{x}");
    }
    Ok(())
}

/// The share of `window`'s energy above `hz`.
fn high_share(window: &[f32], hz: f32) -> Result<f32, Box<dyn StdError>> {
    let mut fft = RealFft::new(window.len())?;
    let mut bins = vec![Complex::default(); fft.bins()];
    fft.forward(window, &mut bins)?;
    let power: Vec<f32> = bins.iter().map(|c| c.abs() * c.abs()).collect();
    #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
    #[allow(clippy::cast_sign_loss)]
    let first = (hz * window.len() as f32 / RATE as f32) as usize;
    let total: f32 = power.iter().sum();
    Ok(power[first..].iter().sum::<f32>() / total)
}

#[test]
fn test_delay_low_pass_darkens_later_repeats() -> TestResult {
    let mut rig = Rig::new(&[
        WET[0],
        WET[1],
        ("feedback", 90.0),
        ("low-pass enabled", 1.0),
        ("low-pass", 1_000.0),
    ])?;
    let input = impulse(5 * TAP);
    let (left, _) = rig.render(&input, &input)?;
    let window = |k: usize| &left[k * TAP..k * TAP + 1024];
    let first = high_share(window(1), 4_000.0)?;
    let fourth = high_share(window(4), 4_000.0)?;
    // The first repeat is the impulse itself, flat; the fourth has passed the low-pass 3 times.
    assert!(first > 0.5, "{first}");
    assert!(fourth < first * 1e-3, "{first} {fourth}");
    Ok(())
}
