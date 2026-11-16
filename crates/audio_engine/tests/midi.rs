//! MIDI clip playback: frame-accurate events, chase after a jump, note-offs on stop, pause and
//! seek, playback against offline render, and refused events.

mod common;

use std::error::Error;
use std::sync::Arc;

use audio_engine::{
    ControlPoint, Engine, Error as EngineError, MidiTrack, NoteSpan, Processor, render,
};
use common::{MidiGraph, ProbeLog, RATE, Seen, StreamControl, TestStreams, midi_graph};
use realtime_guard::{GuardAllocator, assert_realtime};
use signal_graph::{EVENT_CAPACITY, EventBuffer, NodeId};

#[global_allocator]
static ALLOCATOR: GuardAllocator = GuardAllocator;

const BLOCK: usize = 128;

fn note(start: u64, end: u64, channel: u8, pitch: u8) -> NoteSpan {
    NoteSpan {
        start,
        end,
        channel,
        pitch,
        velocity: 100,
    }
}

fn control(frame: u64, controller: u8, value: u8) -> ControlPoint {
    ControlPoint {
        frame,
        channel: 0,
        controller,
        value,
    }
}

/// The test track: two long overlapping notes and a controller that changes once in each.
fn long_notes() -> (Vec<NoteSpan>, Vec<ControlPoint>) {
    (
        vec![note(0, 10_000, 0, 60), note(5000, 20_000, 0, 64)],
        vec![control(100, 7, 10), control(6000, 7, 90)],
    )
}

/// What an event looks like to the probe, without its frame.
fn kind(e: &Seen) -> (u8, u8, u8) {
    (e.status, e.data1, e.data2)
}

/// An engine playing one MIDI track into a probe.
struct Rig {
    engine: Engine,
    processor: Processor,
    log: Arc<ProbeLog>,
    source: NodeId,
    control: Arc<StreamControl>,
    /// Frames processed so far, so the probe's frames can be told apart.
    processed: u64,
    _graph: signal_graph::Graph,
}

impl Rig {
    fn new(notes: Vec<NoteSpan>, controls: Vec<ControlPoint>) -> Result<Rig, Box<dyn Error>> {
        let MidiGraph {
            graph,
            runner,
            source,
            output,
            log,
        } = midi_graph()?;
        let (mut engine, processor) = Engine::new(runner, output, RATE)?;
        engine.set_midi_tracks(vec![MidiTrack::new(source, notes, controls)])?;
        Ok(Rig {
            engine,
            processor,
            log,
            source,
            control: Arc::new(StreamControl::default()),
            processed: 0,
            _graph: graph,
        })
    }

    fn streams(&self) -> TestStreams {
        TestStreams(Arc::clone(&self.control))
    }

    /// Runs `blocks` blocks under the real-time guard.
    fn run(&mut self, blocks: usize) -> Result<(), Box<dyn Error>> {
        let mut left = vec![0.0; BLOCK];
        let mut right = vec![0.0; BLOCK];
        let processor = &mut self.processor;
        assert_realtime(|| -> Result<(), EngineError> {
            for _ in 0..blocks {
                processor.process(&[], 0, 0, &mut left, &mut right)?;
            }
            Ok(())
        })?;
        self.processed += (blocks * BLOCK) as u64;
        Ok(())
    }

    /// Locates to `from` and plays, running one block so playback starts. Returns the probe frame
    /// of `from`.
    fn play_from(&mut self, from: u64) -> Result<u64, Box<dyn Error>> {
        self.engine.locate(from, &mut self.streams())?;
        self.engine.play(&mut self.streams())?;
        let start = self.processed;
        self.run(1)?;
        Ok(start)
    }
}

/// Every event `track` sends over `0..frames` in blocks of `block`, as (timeline frame, message).
fn fill_all(track: &mut MidiTrack, frames: u64, block: u32) -> Vec<(u64, signal_graph::Message)> {
    let mut events = EventBuffer::new(EVENT_CAPACITY);
    let mut seen = Vec::with_capacity(64);
    let mut position = 0;
    while position < frames {
        assert_realtime(|| {
            events.clear();
            track.fill(&mut events, position, block, true);
        });
        seen.extend(
            events
                .iter()
                .map(|e| (position + u64::from(e.frame), e.message)),
        );
        position += u64::from(block);
    }
    seen
}

