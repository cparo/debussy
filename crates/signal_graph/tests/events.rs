//! Event ports: typed connections, fan-out, merging, delay compensation and overflow, run inside
//! the real-time guard.

mod support;

use std::error::Error as StdError;
use std::sync::Arc;

use audio_buffers::Layout;
use modulation::{Curve, Point};
use realtime_guard::{GuardAllocator, assert_realtime};
use signal_graph::{
    Config, Error, Event, Graph, Inputs, Message, Node, NodeId, Outputs, Port, Runner,
};
use support::{EventPass, Gain, Latency};

#[global_allocator]
static ALLOCATOR: GuardAllocator = GuardAllocator;

type TestResult = Result<(), Box<dyn StdError>>;

const RATE: u32 = 48_000;

/// A note-on of `note` at `frame`.
fn note(frame: u32, note: u8) -> Result<Event, &'static str> {
    Ok(Event {
        frame,
        message: Message::note_on(0, note, 100).ok_or("bad note")?,
    })
}

/// Runs one block of `frames` inside the real-time guard, first pushing each source's events.
fn block(
    runner: &mut Runner,
    frames: usize,
    sources: &[(NodeId, &[Event])],
) -> Result<(), &'static str> {
    assert_realtime(|| {
        runner.begin_block(frames).map_err(|_| "block refused")?;
        for &(source, events) in sources {
            let buffer = runner.event_source_mut(source).ok_or("no event source")?;
            for &event in events {
                if !buffer.push(event) {
                    return Err("event source full");
                }
            }
        }
        runner.process();
        Ok(())
    })
}

/// The `(frame, note)` of every event at `sink` this block.
fn notes(runner: &Runner, sink: NodeId) -> Result<Vec<(u32, u8)>, &'static str> {
    Ok(runner
        .event_sink(sink)
        .ok_or("no event sink")?
        .iter()
        .map(|e| (e.frame, e.message.bytes()[1]))
        .collect())
}

#[test]
fn test_connect_refuses_event_to_audio_or_control() -> TestResult {
    let (mut graph, _runner) = Graph::new(Config::new(RATE, 128))?;
    let events = graph.add_event_source();
    let event_sink = graph.add_event_sink();
    let audio = graph.add_source(Layout::Mono);
    let audio_sink = graph.add_sink(Layout::Mono);
    let curve = graph.add_curve(Arc::new(Curve::new(vec![Point::new(0, 0.5)])?));
    assert_eq!(
        graph.connect(events, 0, audio_sink, 0),
        Err(Error::PortTypeMismatch)
    );
    assert_eq!(
        graph.connect(audio, 0, event_sink, 0),
        Err(Error::PortTypeMismatch)
    );
    assert_eq!(
        graph.connect(curve, 0, event_sink, 0),
        Err(Error::PortTypeMismatch)
    );
    graph.connect(events, 0, event_sink, 0)?;
    Ok(())
}

#[test]
fn test_connect_refuses_an_event_cycle() -> TestResult {
    let (mut graph, _runner) = Graph::new(Config::new(RATE, 128))?;
    let a = graph.add_node(Box::new(EventPass::new(0)));
    let b = graph.add_node(Box::new(EventPass::new(0)));
    graph.connect(a, 0, b, 0)?;
    assert_eq!(graph.connect(b, 0, a, 0), Err(Error::CycleWithoutDelay));
    Ok(())
}

/// A node with one audio input and output, to replace an event node with.
struct AudioOnly;

impl Node for AudioOnly {
    fn inputs(&self) -> Vec<Port> {
        vec![Port::audio("in", Layout::Mono)]
    }
    fn outputs(&self) -> Vec<Port> {
        vec![Port::audio("out", Layout::Mono)]
    }
    fn process(&mut self, _inputs: &Inputs<'_>, _outputs: &mut Outputs<'_>) {}
}

#[test]
fn test_replace_node_keeps_event_connections_of_matching_type() -> TestResult {
    let (mut graph, _runner) = Graph::new(Config::new(RATE, 128))?;
    let source = graph.add_event_source();
    let node = graph.add_node(Box::new(EventPass::new(0)));
    let sink = graph.add_event_sink();
    let into = graph.connect(source, 0, node, 0)?;
    let out = graph.connect(node, 0, sink, 0)?;
    assert!(
        graph
            .replace_node(node, Box::new(EventPass::new(5)))?
            .is_empty()
    );
    let mut dropped = graph.replace_node(node, Box::new(AudioOnly))?;
    dropped.sort();
    assert_eq!(dropped, [into, out]);
    Ok(())
}

