//! Routing and mixer state as saved and edited: the project's processing graph described by
//! node kinds, parameter values and connections (`docs/architecture/project.md` "Routing").
//!
//! The model checks references (every endpoint exists, a track or the output is used only in the
//! direction it can carry, no duplicate connections). Port and cycle rules are `signal_graph`'s,
//! checked when the routing is applied to the running graph. Curve ports and control wires are
//! the model's own (`docs/architecture/automation.md` "Curves in the project").

use std::collections::BTreeMap;
use std::fmt;

use audio_buffers::Layout;

use crate::error::Error;
use crate::types::{CurveId, TrackId};

/// Identifies a routing node. Ids are never reused within a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(pub u64);

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "node {}", self.0)
    }
}

/// Identifies a connection. Ids are never reused within a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ConnectionId(pub u64);

impl fmt::Display for ConnectionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "connection {}", self.0)
    }
}

/// One side of a stereo port. A connection end that names a side carries only that channel,
/// as a mono signal (ADR-0058).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Side {
    /// The left channel.
    Left,
    /// The right channel.
    Right,
}

/// One end of a connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Endpoint {
    /// A track's playback: a source with one output, of the track's layout.
    Track(TrackId),
    /// The project output: one stereo input, always present.
    Output,
    /// A routing node's audio ports.
    Node(NodeId),
    /// An automation curve: a source with one control output.
    Curve(CurveId),
    /// Curve port `port` (numbered from 1) of a routing node: one control input, taking one
    /// connection, from a curve.
    CurvePort(NodeId, u32),
}

impl Endpoint {
    /// The routing node the endpoint is on, for [`Endpoint::Node`] and [`Endpoint::CurvePort`].
    pub fn node(self) -> Option<NodeId> {
        match self {
            Endpoint::Node(id) | Endpoint::CurvePort(id, _) => Some(id),
            _ => None,
        }
    }

    /// Whether the endpoint carries a control signal: a curve or a curve port.
    pub fn is_control(self) -> bool {
        matches!(self, Endpoint::Curve(_) | Endpoint::CurvePort(..))
    }
}

/// What a routing node is.
#[derive(Debug, Clone, PartialEq)]
pub enum NodeKind {
    /// A built-in effect, named by a stable id such as `"eq"`, with the parameter values that
    /// differ from its defaults, by parameter name. An id the application does not know is kept
    /// and plays as silence.
    Effect {
        /// The effect's stable id.
        kind: String,
        /// Parameter values by name.
        parameters: BTreeMap<String, f32>,
    },
    /// A mixer (`docs/architecture/signal-graph.md` "Mixer"): `channels` mono inputs and one
    /// stereo output, with the parameter values that differ from their defaults, by name.
    Mixer {
        /// The number of channels, at least 1.
        channels: usize,
        /// Parameter values by name, such as `"channel 1 gain"`.
        parameters: BTreeMap<String, f32>,
    },
    /// A bus: one input and one output of `layout`, summing what is connected to it.
    Bus(Layout),
    /// A feedback delay of `seconds`, with one input and one output of `layout`.
    FeedbackDelay {
        /// The channel layout.
        layout: Layout,
        /// The delay time, as set.
        seconds: f64,
    },
}

/// Where a node's top-left corner sits on the patch board, in logical pixels. The interface's
/// to interpret; it plays no part in the sound.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Position {
    /// Rightwards from the board's left edge.
    pub x: f32,
    /// Downwards from the top of the tracks.
    pub y: f32,
}

/// The furthest a saved board position may be from the origin, either way.
pub const MAX_BOARD_COORDINATE: f32 = 1_000_000.0;

/// Checks a board coordinate: finite and within [`MAX_BOARD_COORDINATE`].
pub(crate) fn check_coordinate(value: f32) -> Result<(), Error> {
    if value.is_finite() && value.abs() <= MAX_BOARD_COORDINATE {
        Ok(())
    } else {
        Err(Error::InvalidValue)
    }
}

/// A routing node.
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    /// The node's id.
    pub id: NodeId,
    /// What it is.
    pub kind: NodeKind,
    /// Where the user put it on the patch board; `None` leaves it to the automatic layout.
    pub position: Option<Position>,
    /// The curve port, numbered from 1, each assigned parameter takes, by parameter name.
    pub curve_ports: BTreeMap<String, u32>,
}

impl Node {
    /// A node of `kind`, laid out automatically, with no curve ports.
    pub fn new(id: NodeId, kind: NodeKind) -> Self {
        Node {
            id,
            kind,
            position: None,
            curve_ports: BTreeMap::new(),
        }
    }

    /// The lowest curve port number no parameter takes.
    pub fn free_curve_port(&self) -> u32 {
        let taken: std::collections::BTreeSet<u32> = self.curve_ports.values().copied().collect();
        (1..).find(|n| !taken.contains(n)).unwrap_or(u32::MAX)
    }

    /// The parameter assigned to curve port `port`.
    pub fn curve_port_parameter(&self, port: u32) -> Option<&str> {
        self.curve_ports
            .iter()
            .find(|&(_, &p)| p == port)
            .map(|(name, _)| name.as_str())
    }
}

