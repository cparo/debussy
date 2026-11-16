//! Armed MIDI tracks: live input monitored into the event source, clips muted while recording,
//! the take's frames across a count-in, and live notes released on stop and disarm, with the
//! real-time side under the guard (`docs/architecture/recording.md` "MIDI recording").

mod common;

use std::error::Error;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use audio_engine::{ArmedMidi, Engine, MidiTrack, NoteSink, NoteSpan, Processor};
use common::{ProbeLog, RATE, Seen, StreamControl, TestStreams, midi_graph};
use midi::{LIVE_QUEUE_CAPACITY, LiveProducer, Message, TimedMessage};
use realtime_guard::{GuardAllocator, assert_realtime};
use signal_graph::NodeId;

#[global_allocator]
static ALLOCATOR: GuardAllocator = GuardAllocator;

const BLOCK: usize = 128;
/// A block's length in host nanoseconds, as the live input reckons it.
const PERIOD: u64 = BLOCK as u64 * 1_000_000_000 / RATE as u64;
/// Slightly more than one frame in nanoseconds, so a frame offset survives rounding down.
const FRAME_NANOS: u64 = 20_834;
/// The first block's host time.
const BASE: u64 = 1_000_000_000;

/// What a [`MemoryNotes`] sink captured, read by the test. Writing allocates and locks nothing.
#[derive(Default)]
struct NoteStore {
    events: Vec<AtomicU64>,
    len: AtomicUsize,
    /// The take's length once ended, plus one; 0 until then.
    ended: AtomicU64,
}

impl NoteStore {
    /// Every (take frame, status, note) captured.
    fn events(&self) -> Vec<(u64, u8, u8)> {
        self.events[..self.len.load(Ordering::SeqCst)]
            .iter()
            .map(|e| {
                let e = e.load(Ordering::SeqCst);
                (e >> 16, (e >> 8) as u8, e as u8)
            })
            .collect()
    }

    fn ended(&self) -> Option<u64> {
        self.ended.load(Ordering::SeqCst).checked_sub(1)
    }
}

struct MemoryNotes(Arc<NoteStore>);

impl NoteSink for MemoryNotes {
    fn write(&mut self, frame: u64, message: Message) -> bool {
        let store = &self.0;
        let len = store.len.load(Ordering::SeqCst);
        let Some(slot) = store.events.get(len) else {
            return false;
        };
        let bytes = message.bytes();
        slot.store(
            (frame << 16) | (u64::from(bytes[0]) << 8) | u64::from(bytes[1]),
            Ordering::SeqCst,
        );
        store.len.store(len + 1, Ordering::SeqCst);
        true
    }

    fn end(&mut self, frames: u64) {
        let _ = self
            .0
            .ended
            .compare_exchange(0, frames + 1, Ordering::SeqCst, Ordering::SeqCst);
    }
}

fn notes_sink() -> (Box<dyn NoteSink>, Arc<NoteStore>) {
    let store = Arc::new(NoteStore {
        events: (0..1024).map(|_| AtomicU64::new(0)).collect(),
        ..NoteStore::default()
    });
    (Box::new(MemoryNotes(Arc::clone(&store))), store)
}

/// An engine with one MIDI track's event source into a probe, and a live input to play.
struct Rig {
    engine: Engine,
    processor: Processor,
    log: Arc<ProbeLog>,
    source: NodeId,
    keys: LiveProducer,
    activity: Arc<AtomicU64>,
    control: Arc<StreamControl>,
    /// Blocks run so far: block `k` runs at host time `BASE + k × PERIOD`.
    blocks: u64,
    _graph: signal_graph::Graph,
}

impl Rig {
    /// The rig with the track armed, monitored if `monitor`, with no take.
    fn new(monitor: bool) -> Result<Rig, Box<dyn Error>> {
        let g = midi_graph()?;
        let (mut engine, processor) = Engine::new(g.runner, g.output, RATE)?;
        let (keys, input) = midi::channel(LIVE_QUEUE_CAPACITY)?;
        let activity = Arc::new(AtomicU64::new(0));
        engine.arm(
            Vec::new(),
            vec![ArmedMidi::new(
                g.source,
                Some(input),
                monitor,
                None,
                Arc::clone(&activity),
            )],
        )?;
        Ok(Rig {
            engine,
            processor,
            log: g.log,
            source: g.source,
            keys,
            activity,
            control: Arc::new(StreamControl::default()),
            blocks: 0,
            _graph: g.graph,
        })
    }

