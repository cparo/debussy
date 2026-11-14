//! Identifiers, ports and the graph's fixed settings.

use std::fmt;

use audio_buffers::Layout;

/// Identifies a node in a [`Graph`](crate::Graph). Ids are never reused within a graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(pub(crate) u64);

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "node {}", self.0)
    }
}

/// Identifies a connection in a [`Graph`](crate::Graph). Ids are never reused within a graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ConnectionId(pub(crate) u64);

impl fmt::Display for ConnectionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "connection {}", self.0)
    }
}

/// The type of signal a port carries (`docs/architecture/signal-graph.md` "Port types").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum PortType {
    /// A mono or stereo `f32` audio signal.
    Audio,
    /// An automation curve's value, driving a parameter (`docs/architecture/automation.md`
    /// "Evaluation"). Has no buffer; its layout is unused.
    Control,
}

/// One input or output of a node: its name, its type and, for audio, its fixed channel layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Port {
    /// A name for display.
    pub name: String,
    /// The signal type. Only ports of the same type can be connected.
    pub kind: PortType,
    /// The channel layout. A connection between layouts is panned or folded down.
    pub layout: Layout,
}

impl Port {
    /// An audio port.
    pub fn audio(name: &str, layout: Layout) -> Self {
        Port {
            name: name.to_owned(),
            kind: PortType::Audio,
            layout,
        }
    }

    /// A control port.
    pub fn control(name: &str) -> Self {
        Port {
            name: name.to_owned(),
            kind: PortType::Control,
            layout: Layout::Mono,
        }
    }
}

/// The settings a graph and every plan compiled from it keep for their lifetime.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Config {
    /// The project sample rate, in hertz.
    pub sample_rate: u32,
    /// The largest block the real-time thread will ask for, in frames
    /// (`audio_buffers::MIN_FRAMES..=audio_buffers::MAX_FRAMES`).
    pub max_frames: usize,
    /// How many committed plans can wait for the real-time thread at once.
    pub plan_queue: usize,
}

impl Config {
    /// A configuration for `sample_rate` and `max_frames`, with room for four waiting plans.
    pub fn new(sample_rate: u32, max_frames: usize) -> Self {
        Config {
            sample_rate,
            max_frames,
            plan_queue: 4,
        }
    }
}
