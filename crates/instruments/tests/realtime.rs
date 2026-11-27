//! The synthesizer run in a processing graph from an event source, every block inside the
//! real-time guard: note timing, release, voice stealing, pitch bend, velocity, reset and every
//! parameter's range; and the note display's preview against the synth it pictures.

use std::error::Error as StdError;

use audio_buffers::Layout;
use instruments::{MAX_VOICES, instrument, note_curve, parameter_infos};
use realtime_guard::{GuardAllocator, assert_realtime};
use signal_graph::{Config, Event, Graph, Message, NodeId, Parameter, Runner};

#[global_allocator]
static ALLOCATOR: GuardAllocator = GuardAllocator;

type TestResult = Result<(), Box<dyn StdError>>;

const RATE: u32 = 48_000;
const BLOCK: usize = 128;

/// An event source feeding one synth, feeding a stereo sink.
struct Rig {
    graph: Graph,
    runner: Runner,
    synth: NodeId,
    source: NodeId,
    sink: NodeId,
}

impl Rig {
    /// A synth with each `(name, value)` set before it is added.
    fn new(settings: &[(&str, f32)]) -> Result<Self, Box<dyn StdError>> {
        let (mut graph, runner) = Graph::new(Config::new(RATE, BLOCK))?;
        let node = instrument("synth").ok_or("no synth")?;
        for parameter in node.parameters() {
            if let Some((_, value)) = settings.iter().find(|(n, _)| *n == parameter.info().name) {
                parameter.set(*value);
            }
        }
        let synth = graph.add_node(node);
        let source = graph.add_event_source();
        let sink = graph.add_sink(Layout::Stereo);
        graph.connect(source, 0, synth, 0)?;
        graph.connect(synth, 0, sink, 0)?;
        graph.commit()?;
        Ok(Rig {
            graph,
            runner,
            synth,
            source,
            sink,
        })
    }

    fn parameter(&self, name: &str) -> Result<Parameter, Box<dyn StdError>> {
        Ok(self
            .graph
            .parameters(self.synth)
            .ok_or("no synth")?
            .iter()
            .find(|p| p.info().name == name)
            .cloned()
            .ok_or("no such parameter")?)
    }

    /// Renders `frames` frames (a whole number of blocks), with each event at its frame counted
    /// from the start of this render, and returns the left channel. Checks that both channels
    /// are the same.
    fn render(
        &mut self,
        frames: usize,
        events: &[(usize, Message)],
    ) -> Result<Vec<f32>, Box<dyn StdError>> {
        let mut out = vec![0.0; frames];
        for (b, chunk) in out.chunks_mut(BLOCK).enumerate() {
            let start = b * BLOCK;
            assert_realtime(|| self.block(start, chunk, events))?;
        }
        Ok(out)
    }

    fn block(
        &mut self,
        start: usize,
        out: &mut [f32],
        events: &[(usize, Message)],
    ) -> Result<(), &'static str> {
        self.runner
            .begin_block(out.len())
            .map_err(|_| "block refused")?;
        let buffer = self
            .runner
            .event_source_mut(self.source)
            .ok_or("no event source")?;
        for &(frame, message) in events
            .iter()
            .filter(|(f, _)| (start..start + out.len()).contains(f))
        {
            // Within one block of 128 frames.
            #[allow(clippy::cast_possible_truncation)]
            let frame = (frame - start) as u32;
            if !buffer.push(Event { frame, message }) {
                return Err("event source full");
            }
        }
        self.runner.process();
        let sink = self.runner.sink(self.sink).ok_or("no sink")?;
        let (left, right) = (
            sink.channel(0).ok_or("no left")?,
            sink.channel(1).ok_or("no right")?,
        );
        if left != right {
            return Err("left and right differ");
        }
        out.copy_from_slice(left);
        Ok(())
    }
}

fn on(note: u8, velocity: u8) -> Result<Message, &'static str> {
    Message::note_on(0, note, velocity).ok_or("bad note on")
}

fn off(note: u8) -> Result<Message, &'static str> {
    Message::note_off(0, note, 0).ok_or("bad note off")
}

fn peak(signal: &[f32]) -> f32 {
    signal.iter().fold(0.0, |m, s| m.max(s.abs()))
}

fn rms(signal: &[f32]) -> f32 {
    (signal.iter().map(|s| s * s).sum::<f32>() / signal.len().max(1) as f32).sqrt()
}

/// The frequency of `signal` from its rising zero crossings, interpolated.
fn frequency(signal: &[f32]) -> f32 {
    let crossings: Vec<f32> = signal
        .windows(2)
        .enumerate()
        .filter(|(_, w)| w[0] < 0.0 && w[1] >= 0.0)
        .map(|(i, w)| i as f32 + w[0] / (w[0] - w[1]))
        .collect();
    match (crossings.first(), crossings.last()) {
        (Some(first), Some(last)) if crossings.len() > 1 => {
            (crossings.len() - 1) as f32 * RATE as f32 / (last - first)
        }
        _ => 0.0,
    }
}