#[test]
fn test_fill_places_events_at_exact_frames_for_any_block_size() -> Result<(), Box<dyn Error>> {
    let source = midi_graph()?.source;
    let notes = vec![
        note(100, 250, 0, 60),
        note(240, 900, 1, 62),
        note(250, 400, 0, 60),
    ];
    let controls = vec![control(240, 1, 5), control(100, 7, 64)];
    use signal_graph::Message as M;
    let expected = vec![
        (100, M::control_change(0, 7, 64)),
        (100, M::note_on(0, 60, 100)),
        (240, M::control_change(0, 1, 5)),
        (240, M::note_on(1, 62, 100)),
        (250, M::note_off(0, 60, 0)),
        (250, M::note_on(0, 60, 100)),
        (400, M::note_off(0, 60, 0)),
        (900, M::note_off(1, 62, 0)),
    ];
    let expected: Vec<_> = expected
        .into_iter()
        .map(|(f, m)| m.map(|m| (f, m)).ok_or("invalid message"))
        .collect::<Result<_, _>>()?;
    for block in [1, 7, 64, 150, 512] {
        let mut track = MidiTrack::new(source, notes.clone(), controls.clone());
        assert_eq!(fill_all(&mut track, 1024, block), expected, "block {block}");
    }
    Ok(())
}

#[test]
fn test_position_jump_is_treated_as_seek() -> Result<(), Box<dyn Error>> {
    let source = midi_graph()?.source;
    let mut track = MidiTrack::new(source, vec![note(0, 1000, 0, 60)], vec![control(10, 7, 99)]);
    let mut events = EventBuffer::new(EVENT_CAPACITY);
    assert_realtime(|| {
        track.fill(&mut events, 0, 128, true);
        events.clear();
        track.fill(&mut events, 128, 128, true);
    });
    assert!(events.is_empty());
    // Back to 50, as a loop would.
    assert_realtime(|| {
        events.clear();
        track.fill(&mut events, 50, 128, true);
    });
    use signal_graph::Message as M;
    let seen: Vec<_> = events.iter().map(|e| (e.frame, e.message)).collect();
    let expected: Vec<_> = [
        M::note_off(0, 60, 0),
        M::control_change(0, 7, 99),
        M::note_on(0, 60, 100),
    ]
    .into_iter()
    .map(|m| m.map(|m| (0, m)).ok_or("invalid message"))
    .collect::<Result<_, _>>()?;
    assert_eq!(seen, expected);
    Ok(())
}

#[test]
fn test_playback_and_offline_render_produce_identical_events() -> Result<(), Box<dyn Error>> {
    let (notes, controls) = long_notes();
    let (from, frames) = (5500, 16_000);
    let mut rig = Rig::new(notes.clone(), controls.clone())?;
    let start = rig.play_from(from)?;
    rig.run((frames as usize).div_ceil(BLOCK))?;
    let played: Vec<_> = rig
        .log
        .events()
        .into_iter()
        .filter(|e| e.frame >= start && e.frame < start + frames)
        .map(|e| (e.frame - start, kind(&e)))
        .collect();

    let mut g = midi_graph()?;
    let mut midi = vec![MidiTrack::new(g.source, notes, controls)];
    render(
        &mut g.runner,
        g.output,
        &mut [],
        &mut midi,
        from,
        frames,
        100,
        &mut |_, _| Ok(()),
    )?;
    let rendered: Vec<_> = g.log.events().iter().map(|e| (e.frame, kind(e))).collect();
    assert_eq!(played, rendered);
    // The chase at the start, then the control change and both note-offs.
    assert_eq!(rendered.len(), 6);
    Ok(())
}

#[test]
fn test_stop_sends_note_offs_at_once() -> Result<(), Box<dyn Error>> {
    let (notes, controls) = long_notes();
    let mut rig = Rig::new(notes, controls)?;
    rig.play_from(0)?;
    rig.run(10)?;
    assert!(matches!(
        rig.engine.set_midi_tracks(Vec::new()),
        Err(EngineError::Playing)
    ));
    rig.log.clear();
    rig.engine.stop()?;
    let at = rig.processed;
    rig.run(2)?;
    let seen = rig.log.events();
    assert_eq!(seen.len(), 1);
    assert!(seen[0].is_note_off() && seen[0].data1 == 60);
    assert_eq!(seen[0].frame, at);
    Ok(())
}

