//! Graph editing and plan compilation, off the real-time thread.

mod support;

use std::error::Error as StdError;

use audio_buffers::Layout;
use realtime_guard::GuardAllocator;
use signal_graph::{Config, Error, Graph};
use support::{Counter, Gain, Latency};

#[global_allocator]
static ALLOCATOR: GuardAllocator = GuardAllocator;

type TestResult = Result<(), Box<dyn StdError>>;

fn graph() -> Result<(Graph, signal_graph::Runner), Error> {
    Graph::new(Config::new(48_000, 128))
}

#[test]
fn test_new_refuses_bad_config() {
    assert!(matches!(
        Graph::new(Config::new(0, 128)),
        Err(Error::ZeroSampleRate)
    ));
    assert!(matches!(
        Graph::new(Config::new(48_000, 2048)),
        Err(Error::Buffers(_))
    ));
    let config = Config {
        plan_queue: 0,
        ..Config::new(48_000, 128)
    };
    assert!(matches!(Graph::new(config), Err(Error::Buffers(_))));
}

#[test]
fn test_connect_refuses_cycle_without_delay() -> TestResult {
    let (mut graph, _runner) = graph()?;
    let a = graph.add_node(Box::new(Gain::new(1.0)?));
    let b = graph.add_node(Box::new(Gain::new(1.0)?));
    graph.connect(a, 0, b, 0)?;
    assert_eq!(graph.connect(b, 0, a, 0), Err(Error::CycleWithoutDelay));
    assert_eq!(graph.connect(a, 0, a, 0), Err(Error::CycleWithoutDelay));
    // The refused connection was not kept: the graph still commits.
    graph.commit()?;
    Ok(())
}

#[test]
fn test_connect_accepts_cycle_through_delay() -> TestResult {
    let (mut graph, _runner) = graph()?;
    let a = graph.add_node(Box::new(Gain::new(1.0)?));
    let b = graph.add_node(Box::new(Gain::new(0.5)?));
    let delay = graph.add_feedback_delay(Layout::Mono, 0.002)?;
    graph.connect(a, 0, b, 0)?;
    graph.connect(b, 0, delay, 0)?;
    graph.connect(delay, 0, a, 0)?;
    graph.connect(delay, 0, delay, 0)?;
    // A second loop, a ⇄ c, with no delay of its own.
    let c = graph.add_node(Box::new(Gain::new(1.0)?));
    graph.connect(a, 0, c, 0)?;
    assert_eq!(graph.connect(c, 0, a, 0), Err(Error::CycleWithoutDelay));
    graph.commit()?;
    Ok(())
}

#[test]
fn test_connect_checks_ports() -> TestResult {
    let (mut graph, _runner) = graph()?;
    let source = graph.add_source(Layout::Stereo);
    let sink = graph.add_sink(Layout::Mono);
    assert_eq!(
        graph.connect(source, 1, sink, 0),
        Err(Error::NoSuchOutput {
            node: source,
            port: 1
        })
    );
    assert_eq!(
        graph.connect(source, 0, sink, 1),
        Err(Error::NoSuchInput {
            node: sink,
            port: 1
        })
    );
    assert_eq!(
        graph.connect(sink, 0, source, 0),
        Err(Error::NoSuchOutput {
            node: sink,
            port: 0
        })
    );
    graph.connect(source, 0, sink, 0)?;
    assert_eq!(
        graph.connect(source, 0, sink, 0),
        Err(Error::AlreadyConnected)
    );
    Ok(())
}

#[test]
fn test_remove_node_removes_its_connections() -> TestResult {
    let (mut graph, _runner) = graph()?;
    let (counter, _) = Counter::new();
    let counter = graph.add_node(Box::new(counter));
    let sink = graph.add_sink(Layout::Mono);
    let connection = graph.connect(counter, 0, sink, 0)?;
    graph.remove_node(counter)?;
    assert!(graph.connection(connection).is_none());
    assert_eq!(graph.remove_node(counter), Err(Error::UnknownNode(counter)));
    assert_eq!(
        graph.disconnect(connection),
        Err(Error::UnknownConnection(connection))
    );
    graph.commit()?;
    Ok(())
}

#[test]
fn test_feedback_delay_times_are_checked() -> TestResult {
    let (mut graph, _runner) = graph()?;
    assert!(matches!(
        graph.add_feedback_delay(Layout::Mono, f64::NAN),
        Err(Error::InvalidDelay(_))
    ));
    assert_eq!(
        graph.add_feedback_delay(Layout::Mono, 11.0),
        Err(Error::InvalidDelay(11.0))
    );
    let delay = graph.add_feedback_delay(Layout::Mono, 0.0)?;
    graph.set_feedback_delay(delay, 0.5)?;
    let sink = graph.add_sink(Layout::Mono);
    assert_eq!(
        graph.set_feedback_delay(sink, 0.5),
        Err(Error::NotAFeedbackDelay(sink))
    );
    assert_eq!(signal_graph::feedback_delay_frames(0.0, 44_100), 45);
    Ok(())
}

#[test]
fn test_node_declarations_are_kept() -> TestResult {
    let (mut graph, _runner) = graph()?;
    let gain = graph.add_node(Box::new(Gain::new(0.5)?));
    let latency = graph.add_node(Box::new(Latency::new(7)));
    let parameters = graph.parameters(gain).ok_or("no parameters")?;
    assert_eq!(parameters.len(), 1);
    assert_eq!(parameters[0].get(), 0.5);
    assert_eq!(graph.inputs(latency).map(<[_]>::len), Some(1));
    assert_eq!(graph.outputs(latency).map(<[_]>::len), Some(1));
    Ok(())
}

#[test]
fn test_commit_reports_latency() -> TestResult {
    let (mut graph, _runner) = graph()?;
    let source = graph.add_source(Layout::Mono);
    let slow = graph.add_node(Box::new(Latency::new(10)));
    let slower = graph.add_node(Box::new(Latency::new(25)));
    let sink = graph.add_sink(Layout::Mono);
    graph.connect(source, 0, slow, 0)?;
    graph.connect(slow, 0, slower, 0)?;
    graph.connect(slower, 0, sink, 0)?;
    graph.connect(source, 0, sink, 0)?;
    graph.commit()?;
    assert_eq!(graph.latency(), 35);
    Ok(())
}

#[test]
fn test_commit_when_busy_keeps_the_graph() -> TestResult {
    let config = Config {
        plan_queue: 1,
        ..Config::new(48_000, 64)
    };
    let (mut graph, mut runner) = Graph::new(config)?;
    let sink = graph.add_sink(Layout::Mono);
    graph.commit()?;
    let (counter, _) = Counter::new();
    let counter = graph.add_node(Box::new(counter));
    graph.connect(counter, 0, sink, 0)?;
    assert_eq!(graph.commit(), Err(Error::Busy));
    // Once the runner takes the first plan, the counter goes out in the next.
    support::render(&mut runner, None, sink, &[0.0; 64], 64)?;
    graph.commit()?;
    let output = support::render(&mut runner, None, sink, &[0.0; 4], 4)?;
    assert_eq!(output, [0.0, 1.0, 2.0, 3.0]);
    Ok(())
}