#[test]
fn test_events_fan_out_and_pass_through_a_node() -> TestResult {
    let (mut graph, mut runner) = Graph::new(Config::new(RATE, 128))?;
    let source = graph.add_event_source();
    let pass = graph.add_node(Box::new(EventPass::new(0)));
    let direct = graph.add_event_sink();
    let through = graph.add_event_sink();
    graph.connect(source, 0, direct, 0)?;
    graph.connect(source, 0, pass, 0)?;
    graph.connect(pass, 0, through, 0)?;
    graph.commit()?;
    block(&mut runner, 64, &[(source, &[note(3, 60)?, note(40, 62)?])])?;
    assert_eq!(notes(&runner, direct)?, [(3, 60), (40, 62)]);
    assert_eq!(notes(&runner, through)?, notes(&runner, direct)?);
    block(&mut runner, 64, &[])?;
    assert!(notes(&runner, direct)?.is_empty() && notes(&runner, through)?.is_empty());
    Ok(())
}

#[test]
fn test_event_inputs_merge_in_frame_then_connection_order() -> TestResult {
    let (mut graph, mut runner) = Graph::new(Config::new(RATE, 128))?;
    let first = graph.add_event_source();
    let second = graph.add_event_source();
    let sink = graph.add_event_sink();
    graph.connect(first, 0, sink, 0)?;
    graph.connect(second, 0, sink, 0)?;
    graph.commit()?;
    block(
        &mut runner,
        64,
        &[
            (second, &[note(3, 20)?, note(5, 21)?]),
            (first, &[note(5, 10)?, note(10, 11)?]),
        ],
    )?;
    assert_eq!(notes(&runner, sink)?, [(3, 20), (5, 10), (5, 21), (10, 11)]);
    Ok(())
}

#[test]
fn test_merge_of_compensated_and_uncompensated_connections() -> TestResult {
    let (mut graph, mut runner) = Graph::new(Config::new(RATE, 128))?;
    let late = graph.add_event_source();
    let early = graph.add_event_source();
    let pass = graph.add_node(Box::new(EventPass::new(20)));
    let sink = graph.add_event_sink();
    graph.connect(late, 0, pass, 0)?;
    graph.connect(pass, 0, sink, 0)?;
    graph.connect(early, 0, sink, 0)?;
    graph.commit()?;
    assert_eq!(graph.latency(), 20);
    block(
        &mut runner,
        32,
        &[
            (late, &[note(25, 1)?]),
            (early, &[note(5, 2)?, note(20, 3)?]),
        ],
    )?;
    // `early` is delayed 20 frames to line up with `late`, which `pass` reports but does not
    // delay: the tie at frame 25 keeps connection order.
    assert_eq!(notes(&runner, sink)?, [(25, 1), (25, 2)]);
    block(&mut runner, 32, &[])?;
    assert_eq!(notes(&runner, sink)?, [(8, 3)]);
    Ok(())
}

/// A graph whose audio path has latency `latency`, with an event source connected to an event
/// sink: `(graph, runner, event source, event sink)`.
fn parallel_to_latency(
    latency: usize,
) -> Result<(Graph, Runner, NodeId, NodeId), Box<dyn StdError>> {
    let (mut graph, runner) = Graph::new(Config::new(RATE, 128))?;
    let audio = graph.add_source(Layout::Mono);
    let delay = graph.add_node(Box::new(Latency::new(latency)));
    let audio_sink = graph.add_sink(Layout::Mono);
    graph.connect(audio, 0, delay, 0)?;
    graph.connect(delay, 0, audio_sink, 0)?;
    let source = graph.add_event_source();
    let sink = graph.add_event_sink();
    graph.connect(source, 0, sink, 0)?;
    Ok((graph, runner, source, sink))
}

#[test]
fn test_event_compensation_crosses_a_block_boundary() -> TestResult {
    let (mut graph, mut runner, source, sink) = parallel_to_latency(100)?;
    graph.commit()?;
    block(&mut runner, 64, &[(source, &[note(10, 1)?, note(63, 2)?])])?;
    assert!(notes(&runner, sink)?.is_empty());
    block(&mut runner, 64, &[])?;
    assert_eq!(notes(&runner, sink)?, [(46, 1)]);
    block(&mut runner, 64, &[])?;
    assert_eq!(notes(&runner, sink)?, [(35, 2)]);
    Ok(())
}

