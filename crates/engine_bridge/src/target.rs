//! What the processing graph must hold for a project state: one entry per track, the output and
//! each routing node, and one per connection, keyed by the model's ids.

use std::collections::BTreeMap;

use audio_buffers::Layout;
use project_model::{ConnectionId, Endpoint, NodeId, NodeKind, Side, State, TrackId};

/// A model element that becomes a graph node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum Key {
    Track(TrackId),
    Output,
    Node(NodeId),
}

impl From<Endpoint> for Key {
    fn from(endpoint: Endpoint) -> Self {
        match endpoint {
            Endpoint::Track(id) => Key::Track(id),
            Endpoint::Output => Key::Output,
            Endpoint::Node(id) => Key::Node(id),
        }
    }
}

/// The parts of a node that fix its graph declaration; a change to any of them replaces it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Shape {
    /// A track's source.
    Source(Layout),
    /// The stereo project output.
    Sink,
    /// A built-in effect by kind id.
    Effect(String),
    /// An effect kind this build does not know, as a silent stand-in with these port counts.
    Unknown {
        inputs: usize,
        outputs: usize,
    },
    Mixer(usize),
    Bus(Layout),
    Delay(Layout),
}

impl Shape {
    /// Whether the graph holds it as a processor, which `replace_node` can swap.
    pub(crate) fn is_processor(&self) -> bool {
        !matches!(self, Shape::Source(_) | Shape::Sink | Shape::Delay(_))
    }
}

/// One node as the state wants it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct NodeTarget {
    pub shape: Shape,
    /// Parameter values that differ from their defaults, by name.
    pub parameters: BTreeMap<String, f32>,
    /// A feedback delay's time.
    pub seconds: Option<f64>,
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
}

impl Target {
    /// The target for `state`.
    pub(crate) fn of(state: &State) -> Target {
        let connections: BTreeMap<ConnectionId, ConnectionTarget> = state
            .connections
            .iter()
            .map(|c| {
                (
                    c.id,
                    ConnectionTarget {
                        from: c.from.into(),
                        output: c.output,
                        to: c.to.into(),
                        input: c.input,
                        pan: c.pan,
                        from_side: c.from_side,
                        to_side: c.to_side,
                    },
                )
            })
            .collect();
        let mut nodes = BTreeMap::new();
        nodes.insert(
            Key::Output,
            NodeTarget {
                shape: Shape::Sink,
                parameters: BTreeMap::new(),
                seconds: None,
            },
        );
        for track in &state.tracks {
            nodes.insert(
                Key::Track(track.id),
                NodeTarget {
                    shape: Shape::Source(track.layout),
                    parameters: BTreeMap::new(),
                    seconds: None,
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
                NodeKind::Mixer {
                    channels,
                    parameters,
                } => (Shape::Mixer(*channels), parameters.clone(), None),
                NodeKind::Bus(layout) => (Shape::Bus(*layout), BTreeMap::new(), None),
                NodeKind::FeedbackDelay { layout, seconds } => {
                    (Shape::Delay(*layout), BTreeMap::new(), Some(*seconds))
                }
            };
            nodes.insert(
                key,
                NodeTarget {
                    shape,
                    parameters,
                    seconds,
                },
            );
        }
        Target { nodes, connections }
    }
}
