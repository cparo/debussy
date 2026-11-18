//! MIDI output: events leaving a routed event sink are queued with the host time they are due,
//! stop and pause queue a silence, and unrouted sinks queue nothing
//! (`docs/architecture/engine.md` "MIDI output").

mod common;

use std::error::Error;
use std::sync::Arc;

use audio_buffers::Layout;
use audio_engine::{Engine, Error as EngineError, MidiTrack, NoteSpan, Processor};
use common::{RATE, StreamControl, TestStreams};
use midi::{OutConsumer, Outgoing, out_channel};
use realtime_guard::{GuardAllocator, assert_realtime};
use signal_graph::{Config, Graph, NodeId};

#[global_allocator]
static ALLOCATOR: GuardAllocator = GuardAllocator;

const BLOCK: usize = 480; // 10 ms
const HOST: u64 = 7_000_000_000;
const PERIOD: u64 = 10_000_000;

struct Rig {
    engine: Engine,
    processor: Processor,
    sink: NodeId,
    control: Arc<StreamControl>,
    /// The host time of the next block's callback.
    host: u64,
    _graph: Graph,
}

impl Rig {
    /// A MIDI track's source feeding an event sink, the track playing one note at `start` to
    /// `end`.
    fn new(start: u64, end: u64) -> Result<Rig, Box<dyn Error>> {
        let (mut graph, runner) = Graph::new(Config::new(RATE, 512))?;
        let source = graph.add_event_source();
        let sink = graph.add_event_sink();
        let output = graph.add_sink(Layout::Stereo);
        graph.connect(source, 0, sink, 0)?;
        graph.commit()?;
        let (mut engine, processor) = Engine::new(runner, output, RATE)?;
        let note = NoteSpan {
            start,
            end,
            channel: 2,
            pitch: 60,
            velocity: 100,
        };
        engine.set_midi_tracks(vec![MidiTrack::new(source, vec![note], Vec::new())])?;
        Ok(Rig {
            engine,
            processor,
            sink,
            control: Arc::new(StreamControl::default()),
            host: HOST,
            _graph: graph,
        })
    }

    fn run(&mut self, blocks: usize) -> Result<(), Box<dyn Error>> {
        let mut left = vec![0.0; BLOCK];
        let mut right = vec![0.0; BLOCK];
        let processor = &mut self.processor;
        let host = &mut self.host;
        assert_realtime(|| -> Result<(), EngineError> {
            for _ in 0..blocks {
                processor.process(&[], 0, *host, &mut left, &mut right)?;
                *host += PERIOD;
            }
            Ok(())
        })?;
        Ok(())
    }

    fn play(&mut self) -> Result<(), Box<dyn Error>> {
        self.engine
            .play(&mut TestStreams(Arc::clone(&self.control)))?;
        Ok(())
    }
}

fn drain(rx: &mut OutConsumer) -> Vec<Outgoing> {
    std::iter::from_fn(|| rx.pop()).collect()
}

/// The host time and status byte of each message in `items`, and `None` for each silence.
fn stamps(items: &[Outgoing]) -> Vec<Option<(u64, u8)>> {
    items
        .iter()
        .map(|item| match item {
            Outgoing::Message {
                host_nanos,
                message,
            } => Some((*host_nanos, message.status())),
            Outgoing::Silence => None,
        })
        .collect()
}

#[test]
fn test_routed_sink_queues_events_at_their_host_time() -> Result<(), Box<dyn Error>> {
    // The note starts 120 frames into the second block and ends 40 frames into the third.
    let mut rig = Rig::new(600, 1000)?;
    let (tx, mut rx) = out_channel(64)?;
    rig.engine.route_midi_out(rig.sink, tx)?;
    rig.play()?;
    rig.run(4)?;
    assert_eq!(
        stamps(&drain(&mut rx)),
        [
            Some((HOST + PERIOD + 2_500_000, 0x92)),
            Some((HOST + 2 * PERIOD + 833_333, 0x82)),
        ]
    );
    Ok(())
}

#[test]
fn test_stop_and_pause_queue_a_silence() -> Result<(), Box<dyn Error>> {
    let mut rig = Rig::new(0, 100_000)?;
    let (tx, mut rx) = out_channel(64)?;
    rig.engine.route_midi_out(rig.sink, tx)?;
    rig.play()?;
    rig.run(2)?;
    rig.engine.pause()?;
    rig.run(1)?;
    let paused = drain(&mut rx);
    assert_eq!(
        paused
            .first()
            .map(|o| matches!(o, Outgoing::Message { .. })),
        Some(true)
    );
    assert_eq!(
        paused.iter().filter(|o| **o == Outgoing::Silence).count(),
        1
    );
    rig.play()?;
    rig.run(1)?;
    rig.engine.stop()?;
    rig.run(1)?;
    assert!(drain(&mut rx).contains(&Outgoing::Silence));
    Ok(())
}

#[test]
fn test_unrouted_sink_queues_nothing_and_retires_its_queue() -> Result<(), Box<dyn Error>> {
    let mut rig = Rig::new(0, 100_000)?;
    let (tx, mut rx) = out_channel(64)?;
    rig.engine.route_midi_out(rig.sink, tx)?;
    rig.run(1)?;
    rig.engine.unroute_midi_out(rig.sink)?;
    rig.play()?;
    rig.run(2)?;
    assert!(drain(&mut rx).is_empty());
    // The producer comes back to the control side to be dropped.
    rig.engine.collect();
    Ok(())
}

#[test]
fn test_full_queue_drops_and_counts() -> Result<(), Box<dyn Error>> {
    let mut rig = Rig::new(600, 1000)?;
    let (tx, mut rx) = out_channel(1)?;
    rig.engine.route_midi_out(rig.sink, tx)?;
    rig.play()?;
    rig.run(4)?;
    assert_eq!(drain(&mut rx).len(), 1);
    assert_eq!(rig.engine.midi_out_dropped(), 1);
    Ok(())
}