#[test]
fn test_pause_sends_note_offs_at_once() -> Result<(), Box<dyn Error>> {
    let (notes, controls) = long_notes();
    let mut rig = Rig::new(notes, controls)?;
    rig.play_from(5200)?;
    rig.log.clear();
    rig.engine.pause()?;
    let at = rig.processed;
    rig.run(2)?;
    let seen = rig.log.events();
    assert_eq!(seen.len(), 2);
    assert!(seen.iter().all(|e| e.is_note_off() && e.frame == at));
    Ok(())
}

#[test]
fn test_locate_while_playing_flushes_then_chases_notes_and_controls() -> Result<(), Box<dyn Error>>
{
    let (notes, controls) = long_notes();
    let mut rig = Rig::new(notes, controls)?;
    rig.play_from(0)?;
    rig.run(3)?;
    rig.log.clear();
    rig.engine.locate(8000, &mut rig.streams())?;
    let at = rig.processed;
    rig.run(1)?;
    let seen = rig.log.events();
    assert!(seen.iter().all(|e| e.frame == at));
    let kinds: Vec<_> = seen.iter().map(kind).collect();
    assert_eq!(
        kinds,
        vec![
            (0x80, 60, 0),
            (0xB0, 7, 90),
            (0x90, 60, 100),
            (0x90, 64, 100),
        ]
    );
    Ok(())
}

#[test]
fn test_play_from_mid_note_chases_note_and_last_control() -> Result<(), Box<dyn Error>> {
    let (notes, controls) = long_notes();
    let mut rig = Rig::new(notes, controls)?;
    let start = rig.play_from(5500)?;
    let seen = rig.log.events();
    assert!(seen.iter().all(|e| e.frame == start));
    let kinds: Vec<_> = seen.iter().map(kind).collect();
    assert_eq!(kinds, vec![(0xB0, 7, 10), (0x90, 60, 100), (0x90, 64, 100)]);
    Ok(())
}

#[test]
fn test_refused_note_offs_carry_to_next_block_and_count_overflow() -> Result<(), Box<dyn Error>> {
    const NOTES: usize = 600;
    // One note per key, starting one frame apart so no block holds more than the buffer takes.
    let notes = (0..NOTES)
        .map(|i| note(i as u64, 1_000_000, (i / 128) as u8, (i % 128) as u8))
        .collect();
    let mut rig = Rig::new(notes, Vec::new())?;
    rig.play_from(0)?;
    rig.run(NOTES / BLOCK + 1)?;
    assert_eq!(rig.log.events().len(), NOTES);
    assert_eq!(rig.engine.midi_overflows(), 0);
    rig.log.clear();
    rig.engine.stop()?;
    let at = rig.processed;
    rig.run(3)?;
    let seen = rig.log.events();
    assert!(seen.iter().all(Seen::is_note_off));
    let first = seen.iter().filter(|e| e.frame == at).count();
    let next = seen.iter().filter(|e| e.frame == at + BLOCK as u64).count();
    assert_eq!((first, next), (EVENT_CAPACITY, NOTES - EVENT_CAPACITY));
    assert_eq!(rig.engine.midi_overflows(), (NOTES - EVENT_CAPACITY) as u64);
    let mut keys: Vec<_> = seen.iter().map(|e| (e.status, e.data1)).collect();
    keys.sort_unstable();
    keys.dedup();
    assert_eq!(keys.len(), NOTES);
    Ok(())
}

#[test]
fn test_midi_tracks_replaced_while_paused_keep_pending_note_offs() -> Result<(), Box<dyn Error>> {
    let (notes, controls) = long_notes();
    let mut rig = Rig::new(notes, controls)?;
    rig.play_from(100)?;
    rig.run(2)?;
    rig.log.clear();
    rig.engine.pause()?;
    let source = rig.source;
    rig.engine
        .set_midi_tracks(vec![MidiTrack::new(source, Vec::new(), Vec::new())])?;
    let at = rig.processed;
    rig.run(2)?;
    let seen = rig.log.events();
    assert_eq!(seen.len(), 1);
    assert!(seen[0].is_note_off() && seen[0].data1 == 60 && seen[0].frame == at);
    Ok(())
}
