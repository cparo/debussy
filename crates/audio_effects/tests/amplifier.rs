//! The amplifier run in a processing graph, every block inside the real-time guard: the harmonics
//! bias adds, the DC filter, aliasing with and without oversampling, and its latency.

use std::error::Error as StdError;
use std::f32::consts::TAU;

use audio_buffers::Layout;
use audio_effects::effect;
use dsp::{Complex, RealFft};
use realtime_guard::{GuardAllocator, assert_realtime};
use signal_graph::{Config, Graph, NodeId, Runner};

#[global_allocator]
static ALLOCATOR: GuardAllocator = GuardAllocator;

type TestResult = Result<(), Box<dyn StdError>>;

const RATE: u32 = 48_000;
const BLOCK: usize = 128;
/// The analysis length; tones on its bins have whole cycles in it.
const SIZE: usize = 8192;

/// A graph of one amplifier node between a stereo source and sink.
struct Rig {
    graph: Graph,
    runner: Runner,
    source: NodeId,
    sink: NodeId,
}

impl Rig {
    /// An amplifier with the named parameters set before it is prepared.
    fn new(values: &[(&str, f32)]) -> Result<Self, Box<dyn StdError>> {
        let node = effect("amplifier").ok_or("unknown kind")?;
        for parameter in node.parameters() {
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

/// One second of a sine on analysis bin `bin`, at `amplitude`.
fn sine(bin: usize, amplitude: f32) -> Vec<f32> {
    (0..RATE as usize)
        .map(|n| amplitude * (TAU * (bin * n % SIZE) as f32 / SIZE as f32).sin())
        .collect()
}

/// The power in each bin of the last [`SIZE`] samples of `signal`.
fn spectrum(signal: &[f32]) -> Result<Vec<f32>, Box<dyn StdError>> {
    let mut fft = RealFft::new(SIZE)?;
    let mut bins = vec![Complex::default(); fft.bins()];
    fft.forward(&signal[signal.len() - SIZE..], &mut bins)?;
    Ok(bins.iter().map(|c| c.abs() * c.abs()).collect())
}

fn db(power_ratio: f32) -> f32 {
    10.0 * power_ratio.log10()
}

#[test]
fn test_symmetric_shaping_adds_odd_harmonics_and_bias_adds_even() -> TestResult {
    for shape in [0.0, 1.0, 2.0] {
        let input = sine(64, 0.5);
        let plain = spectrum(&Rig::new(&[("drive", 12.0), ("shape", shape)])?.render(&input)?)?;
        let biased = spectrum(
            &Rig::new(&[("drive", 12.0), ("shape", shape), ("bias", 0.3)])?.render(&input)?,
        )?;
        let fundamental = plain[64];
        // Third harmonic present either way; second only with bias.
        assert!(db(plain[192] / fundamental) > -40.0, "shape {shape}");
        assert!(db(plain[128] / fundamental) < -90.0, "shape {shape}");
        assert!(db(biased[128] / biased[64]) > -40.0, "shape {shape}");
    }
    Ok(())
}

#[test]
fn test_dc_filter_removes_the_bias_offset() -> TestResult {
    let mut rig = Rig::new(&[("drive", 12.0), ("bias", 0.5), ("shape", 1.0)])?;
    let output = rig.render(&sine(64, 0.5))?;
    let tail = &output[output.len() - SIZE..];
    let mean = tail.iter().sum::<f32>() / SIZE as f32;
    assert!(mean.abs() < 1e-3, "{mean}");
    let silence = Rig::new(&[("bias", 0.8)])?.render(&vec![0.0; RATE as usize])?;
    assert!(
        silence[silence.len() - SIZE..]
            .iter()
            .all(|x| x.abs() < 1e-4)
    );
    Ok(())
}

#[test]
fn test_oversampling_lowers_aliasing() -> TestResult {
    // 15 kHz: every harmonic is above Nyquist, so all energy off the fundamental is aliasing.
    let bin = 2560;
    let input = sine(bin, 0.5);
    let aliased = |oversampling: f32| -> Result<f32, Box<dyn StdError>> {
        let output = Rig::new(&[
            ("drive", 18.0),
            ("shape", 1.0),
            ("oversampling", oversampling),
        ])?
        .render(&input)?;
        let power = spectrum(&output)?;
        let total: f32 = power.iter().sum();
        Ok(db((total - power[bin]) / power[bin]))
    };
    let (off, twice, four) = (aliased(0.0)?, aliased(1.0)?, aliased(2.0)?);
    assert!(twice < off - 5.0, "off {off} dB, 2× {twice} dB");
    assert!(four < off - 30.0, "off {off} dB, 4× {four} dB");
    Ok(())
}

#[test]
fn test_latency_is_the_oversamplers() -> TestResult {
    assert_eq!(Rig::new(&[])?.graph.latency(), 0);
    for oversampling in [1.0, 2.0] {
        let mut rig = Rig::new(&[("oversampling", oversampling)])?;
        let latency = rig.graph.latency();
        assert!(latency > 0);
        // A small click comes out `latency` frames late, at its own size.
        let mut input = vec![0.0; 4096];
        input[1000] = 0.01;
        let output = rig.render(&input)?;
        let peak = output
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.abs().total_cmp(&b.1.abs()))
            .map(|(i, _)| i);
        assert_eq!(peak, Some(1000 + latency), "{oversampling}");
    }
    Ok(())
}
