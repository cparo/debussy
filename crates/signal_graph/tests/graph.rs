//! Graph editing and plan compilation, off the real-time thread.

mod support;

use std::error::Error as StdError;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use audio_buffers::Layout;
use realtime_guard::GuardAllocator;
use signal_graph::{Config, Error, Graph, Inputs, Node, Outputs, Port};
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

#[test]
fn test_replace_node_keeps_compatible_connections() -> TestResult {
    let (mut graph, _runner) = graph()?;
    let source = graph.add_source(Layout::Mono);
    let gain = graph.add_node(Box::new(Gain::new(1.0)?));
    let sink = graph.add_sink(Layout::Mono);
    let input = graph.connect(source, 0, gain, 0)?;
    let output = graph.connect(gain, 0, sink, 0)?;
    // A counter has an output but no input.
    let (counter, _) = Counter::new();
    let dropped = graph.replace_node(gain, Box::new(counter))?;
    assert_eq!(dropped, [input]);
    assert!(graph.connection(input).is_none());
    assert!(graph.connection(output).is_some());
    assert_eq!(graph.inputs(gain).map(<[_]>::len), Some(0));
    graph.commit()?;
    Ok(())
}

#[test]
fn test_replace_node_refuses_non_processors() -> TestResult {
    let (mut graph, _runner) = graph()?;
    let sink = graph.add_sink(Layout::Mono);
    let delay = graph.add_feedback_delay(Layout::Mono, 0.01)?;
    for id in [sink, delay] {
        assert_eq!(
            graph.replace_node(id, Box::new(Gain::new(1.0)?)),
            Err(Error::NotAProcessor(id))
        );
    }
    graph.remove_node(sink)?;
    assert_eq!(
        graph.replace_node(sink, Box::new(Gain::new(1.0)?)),
        Err(Error::UnknownNode(sink))
    );
    Ok(())
}

/// A node that reads the tempo, recording the whole-note length it holds when prepared.
struct Tempo {
    given: f64,
    prepared: Arc<AtomicU64>,
}

impl Node for Tempo {
    fn inputs(&self) -> Vec<Port> {
        Vec::new()
    }

    fn outputs(&self) -> Vec<Port> {
        Vec::new()
    }

    fn uses_tempo(&self) -> bool {
        true
    }

    fn set_tempo(&mut self, frames_per_whole_note: f64) {
        self.given = frames_per_whole_note;
    }

    fn prepare(&mut self, _sample_rate: u32, _max_frames: usize) {
        self.prepared.store(self.given.to_bits(), Ordering::Relaxed);
    }

    fn process(&mut self, _inputs: &Inputs<'_>, _outputs: &mut Outputs<'_>) {}
}

#[test]
fn test_set_tempo_reaches_prepare_and_names_only_tempo_users() -> TestResult {
    let (mut graph, _runner) = graph()?;
    // 120 BPM in 4/4 until set: two seconds at 48 kHz.
    assert_eq!(graph.tempo(), 96_000.0);
    let prepared = Arc::new(AtomicU64::new(0));
    let reader = || {
        Box::new(Tempo {
            given: 0.0,
            prepared: Arc::clone(&prepared),
        })
    };
    let user = graph.add_node(reader());
    assert_eq!(f64::from_bits(prepared.load(Ordering::Relaxed)), 96_000.0);
    graph.add_node(Box::new(Gain::new(1.0)?));
    assert_eq!(graph.set_tempo(72_000.0), [user]);
    assert_eq!(graph.tempo(), 72_000.0);
    // Unchanged, or not a length: nothing to replace.
    assert!(graph.set_tempo(72_000.0).is_empty());
    assert!(graph.set_tempo(f64::NAN).is_empty());
    assert!(graph.set_tempo(0.0).is_empty());
    assert_eq!(graph.tempo(), 72_000.0);
    graph.replace_node(user, reader())?;
    assert_eq!(f64::from_bits(prepared.load(Ordering::Relaxed)), 72_000.0);
    Ok(())
}