/// A connection from an output port of one endpoint to an input port of another.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Connection {
    /// The connection's id.
    pub id: ConnectionId,
    /// Where the signal comes from; never [`Endpoint::Output`].
    pub from: Endpoint,
    /// The output port.
    pub output: usize,
    /// Where it goes; never [`Endpoint::Track`].
    pub to: Endpoint,
    /// The input port.
    pub input: usize,
    /// Where a mono signal entering a stereo input is panned, −1 to 1.
    pub pan: f32,
    /// The side of a stereo output it takes, or `None` for the whole port.
    pub from_side: Option<Side>,
    /// The side of a stereo input it feeds, or `None` for the whole port.
    pub to_side: Option<Side>,
}

impl Connection {
    /// A connection panned centre.
    pub fn new(
        id: ConnectionId,
        from: Endpoint,
        output: usize,
        to: Endpoint,
        input: usize,
    ) -> Self {
        Connection {
            id,
            from,
            output,
            to,
            input,
            pan: 0.0,
            from_side: None,
            to_side: None,
        }
    }

    /// The same connection taking `from_side` of its output and feeding `to_side` of its input.
    pub fn with_sides(mut self, from_side: Option<Side>, to_side: Option<Side>) -> Self {
        self.from_side = from_side;
        self.to_side = to_side;
        self
    }

    /// Whether it starts or ends at `endpoint`. A connection to one of a node's curve ports
    /// touches the node too.
    pub fn touches(&self, endpoint: Endpoint) -> bool {
        let on_curve_port = matches!(
            (endpoint, self.to),
            (Endpoint::Node(node), Endpoint::CurvePort(port_node, _)) if node == port_node
        );
        self.from == endpoint || self.to == endpoint || on_curve_port
    }
}

/// Checks a node's own values, and its curve ports: numbered from 1, no number twice, and for a
/// mixer or an effect this build knows, each on a parameter that may be automated.
pub(crate) fn check_node(node: &Node) -> Result<(), Error> {
    if let Some(p) = node.position {
        check_coordinate(p.x)?;
        check_coordinate(p.y)?;
    }
    let mut numbers = std::collections::BTreeSet::new();
    for (name, &port) in &node.curve_ports {
        if port == 0 || !numbers.insert(port) {
            return Err(Error::CurvePortTaken(node.id, port));
        }
        match parameter_info(&node.kind, name) {
            Lookup::Known { automatable: true } | Lookup::UnknownKind => {}
            Lookup::Known { automatable: false }
            | Lookup::NoSuchParameter
            | Lookup::NoParameters => {
                return Err(Error::NotAutomatable(node.id, name.clone()));
            }
        }
    }
    match &node.kind {
        NodeKind::Effect { parameters, .. } => check_parameters(parameters),
        NodeKind::Mixer {
            channels,
            parameters,
        } => {
            check_channels(*channels)?;
            check_parameters(parameters)
        }
        NodeKind::Bus(_) => Ok(()),
        NodeKind::FeedbackDelay { seconds, .. } => check_delay(*seconds),
    }
}

/// Checks that every parameter value is finite.
fn check_parameters(parameters: &BTreeMap<String, f32>) -> Result<(), Error> {
    match parameters.values().all(|v| v.is_finite()) {
        true => Ok(()),
        false => Err(Error::InvalidValue),
    }
}

/// Checks a mixer has at least one channel. The upper limit is the mixer node's, checked when the
/// routing reaches the running graph.
pub(crate) fn check_channels(channels: usize) -> Result<(), Error> {
    match channels {
        0 => Err(Error::InvalidValue),
        _ => Ok(()),
    }
}

/// Checks a feedback-delay time is one `signal_graph` accepts.
pub(crate) fn check_delay(seconds: f64) -> Result<(), Error> {
    if seconds.is_finite() && seconds <= signal_graph::MAX_FEEDBACK_DELAY {
        Ok(())
    } else {
        Err(Error::InvalidValue)
    }
}

/// Checks a pan position.
pub(crate) fn check_pan(pan: f32) -> Result<(), Error> {
    if (-1.0..=1.0).contains(&pan) {
        Ok(())
    } else {
        Err(Error::InvalidValue)
    }
}

/// What a node kind says about one of its parameters.
pub(crate) enum Lookup {
    /// The parameter exists; whether it may be automated
    /// ([`signal_graph::ParameterInfo::is_automatable`]).
    Known {
        /// Whether a curve may drive it.
        automatable: bool,
    },
    /// The node kind is known, but has no parameter of that name.
    NoSuchParameter,
    /// An effect this build does not know: its parameters cannot be checked.
    UnknownKind,
    /// A bus or feedback delay, which has no parameters.
    NoParameters,
}

/// Looks up parameter `name` of a node of `kind`. Allocates: builds the effect to ask it.
pub(crate) fn parameter_info(kind: &NodeKind, name: &str) -> Lookup {
    let infos = match kind {
        NodeKind::Effect { kind, .. } => match audio_effects::parameter_infos(kind) {
            Some(infos) => infos,
            None => return Lookup::UnknownKind,
        },
        NodeKind::Mixer { channels, .. } => audio_effects::mixer_parameter_infos(*channels),
        NodeKind::Bus(_) | NodeKind::FeedbackDelay { .. } => return Lookup::NoParameters,
    };
    match infos.into_iter().find(|info| info.name == name) {
        Some(info) => Lookup::Known {
            automatable: info.is_automatable(),
        },
        None => Lookup::NoSuchParameter,
    }
}
