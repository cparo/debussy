//! What the processing graph must hold for a project state: one entry per track, the output,
//! each routing node and each automation curve, one per audio connection, and one per control
//! wire, keyed by the model's ids.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use audio_buffers::Layout;
use instruments::Playback;
use modulation::Curve;
use project_model::{
    ConnectionId, CurveId, Endpoint, NodeId, NodeKind, SampleZone, Side, Source, State, Tempo,
    TrackId, ZoneMode,
};

use crate::sample_loader::Samples;

/// A model element that becomes a graph node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum Key {
    Track(TrackId),
    Output,
    Node(NodeId),
    Curve(CurveId),
}

impl Key {
    /// The graph node an audio endpoint is; `None` for a curve or curve port, which carry
    /// control signals the graph does not hold yet (issue #63).
    pub(crate) fn of_audio(endpoint: Endpoint) -> Option<Key> {
        match endpoint {
            Endpoint::Track(id) => Some(Key::Track(id)),
            Endpoint::Output => Some(Key::Output),
            Endpoint::Node(id) => Some(Key::Node(id)),
            Endpoint::Curve(_) | Endpoint::CurvePort(..) => None,
        }
    }
}

/// The parts of a node that fix its graph declaration; a change to any of them replaces it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Shape {
    /// An audio track's source.
    Source(Layout),
    /// A MIDI track's source, with one event output.
    EventSource,
    /// The stereo project output.
    Sink,
    /// A MIDI output node, with one event input; its port is the sender's to know, not the
    /// graph's.
    MidiOut,
    /// A built-in effect by kind id.
    Effect(String),
    /// A built-in instrument by kind id, with a sampler's zones.
    Instrument(String, Zones),
    /// An instrument kind this build does not know, as a silent stand-in with an instrument's
    /// ports.
    UnknownInstrument,
    /// An effect kind this build does not know, as a silent stand-in with these port counts.
    Unknown {
        inputs: usize,
        outputs: usize,
    },
    Mixer(usize),
    Bus(Layout),
    Delay(Layout),
    /// An automation curve, its points in [`NodeTarget::curve`].
    Curve,
}

impl Shape {
    /// Whether the graph holds it as a processor, which `replace_node` can swap.
    pub(crate) fn is_processor(&self) -> bool {
        !matches!(
            self,
            Shape::Source(_)
                | Shape::EventSource
                | Shape::Sink
                | Shape::MidiOut
                | Shape::Delay(_)
                | Shape::Curve
        )
    }
}

/// A sampler's zones as the model has them, and their samples once loaded
/// ([`Target::load_samples`]). Two are equal when the model's zones are: loading changes nothing
/// that would need a new node.
#[derive(Debug, Clone, Default)]
pub(crate) struct Zones {
    pub model: Vec<SampleZone>,
    /// The zones whose files loaded, in the model's order.
    pub loaded: Arc<[instruments::Zone]>,
}

impl PartialEq for Zones {
    fn eq(&self, other: &Self) -> bool {
        self.model == other.model
    }
}

/// The instrument zone `zone` with its samples from `samples` at `rate`; `None`, leaving the
/// zone silent, if a file does not load.
fn load_zone(zone: &SampleZone, samples: &mut Samples, rate: u32) -> Option<instruments::Zone> {
    let (left, right) = match &zone.file {
        Source::Mono(file) => (samples.load(&file.name, rate)?, None),
        Source::Stereo { left, right } => (
            samples.load(&left.name, rate)?,
            Some(samples.load(&right.name, rate)?),
        ),
        Source::Midi(_) => return None,
    };
    Some(instruments::Zone {
        low: zone.low,
        high: zone.high,
        root: zone.root,
        playback: match zone.mode {
            ZoneMode::OneShot => Playback::OneShot,
            ZoneMode::Gated => Playback::Gated,
        },
        choke: zone.choke,
        tune: zone.tune,
        left,
        right,
    })
}

/// One node as the state wants it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct NodeTarget {
    pub shape: Shape,
    /// Parameter values that differ from their defaults, by name.
    pub parameters: BTreeMap<String, f32>,
    /// A feedback delay's time.
    pub seconds: Option<f64>,
    /// A curve's points.
    pub curve: Option<Arc<Curve>>,
}

/// One control wire as the state wants it: a curve driving a parameter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ControlTarget {
    pub curve: CurveId,
    pub node: NodeId,
    /// The parameter the wired curve port is assigned to.
    pub parameter: String,
}

/// One connection as the state wants it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ConnectionTarget {
    pub from: Key,
    pub output: usize,
    pub to: Key,
    pub input: usize,
    pub pan: f32,
    pub from_side: Option<Side>,
    pub to_side: Option<Side>,
}

impl ConnectionTarget {
    /// Whether `other` joins the same ports and sides (pan aside).
    pub(crate) fn same_ports(&self, other: &ConnectionTarget) -> bool {
        (
            self.from,
            self.output,
            self.from_side,
            self.to,
            self.input,
            self.to_side,
        ) == (
            other.from,
            other.output,
            other.from_side,
            other.to,
            other.input,
            other.to_side,
        )
    }
}

/// Everything the graph must hold.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Target {
    pub nodes: BTreeMap<Key, NodeTarget>,
    pub connections: BTreeMap<ConnectionId, ConnectionTarget>,
    /// The control wires.
    pub controls: BTreeMap<ConnectionId, ControlTarget>,
    /// The project tempo, which nodes that use it are prepared at.
    pub tempo: Tempo,
}