/// A plain patch: one saw, the filter open, no filter envelope, full sustain.
const PLAIN: &[(&str, f32)] = &[
    ("osc 2 level", 0.0),
    ("cutoff", 20_000.0),
    ("envelope amount", 0.0),
    ("amp attack", 0.001),
    ("amp sustain", 1.0),
    ("amp release", 0.05),
    ("gain", 0.0),
];

#[test]
fn test_note_on_sounds_from_its_frame_and_silence_before() -> TestResult {
    let mut rig = Rig::new(PLAIN)?;
    let at = 300;
    let out = rig.render(2 * BLOCK + 512, &[(at, on(60, 100)?)])?;
    assert!(out[..at].iter().all(|&s| s == 0.0), "sound before the note");
    assert!(peak(&out[at..at + 64]) > 0.01, "no sound after the note");
    Ok(())
}

#[test]
fn test_note_off_decays_to_silence_after_the_release() -> TestResult {
    let mut rig = Rig::new(PLAIN)?;
    let out = rig.render(4_096, &[(0, on(60, 100)?), (2_048, off(60)?)])?;
    assert!(rms(&out[1_024..2_048]) > 0.1);
    // 50 ms release covers 99% in 2 400 frames; well past it, nothing is left.
    let after = rig.render(9_600, &[])?;
    assert!(
        peak(&after[4_800..]) < 1e-4,
        "still sounding: {}",
        peak(&after[4_800..])
    );
    Ok(())
}

#[test]
fn test_seventeenth_note_steals_without_a_click() -> TestResult {
    let mut rig = Rig::new(&[
        ("voices", 16.0),
        ("osc 2 level", 0.0),
        ("cutoff", 2_000.0),
        ("envelope amount", 0.0),
        ("amp attack", 0.01),
        ("gain", -24.0),
    ])?;
    let mut events = Vec::new();
    for k in 0..=MAX_VOICES {
        // Low notes, one every 512 frames, all held.
        #[allow(clippy::cast_possible_truncation)]
        events.push((k * 512, on(36 + k as u8, 100)?));
    }
    let out = rig.render(512 * (MAX_VOICES + 2), &events)?;
    // The steal is at the 17th note; the largest sample-to-sample step around it stays near
    // what the sustained voices make on their own.
    let steal = 512 * MAX_VOICES;
    let step = |range: std::ops::Range<usize>| {
        out[range]
            .windows(2)
            .fold(0.0_f32, |m, w| m.max((w[1] - w[0]).abs()))
    };
    let before = step(steal - 512..steal);
    let around = step(steal..steal + 512);
    assert!(
        around < 1.5 * before,
        "step {around} around the steal, {before} before"
    );
    assert!(out.iter().all(|s| s.is_finite()));
    Ok(())
}

#[test]
fn test_pitch_bend_up_two_semitones_raises_the_frequency() -> TestResult {
    let mut rig = Rig::new(&[PLAIN, &[("osc 1 wave", 2.0)]].concat())?;
    let before = rig.render(9_600, &[(0, on(57, 100)?)])?;
    let bend = Message::pitch_bend(0, 16_383).ok_or("bad bend")?;
    let after = rig.render(9_600, &[(0, bend)])?;
    let (f0, f1) = (frequency(&before[2_400..]), frequency(&after[2_400..]));
    assert!((f0 - 220.0).abs() < 1.0, "{f0}");
    let ratio = f1 / f0;
    // 16 383 is one step short of the top: 2 · 8 191/8 192 semitones.
    let expected = (2.0 * 8_191.0 / 8_192.0 / 12.0_f32).exp2();
    assert!(
        (ratio - expected).abs() < 0.002,
        "{ratio} against {expected}"
    );
    Ok(())
}

#[test]
fn test_higher_velocity_is_louder() -> TestResult {
    let mut soft = Rig::new(PLAIN)?;
    let mut loud = Rig::new(PLAIN)?;
    let s = soft.render(4_096, &[(0, on(60, 30)?)])?;
    let l = loud.render(4_096, &[(0, on(60, 120)?)])?;
    assert!(rms(&l[1_024..]) > 1.5 * rms(&s[1_024..]));
    Ok(())
}

#[test]
fn test_reset_silences_at_once_and_releases_the_pedal() -> TestResult {
    let mut rig = Rig::new(&[("amp release", 5.0)])?;
    let pedal = Message::control_change(0, 64, 127).ok_or("bad pedal")?;
    rig.render(2_048, &[(0, pedal), (0, on(60, 100)?), (100, off(60)?)])?;
    rig.runner.reset();
    let out = rig.render(1_024, &[])?;
    assert_eq!(peak(&out), 0.0);
    // The pedal is up after the reset: a new note's release is not held.
    // Held by the pedal it would stay at the 0.8 sustain; released over 5 s, a second in, it
    // is near 0.32 of the peak.
    let out = rig.render(48_000, &[(0, on(62, 100)?), (512, off(62)?)])?;
    assert!(peak(&out[out.len() - 1_024..]) < 0.6 * peak(&out[..1_024]));
    Ok(())
}