/// A mono pass-through with a continuous `level` and a stepped `steps` parameter.
struct Knobs(Vec<signal_graph::Parameter>);

impl Knobs {
    fn new() -> Result<Self, Error> {
        use signal_graph::{Parameter, ParameterInfo, Scale};
        Ok(Knobs(vec![
            Parameter::new(ParameterInfo::new("level", 0.0, 1.0, 0.5))?,
            Parameter::new(ParameterInfo::new("steps", 0.0, 4.0, 0.0).with_scale(Scale::Integer))?,
        ]))
    }
}

impl Node for Knobs {
    fn inputs(&self) -> Vec<Port> {
        vec![Port::audio("in", Layout::Mono)]
    }
    fn outputs(&self) -> Vec<Port> {
        vec![Port::audio("out", Layout::Mono)]
    }
    fn parameters(&self) -> Vec<signal_graph::Parameter> {
        self.0.clone()
    }
    fn process(&mut self, _inputs: &Inputs<'_>, _outputs: &mut Outputs<'_>) {}
}

#[test]
fn test_connect_curve_checks_its_ends() -> TestResult {
    let (mut graph, _runner) = graph()?;
    let curve = graph.add_curve(Arc::new(modulation::Curve::default()));
    let knobs = graph.add_node(Box::new(Knobs::new()?));
    let source = graph.add_source(Layout::Mono);
    assert_eq!(
        graph.connect_curve(source, knobs, "level"),
        Err(Error::NotACurve(source))
    );
    assert_eq!(
        graph.connect_curve(curve, source, "level"),
        Err(Error::NotAProcessor(source))
    );
    assert!(matches!(
        graph.connect_curve(curve, knobs, "nope"),
        Err(Error::UnknownParameter { .. })
    ));
    assert!(matches!(
        graph.connect_curve(curve, knobs, "steps"),
        Err(Error::NotAutomatable { .. })
    ));
    let id = graph.connect_curve(curve, knobs, "level")?;
    assert_eq!(
        graph.connect_curve(curve, knobs, "level"),
        Err(Error::AlreadyConnected)
    );
    // A curve's control output takes no audio connection.
    assert_eq!(
        graph.connect(curve, 0, knobs, 0),
        Err(Error::PortTypeMismatch)
    );
    assert_eq!(graph.control_connection(id).map(|c| c.node), Some(knobs));
    graph.set_curve(curve, Arc::new(modulation::Curve::default()))?;
    assert!(graph.control_connection(id).is_some());
    assert_eq!(
        graph.set_curve(knobs, Arc::new(modulation::Curve::default())),
        Err(Error::NotACurve(knobs))
    );
    graph.disconnect(id)?;
    assert_eq!(graph.control_connections().count(), 0);
    graph.commit()?;
    Ok(())
}

#[test]
fn test_removing_or_replacing_drops_control_connections() -> TestResult {
    let (mut graph, _runner) = graph()?;
    let curve = graph.add_curve(Arc::new(modulation::Curve::default()));
    let knobs = graph.add_node(Box::new(Knobs::new()?));
    let gain = graph.add_node(Box::new(Gain::new(1.0)?));
    let kept = graph.connect_curve(curve, knobs, "level")?;
    let lost = graph.connect_curve(curve, gain, "gain")?;
    // A replacement with the parameter keeps its curve; one without loses it.
    assert!(
        graph
            .replace_node(knobs, Box::new(Knobs::new()?))?
            .is_empty()
    );
    assert_eq!(
        graph.replace_node(gain, Box::new(Knobs::new()?))?,
        vec![lost]
    );
    assert!(graph.control_connection(kept).is_some());
    graph.remove_node(curve)?;
    assert_eq!(graph.control_connections().count(), 0);
    Ok(())
}
