//! The sampler run in a processing graph from an event source, every block inside the
//! real-time guard: repitching, one-shot and gated playback, velocity, zones and the end of a
//! sample (`docs/architecture/instruments.md` "Sampler").

use std::error::Error as StdError;
use std::f32::consts::TAU;
use std::sync::Arc;

use audio_buffers::Layout;
use instruments::{Playback, Zone, instrument, sampler};
use realtime_guard::{GuardAllocator, assert_realtime};
use signal_graph::{Config, Event, Graph, Message, Node, NodeId, Runner};

#[global_allocator]
static ALLOCATOR: GuardAllocator = GuardAllocator;

type TestResult = Result<(), Box<dyn StdError>>;

const RATE: u32 = 48_000;
const BLOCK: usize = 128;

/// An event source feeding one sampler, feeding a stereo sink.
struct Rig {
    _graph: Graph,
    runner: Runner,
    source: NodeId,
    sink: NodeId,
}

impl Rig {
    /// `node` with each `(name, value)` set before it is added.
    fn new(node: Box<dyn Node>, settings: &[(&str, f32)]) -> Result<Self, Box<dyn StdError>> {
        let (mut graph, runner) = Graph::new(Config::new(RATE, BLOCK))?;
        for parameter in node.parameters() {
            if let Some((_, value)) = settings.iter().find(|(n, _)| *n == parameter.info().name) {
                parameter.set(*value);
            }
        }
        let node = graph.add_node(node);
        let source = graph.add_event_source();
        let sink = graph.add_sink(Layout::Stereo);
        graph.connect(source, 0, node, 0)?;
        graph.connect(node, 0, sink, 0)?;
        graph.commit()?;
        Ok(Rig {
            _graph: graph,
            runner,
            source,
            sink,
        })
    }

    /// Renders `frames` frames (a whole number of blocks) with each event at its frame, and
    /// returns both channels.
    fn render(
        &mut self,
        frames: usize,
        events: &[(usize, Message)],
    ) -> Result<(Vec<f32>, Vec<f32>), Box<dyn StdError>> {
        let (mut left, mut right) = (vec![0.0; frames], vec![0.0; frames]);
        for (b, (l, r)) in left
            .chunks_mut(BLOCK)
            .zip(right.chunks_mut(BLOCK))
            .enumerate()
        {
            assert_realtime(|| self.block(b * BLOCK, l, r, events))?;
        }
        Ok((left, right))
    }