#[test]
fn test_every_parameter_at_its_limits_plays_finite_bounded_audio() -> TestResult {
    let rig = Rig::new(&[])?;
    let infos: Vec<_> = rig
        .graph
        .parameters(rig.synth)
        .ok_or("no synth")?
        .iter()
        .map(|p| p.info().clone())
        .collect();
    drop(rig);
    let chord = [
        (0, on(48, 127)?),
        (0, on(67, 64)?),
        (0, on(96, 1)?),
        (3_000, off(48)?),
    ];
    for info in &infos {
        for value in [info.min, info.max] {
            let mut rig = Rig::new(&[(&info.name, value)])?;
            rig.parameter(&info.name)?.set(value);
            let out = rig.render(6_144, &chord)?;
            assert!(out.iter().all(|s| s.is_finite()), "{} = {value}", info.name);
            assert!(
                peak(&out) < 64.0,
                "{} = {value}: peak {}",
                info.name,
                peak(&out)
            );
            rig.runner.reset();
            assert_eq!(
                peak(&rig.render(BLOCK, &[])?),
                0.0,
                "{} = {value}",
                info.name
            );
        }
    }
    Ok(())
}

#[test]
fn test_filter_order_change_takes_effect_from_rest() -> TestResult {
    let mut rig = Rig::new(PLAIN)?;
    let order = rig.parameter("filter order")?;
    // Stopped-only: the UI changes it only while stopped, but a held note may still sound.
    for value in [4.0, 1.0, 3.0, 2.0] {
        order.set(value);
        let out = rig.render(2_048, &[(0, on(60, 100)?)])?;
        assert!(out.iter().all(|s| s.is_finite()), "order {value}");
        assert!(rms(&out[1_024..]) > 0.01, "order {value}: silent");
    }
    Ok(())
}

#[test]
fn test_parameters_change_while_playing_without_a_jump() -> TestResult {
    let mut rig = Rig::new(&[("gain", -6.0)])?;
    rig.render(4_096, &[(0, on(60, 100)?)])?;
    let before = rig.render(1_024, &[])?;
    rig.parameter("cutoff")?.set(300.0);
    rig.parameter("gain")?.set(-30.0);
    let after = rig.render(2_048, &[])?;
    // Over the 10 ms ramp no step is larger than the steady note's own largest.
    let largest = |s: &[f32]| {
        s.windows(2)
            .fold(0.0_f32, |m, w| m.max((w[1] - w[0]).abs()))
    };
    let edge = largest(&[&before[1_023..], &after[..480]].concat());
    let typical = largest(&before);
    assert!(
        edge <= typical + 1e-4,
        "step {edge} while ramping, {typical} steady"
    );
    assert!(rms(&after[1_024..]) < 0.25 * rms(&before));
    Ok(())
}

/// The synth's default values with each `(name, value)` of `settings` set, in declaration order.
fn values(settings: &[(&str, f32)]) -> Result<Vec<f32>, Box<dyn StdError>> {
    Ok(parameter_infos("synth")
        .ok_or("no synth")?
        .iter()
        .map(|info| {
            settings
                .iter()
                .find(|(n, _)| *n == info.name)
                .map_or(info.default, |&(_, v)| v)
        })
        .collect())
}

#[test]
fn test_note_curve_matches_the_live_synth() -> TestResult {
    // The LFO on the cutoff, a pulse and the 24 dB order, and settings the preview ignores.
    let settings = [
        ("lfo depth", 0.5),
        ("lfo destination", 1.0),
        ("lfo shape", 5.0),
        ("osc 1 wave", 1.0),
        ("filter order", 4.0),
        ("voices", 3.0),
        ("amp velocity", 0.2),
        ("amp release", 0.1),
    ];
    let curve = note_curve("synth", &values(&settings)?, RATE).ok_or("no curve")?;
    let note = curve.render();
    let samples = note.samples();
    // Under a second at 48 kHz.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let gate_off = (curve.gate_off() * RATE as f32).round() as usize;
    let mut rig = Rig::new(&settings)?;
    let frames = samples.len().div_ceil(BLOCK) * BLOCK;
    let out = rig.render(frames, &[(0, on(69, 127)?), (gate_off, off(69)?)])?;
    assert!(peak(samples) > 0.01);
    assert_eq!(&out[..samples.len()], samples);
    Ok(())
}

#[test]
fn test_note_curve_leaves_a_live_synth_unchanged() -> TestResult {
    let (mut with, mut without) = (Rig::new(PLAIN)?, Rig::new(PLAIN)?);
    let events = [(0, on(60, 100)?)];
    assert_eq!(
        with.render(1_024, &events)?,
        without.render(1_024, &events)?
    );
    let curve = note_curve("synth", &values(PLAIN)?, RATE).ok_or("no curve")?;
    let _note = curve.render();
    assert_eq!(with.render(1_024, &[])?, without.render(1_024, &[])?);
    Ok(())
}