    /// Re-arms with the input carried over, `monitor`, and `sink`.
    fn rearm(
        &mut self,
        monitor: bool,
        sink: Option<Box<dyn NoteSink>>,
    ) -> Result<(), Box<dyn Error>> {
        let track = ArmedMidi::new(self.source, None, monitor, sink, Arc::clone(&self.activity));
        self.engine.arm(Vec::new(), vec![track])?;
        Ok(())
    }

    fn host(&self, block: u64) -> u64 {
        BASE + block * PERIOD
    }

    /// Plays `message` so that it lands at frame `frame` of block `block` (a whole block).
    fn press(&mut self, block: u64, frame: u64, message: Message) -> Result<(), Box<dyn Error>> {
        let host_nanos = self.host(block) - PERIOD + frame * FRAME_NANOS;
        self.at(host_nanos, message)
    }

    /// Plays `message` at host time `host_nanos`.
    fn at(&mut self, host_nanos: u64, message: Message) -> Result<(), Box<dyn Error>> {
        if !self.keys.push(TimedMessage {
            host_nanos,
            message,
        }) {
            return Err("live queue full".into());
        }
        Ok(())
    }

    /// Runs `blocks` blocks under the guard.
    fn run(&mut self, blocks: u64) -> Result<(), Box<dyn Error>> {
        let (mut left, mut right) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
        for _ in 0..blocks {
            let host = self.host(self.blocks);
            let processor = &mut self.processor;
            assert_realtime(|| processor.process(&[], 0, host, &mut left, &mut right))?;
            self.blocks += 1;
        }
        Ok(())
    }

    fn streams(&self) -> TestStreams {
        TestStreams(Arc::clone(&self.control))
    }

    /// The probe's events since the last call, as (frame, status, note).
    fn seen(&self) -> Vec<(u64, u8, u8)> {
        let seen = self
            .log
            .events()
            .iter()
            .map(|e: &Seen| (e.frame, e.status, e.data1))
            .collect();
        self.log.clear();
        seen
    }
}

fn on(note: u8) -> Result<Message, Box<dyn Error>> {
    Ok(Message::note_on(0, note, 100).ok_or("bad note")?)
}

fn off(note: u8) -> Result<Message, Box<dyn Error>> {
    Ok(Message::note_off(0, note, 0).ok_or("bad note")?)
}

#[test]
fn test_monitored_input_reaches_the_event_source_at_its_frame() -> Result<(), Box<dyn Error>> {
    let mut rig = Rig::new(true)?;
    rig.press(1, 10, on(60)?)?;
    rig.press(1, 50, off(60)?)?;
    rig.run(2)?;
    let block = BLOCK as u64;
    assert_eq!(rig.seen(), [(block + 10, 0x90, 60), (block + 50, 0x80, 60)]);
    assert_eq!(rig.activity.load(Ordering::SeqCst), 2);
    Ok(())
}

#[test]
fn test_unmonitored_input_is_drained_but_not_heard() -> Result<(), Box<dyn Error>> {
    let mut rig = Rig::new(false)?;
    for _ in 0..LIVE_QUEUE_CAPACITY {
        rig.press(1, 5, on(60)?)?;
    }
    rig.run(4)?;
    assert!(rig.seen().is_empty());
    // The queue was drained, so more input fits.
    rig.press(5, 5, on(61)?)?;
    rig.rearm(true, None)?;
    rig.run(2)?;
    assert_eq!(rig.seen(), [(5 * BLOCK as u64 + 5, 0x90, 61)]);
    Ok(())
}