    fn block(
        &mut self,
        start: usize,
        left: &mut [f32],
        right: &mut [f32],
        events: &[(usize, Message)],
    ) -> Result<(), &'static str> {
        self.runner
            .begin_block(left.len())
            .map_err(|_| "block refused")?;
        let buffer = self
            .runner
            .event_source_mut(self.source)
            .ok_or("no event source")?;
        for &(frame, message) in events
            .iter()
            .filter(|(f, _)| (start..start + left.len()).contains(f))
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
        left.copy_from_slice(sink.channel(0).ok_or("no left")?);
        right.copy_from_slice(sink.channel(1).ok_or("no right")?);
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

/// The frequency of `signal` from its rising zero crossings, interpolated.
fn frequency(signal: &[f32]) -> f32 {
    let crossings: Vec<f32> = signal
        .windows(2)
        .enumerate()
        .filter(|(_, w)| w[0] < 0.0 && w[1] >= 0.0)
        .map(|(i, w)| i as f32 + w[0] / (w[0] - w[1]))
        .collect();
    let (Some(first), Some(last)) = (crossings.first(), crossings.last()) else {
        return 0.0;
    };
    (crossings.len() - 1) as f32 * RATE as f32 / (last - first)
}

fn sine(hz: f32, frames: usize) -> Arc<[f32]> {
    (0..frames)
        .map(|n| 0.5 * (TAU * hz * n as f32 / RATE as f32).sin())
        .collect()
}

fn constant(value: f32, frames: usize) -> Arc<[f32]> {
    vec![value; frames].into()
}

/// A mono zone over `low..=high` with root `root`.
fn zone(low: u8, high: u8, root: u8, playback: Playback, samples: Arc<[f32]>) -> Zone {
    Zone {
        low,
        high,
        root,
        playback,
        choke: None,
        tune: 0,
        left: samples,
        right: None,
    }
}

fn rig(zones: Vec<Zone>, settings: &[(&str, f32)]) -> Result<Rig, Box<dyn StdError>> {
    Rig::new(sampler(zones.into()).ok_or("no sampler")?, settings)
}

#[test]
fn test_sampler_repitches_an_octave_up_at_twice_the_rate() -> TestResult {
    let zones = vec![zone(
        0,
        127,
        60,
        Playback::Gated,
        sine(480.0, RATE as usize),
    )];
    let mut rig = rig(zones, &[])?;
    let (root, _) = rig.render(9_600, &[(0, on(60, 127)?)])?;
    let (up, _) = rig.render(9_600, &[(0, off(60)?), (4_800, on(72, 127)?)])?;
    assert!((frequency(&root[480..]) - 480.0).abs() < 1.0);
    assert!((frequency(&up[5_280..]) - 960.0).abs() < 2.0);
    Ok(())
}

#[test]
fn test_sampler_zone_tune_shifts_its_pitch_by_semitones() -> TestResult {
    // A one-key zone tuned up an octave plays its sample at twice the frequency on its own key.
    let mut tuned = zone(60, 60, 60, Playback::Gated, sine(480.0, RATE as usize));
    tuned.tune = 12;
    let mut rig = rig(vec![tuned], &[])?;
    let (out, _) = rig.render(9_600, &[(0, on(60, 127)?)])?;
    assert!((frequency(&out[480..]) - 960.0).abs() < 2.0);
    Ok(())
}

#[test]
fn test_sampler_one_shot_ignores_note_off_and_gated_releases() -> TestResult {
    let settings = [("amp release", 0.01)];
    for (playback, sounding) in [(Playback::OneShot, true), (Playback::Gated, false)] {
        let zones = vec![zone(60, 60, 60, playback, constant(0.5, RATE as usize))];
        let mut rig = rig(zones, &settings)?;
        let (out, _) = rig.render(12_800, &[(0, on(60, 127)?), (1_280, off(60)?)])?;
        let tail = peak(&out[9_600..]);
        assert_eq!(tail > 0.1, sounding, "{playback:?}: {tail}");
    }
    Ok(())
}

#[test]
fn test_sampler_choke_cuts_the_other_voice_in_its_group() -> TestResult {
    let hat = |key, choke| Zone {
        choke,
        ..zone(
            key,
            key,
            key,
            Playback::OneShot,
            constant(0.5, RATE as usize),
        )
    };
    for (group, cut) in [(Some(1), true), (None, false)] {
        // Open hi-hat on 46, closed on 42, sharing `group`; only 46 is heard (42 is quiet).
        let mut closed = hat(42, group);
        closed.left = constant(0.0, RATE as usize);
        let mut rig = rig(vec![hat(46, group), closed], &[])?;
        let (out, _) = rig.render(2_560, &[(0, on(46, 127)?), (1_280, on(42, 127)?)])?;
        // 3 ms is 144 frames; well after it the open hat is gone if cut.
        let tail = peak(&out[1_280 + 288..]);
        assert_eq!(tail == 0.0, cut, "{group:?}: {tail}");
    }
    Ok(())
}

#[test]
fn test_sampler_velocity_scales_the_gain() -> TestResult {
    let zones = || {
        vec![zone(
            60,
            60,
            60,
            Playback::OneShot,
            constant(0.5, RATE as usize),
        )]
    };
    let settings = [("amp velocity", 1.0)];
    let mut loud = rig(zones(), &settings)?;
    let mut soft = rig(zones(), &settings)?;
    let (a, _) = loud.render(1_280, &[(0, on(60, 127)?)])?;
    let (b, _) = soft.render(1_280, &[(0, on(60, 64)?)])?;
    let ratio = peak(&b[640..]) / peak(&a[640..]);
    assert!((ratio - 64.0 / 127.0).abs() < 0.01, "{ratio}");
    Ok(())
}

#[test]
fn test_sampler_note_outside_every_zone_is_silent() -> TestResult {
    let zones = vec![zone(36, 48, 36, Playback::OneShot, constant(0.5, 4_800))];
    let mut rig = rig(zones, &[])?;
    let (out, _) = rig.render(1_280, &[(0, on(60, 127)?)])?;
    assert_eq!(peak(&out), 0.0);
    Ok(())
}

#[test]
fn test_sampler_voice_frees_itself_at_the_end_of_the_sample() -> TestResult {
    let zones = vec![zone(0, 127, 60, Playback::OneShot, constant(0.5, 1_000))];
    let mut rig = rig(zones, &[("voices", 1.0)])?;
    let (out, _) = rig.render(2_560, &[(0, on(60, 127)?)])?;
    assert!(peak(&out[..900]) > 0.1);
    assert_eq!(peak(&out[1_100..]), 0.0);
    // The one voice is free again: a new note starts at once, with no steal fade.
    let (again, _) = rig.render(256, &[(0, on(62, 127)?)])?;
    assert!(peak(&again[..128]) > 0.1);
    Ok(())
}

#[test]
fn test_sampler_plays_stereo_and_copies_mono_to_both_sides() -> TestResult {
    let mut stereo = zone(60, 60, 60, Playback::OneShot, constant(0.5, 4_800));
    stereo.right = Some(constant(-0.25, 4_800));
    let mono = zone(62, 62, 62, Playback::OneShot, constant(0.5, 4_800));
    let zones = vec![stereo, mono];
    let settings = [("gain", 0.0), ("amp velocity", 0.0)];
    let (l, r) = rig(zones.clone(), &settings)?.render(1_280, &[(0, on(60, 127)?)])?;
    assert!((l[640] - 0.5).abs() < 1e-3 && (r[640] + 0.25).abs() < 1e-3);
    let (l, r) = rig(zones, &settings)?.render(1_280, &[(0, on(62, 127)?)])?;
    assert!(l[640] > 0.4);
    assert_eq!(l, r);
    Ok(())
}

#[test]
fn test_empty_sampler_is_silent() -> TestResult {
    let mut rig = Rig::new(instrument("sampler").ok_or("no sampler")?, &[])?;
    let (l, r) = rig.render(1_280, &[(0, on(60, 127)?)])?;
    assert_eq!(peak(&l) + peak(&r), 0.0);
    Ok(())
}