impl Target {
    /// Loads every sampler zone's samples from `samples` at `rate`, off the audio thread, and
    /// forgets cached files no zone uses. A file that does not load leaves its zone out, with a
    /// warning in `samples`.
    pub(crate) fn load_samples(&mut self, samples: &mut Samples, rate: u32) {
        for node in self.nodes.values_mut() {
            if let Shape::Instrument(_, zones) = &mut node.shape {
                zones.loaded = zones
                    .model
                    .iter()
                    .filter_map(|z| load_zone(z, samples, rate))
                    .collect();
            }
        }
        let used: BTreeSet<&str> = self
            .nodes
            .values()
            .filter_map(|n| match &n.shape {
                Shape::Instrument(_, zones) => Some(&zones.model),
                _ => None,
            })
            .flatten()
            .flat_map(|z| z.file.files())
            .map(|f| f.name.as_str())
            .collect();
        samples.retain(&used);
    }

    /// The target for `state`, with no samples loaded yet.
    pub(crate) fn of(state: &State) -> Target {
        let connections: BTreeMap<ConnectionId, ConnectionTarget> = state
            .connections
            .iter()
            .filter_map(|c| {
                Some((
                    c.id,
                    ConnectionTarget {
                        from: Key::of_audio(c.from)?,
                        output: c.output,
                        to: Key::of_audio(c.to)?,
                        input: c.input,
                        pan: c.pan,
                        from_side: c.from_side,
                        to_side: c.to_side,
                    },
                ))
            })
            .collect();
        let mut nodes = BTreeMap::new();
        nodes.insert(
            Key::Output,
            NodeTarget {
                shape: Shape::Sink,
                parameters: BTreeMap::new(),
                seconds: None,
                curve: None,
            },
        );
        for track in &state.tracks {
            nodes.insert(
                Key::Track(track.id),
                NodeTarget {
                    shape: match track.layout() {
                        Some(layout) => Shape::Source(layout),
                        None => Shape::EventSource,
                    },
                    parameters: BTreeMap::new(),
                    seconds: None,
                    curve: None,
                },
            );
        }
        for node in &state.nodes {
            let key = Key::Node(node.id);
            let (shape, parameters, seconds) = match &node.kind {
                NodeKind::Effect { kind, parameters } => {
                    let shape = if audio_effects::effect(kind).is_some() {
                        Shape::Effect(kind.clone())
                    } else {
                        // Enough ports for the connections saved with it.
                        let ports = |pick: &dyn Fn(&ConnectionTarget) -> Option<usize>| {
                            connections
                                .values()
                                .filter_map(pick)
                                .max()
                                .map_or(1, |p| p + 1)
                        };
                        Shape::Unknown {
                            inputs: ports(&|c| (c.to == key).then_some(c.input)),
                            outputs: ports(&|c| (c.from == key).then_some(c.output)),
                        }
                    };
                    (shape, parameters.clone(), None)
                }
                NodeKind::Instrument {
                    kind,
                    parameters,
                    zones,
                } => {
                    let shape = match instruments::instrument(kind) {
                        Some(_) => Shape::Instrument(
                            kind.clone(),
                            Zones {
                                model: zones.clone(),
                                loaded: Arc::from([]),
                            },
                        ),
                        None => Shape::UnknownInstrument,
                    };
                    (shape, parameters.clone(), None)
                }
                NodeKind::Mixer {
                    channels,
                    parameters,
                } => (Shape::Mixer(*channels), parameters.clone(), None),
                NodeKind::Bus(layout) => (Shape::Bus(*layout), BTreeMap::new(), None),
                NodeKind::FeedbackDelay { layout, seconds } => {
                    (Shape::Delay(*layout), BTreeMap::new(), Some(*seconds))
                }
                NodeKind::MidiOut { .. } => (Shape::MidiOut, BTreeMap::new(), None),
            };
            nodes.insert(
                key,
                NodeTarget {
                    shape,
                    parameters,
                    seconds,
                    curve: None,
                },
            );
        }
        for lane in &state.curves {
            nodes.insert(
                Key::Curve(lane.id),
                NodeTarget {
                    shape: Shape::Curve,
                    parameters: BTreeMap::new(),
                    seconds: None,
                    curve: Some(Arc::new(lane.curve.clone())),
                },
            );
        }
        let controls = state
            .connections
            .iter()
            .filter_map(|c| {
                let (Endpoint::Curve(curve), Endpoint::CurvePort(node, port)) = (c.from, c.to)
                else {
                    return None;
                };
                // A stand-in for an unknown effect or instrument has no parameters to drive.
                if matches!(
                    nodes.get(&Key::Node(node)).map(|n| &n.shape),
                    Some(Shape::Unknown { .. } | Shape::UnknownInstrument | Shape::MidiOut) | None
                ) {
                    return None;
                }
                let parameter = state.node(node)?.curve_port_parameter(port)?.to_owned();
                Some((
                    c.id,
                    ControlTarget {
                        curve,
                        node,
                        parameter,
                    },
                ))
            })
            .collect();
        Target {
            nodes,
            connections,
            controls,
            tempo: state.tempo,
        }
    }
}