#[test]
fn test_clips_are_merged_while_playing_and_muted_while_recording() -> Result<(), Box<dyn Error>> {
    let mut rig = Rig::new(true)?;
    let clip = NoteSpan {
        start: 0,
        end: 1_000_000,
        channel: 0,
        pitch: 40,
        velocity: 90,
    };
    rig.engine
        .set_midi_tracks(vec![MidiTrack::new(rig.source, vec![clip], Vec::new())])?;
    rig.engine.play(&mut rig.streams())?;
    rig.press(1, 3, on(60)?)?;
    rig.run(2)?;
    let notes: Vec<u8> = rig.seen().iter().map(|e| e.2).collect();
    assert_eq!(notes, [40, 60], "the clip and the live note both sound");

    rig.engine.stop()?;
    rig.run(1)?;
    rig.seen();
    let (sink, _) = notes_sink();
    rig.rearm(true, Some(sink))?;
    rig.engine.record(&mut rig.streams(), 300)?;
    rig.press(rig.blocks + 1, 3, on(62)?)?;
    rig.press(rig.blocks + 4, 3, off(62)?)?;
    rig.run(6)?;
    let seen = rig.seen();
    assert!(
        seen.iter().all(|e| e.2 != 40),
        "the clip is silent through the count-in and the take: {seen:?}"
    );
    assert_eq!(seen.iter().map(|e| e.2).collect::<Vec<_>>(), [62, 62]);
    Ok(())
}

#[test]
fn test_take_frames_count_from_the_capture_start_across_a_count_in() -> Result<(), Box<dyn Error>> {
    let mut rig = Rig::new(true)?;
    let (sink, take) = notes_sink();
    rig.rearm(true, Some(sink))?;
    // Block 0 counts in; block 1 counts in for 72 frames, then captures 56.
    rig.engine.record(&mut rig.streams(), 200)?;
    let h1 = rig.host(1);
    let counted = 72 * 1_000_000_000 / u64::from(RATE);
    // Heard during the count-in, at its frame 5: not in the take.
    rig.at(h1 - counted + 5 * FRAME_NANOS, on(60)?)?;
    // The captured part's window starts 56 frames before its own host time.
    let window = h1 + counted - 56 * 1_000_000_000 / u64::from(RATE);
    rig.at(window + 10 * FRAME_NANOS, off(60)?)?;
    rig.run(2)?;
    // Block 2's window is the host time between blocks 1 and 2.
    rig.press(2, 96, on(64)?)?;
    rig.run(1)?;
    assert_eq!(take.events(), [(10, 0x80, 60), (56 + 96, 0x90, 64)]);
    let frames = 56 + BLOCK as u64;
    let block = BLOCK as u64;
    assert_eq!(
        rig.seen(),
        [
            (block + 5, 0x90, 60),
            (block + 72 + 10, 0x80, 60),
            (2 * block + 96, 0x90, 64)
        ]
    );
    rig.engine.stop()?;
    rig.run(1)?;
    assert_eq!(take.ended(), Some(frames));
    Ok(())
}

#[test]
fn test_stop_and_disarm_release_held_notes() -> Result<(), Box<dyn Error>> {
    let mut rig = Rig::new(true)?;
    rig.engine.play(&mut rig.streams())?;
    rig.press(1, 0, on(60)?)?;
    rig.press(1, 1, on(61)?)?;
    rig.press(1, 2, off(61)?)?;
    rig.run(2)?;
    rig.seen();
    rig.engine.stop()?;
    rig.run(1)?;
    assert_eq!(rig.seen(), [(2 * BLOCK as u64, 0x80, 60)]);

    rig.press(4, 0, on(62)?)?;
    rig.run(2)?;
    rig.seen();
    rig.engine.arm(Vec::new(), Vec::new())?;
    rig.run(1)?;
    assert_eq!(rig.seen(), [(5 * BLOCK as u64, 0x80, 62)]);
    // Released once only.
    rig.run(1)?;
    assert!(rig.seen().is_empty());
    Ok(())
}

#[test]
fn test_rearming_keeps_the_input_and_held_notes() -> Result<(), Box<dyn Error>> {
    let mut rig = Rig::new(true)?;
    rig.press(1, 0, on(60)?)?;
    rig.run(2)?;
    rig.seen();
    rig.rearm(true, None)?;
    rig.press(3, 7, off(60)?)?;
    rig.run(2)?;
    assert_eq!(rig.seen(), [(3 * BLOCK as u64 + 7, 0x80, 60)]);
    // Nothing is held, so disarming sends nothing.
    rig.engine.arm(Vec::new(), Vec::new())?;
    rig.run(1)?;
    assert!(rig.seen().is_empty());
    Ok(())
}