#[test]
fn test_event_compensation_through_automation_sub_blocks() -> TestResult {
    let (mut graph, mut runner, source, sink) = parallel_to_latency(70)?;
    let audio = graph.add_source(Layout::Mono);
    let gain = graph.add_node(Box::new(Gain::new(1.0)?));
    let audio_sink = graph.add_sink(Layout::Mono);
    graph.connect(audio, 0, gain, 0)?;
    graph.connect(gain, 0, audio_sink, 0)?;
    let curve = graph.add_curve(Arc::new(Curve::new(vec![Point::new(0, 0.5)])?));
    graph.connect_curve(curve, gain, "gain")?;
    graph.commit()?;
    let events = [note(0, 1)?, note(31, 2)?, note(50, 3)?, note(99, 4)?];
    let mut position = 7;
    let mut arrived = Vec::new();
    for sent in [&events[..], &[]] {
        assert_realtime(|| -> Result<(), &'static str> {
            runner.begin_block(100).map_err(|_| "block refused")?;
            runner.set_transport(position, true);
            let buffer = runner.event_source_mut(source).ok_or("no event source")?;
            for &event in sent {
                buffer.push(event);
            }
            runner.process();
            Ok(())
        })?;
        arrived.push(notes(&runner, sink)?);
        position += 100;
    }
    assert_eq!(arrived, [vec![(70, 1)], vec![(1, 2), (20, 3), (69, 4)]]);
    Ok(())
}

#[test]
fn test_live_event_source_is_not_delayed() -> TestResult {
    let (mut graph, mut runner, source, sink) = parallel_to_latency(100)?;
    graph.set_live(source, true)?;
    graph.commit()?;
    block(&mut runner, 64, &[(source, &[note(10, 1)?])])?;
    assert_eq!(notes(&runner, sink)?, [(10, 1)]);
    Ok(())
}

#[test]
fn test_reset_drops_pending_events() -> TestResult {
    let (mut graph, mut runner, source, sink) = parallel_to_latency(100)?;
    graph.commit()?;
    block(&mut runner, 64, &[(source, &[note(10, 1)?])])?;
    assert_realtime(|| runner.reset());
    block(&mut runner, 64, &[])?;
    assert!(notes(&runner, sink)?.is_empty());
    Ok(())
}

#[test]
fn test_merge_overflow_is_dropped_and_counted() -> TestResult {
    let (mut graph, mut runner) = Graph::new(Config::new(RATE, 128))?;
    let first = graph.add_event_source();
    let second = graph.add_event_source();
    let sink = graph.add_event_sink();
    graph.connect(first, 0, sink, 0)?;
    graph.connect(second, 0, sink, 0)?;
    graph.commit()?;
    let burst: Vec<Event> = (0..300)
        .map(|i| note(i % 64, 1))
        .collect::<Result<_, _>>()?;
    block(&mut runner, 64, &[(first, &burst), (second, &burst)])?;
    assert_eq!(notes(&runner, sink)?.len(), signal_graph::EVENT_CAPACITY);
    assert_eq!(
        graph.event_overflows(),
        (600 - signal_graph::EVENT_CAPACITY) as u64
    );
    Ok(())
}

/// An event node that sends `count` notes at frame 0 of each stretch, then one past its end.
struct Burst {
    count: usize,
}

impl Node for Burst {
    fn inputs(&self) -> Vec<Port> {
        Vec::new()
    }
    fn outputs(&self) -> Vec<Port> {
        vec![Port::event("out")]
    }
    fn process(&mut self, _inputs: &Inputs<'_>, outputs: &mut Outputs<'_>) {
        let Some(message) = Message::note_on(0, 60, 100) else {
            return;
        };
        for _ in 0..self.count {
            outputs.push_event(0, 0, message);
        }
        let past_end = u32::try_from(outputs.frames()).unwrap_or(u32::MAX);
        assert!(!outputs.push_event(0, past_end, message));
        assert!(!outputs.push_event(1, 0, message));
    }
}

#[test]
fn test_node_output_overflow_is_dropped_and_counted() -> TestResult {
    let (mut graph, mut runner) = Graph::new(Config::new(RATE, 128))?;
    let burst = graph.add_node(Box::new(Burst { count: 600 }));
    let sink = graph.add_event_sink();
    graph.connect(burst, 0, sink, 0)?;
    graph.commit()?;
    block(&mut runner, 64, &[])?;
    assert_eq!(notes(&runner, sink)?.len(), signal_graph::EVENT_CAPACITY);
    assert_eq!(
        graph.event_overflows(),
        (600 - signal_graph::EVENT_CAPACITY) as u64
    );
    Ok(())
}
