//! The processing graph: nodes, typed ports, cycle rules, processing plans, delay compensation,
//! panning and summing (`docs/architecture/signal-graph.md`, every section).
//!
//! A [`Graph`] is edited off the real-time thread: add processors ([`Node`]), feedback delays,
//! sources (signals the engine supplies) and sinks (signals it takes), and connect their ports.
//! [`Graph::commit`] compiles it into a processing plan and sends it to the [`Runner`] on the
//! real-time thread, which swaps it in at the next block and sends the old plan back to be
//! dropped. A node keeps its state across plans: it moves from the old plan into the new one.
//!
//! - Ports carry audio, control (an automation curve's value) or events: MIDI 1.0 channel
//!   messages stamped with frames in the block, in fixed-capacity buffers ([`EventBuffer`]).
//!   Connections join ports of the same type only.
//! - Several connections into one audio input are summed. A mono signal entering a stereo input is
//!   panned with the −3 dB law (centre by default); a stereo signal entering a mono input is
//!   folded down to `(L + R) × 0.7071`.
//! - Several connections into one event input are merged in frame order; events at the same frame
//!   keep the order of the connections' ids. An event a full buffer cannot take is dropped and
//!   counted ([`Graph::event_overflows`]); nothing allocates.
//! - A connection that would close a cycle without passing through a feedback delay is refused.
//!   Each cycle runs in sub-blocks no longer than its shortest feedback delay, so output does not
//!   depend on the block size; everything else runs once per block.
//! - Every connection is delayed so that the signals meeting at a node are aligned despite the
//!   latency each node reports, and every sink is aligned to the plan's latency. Event
//!   connections are delayed by shifting their events' frames.
//! - Parameters are [`Parameter`]s, atomics the UI sets; nodes read them through [`Smoothed`].
//!
//! # Modules
//!
//! - `types`: [`NodeId`], [`ConnectionId`], [`Port`], [`PortType`], [`Config`].
//! - `param`: [`Parameter`], [`ParameterInfo`], [`Scale`], [`Ramp`] and [`Smoothed`].
//! - `node`: the [`Node`] trait, [`Inputs`] and [`Outputs`].
//! - `graph`: [`Graph`], the editable graph, and [`Connection`].
//! - `topology`: cycle finding and run order.
//! - `compile`: graph to plan, with delay compensation.
//! - `buffers`: per-port buffers and event-connection delays.
//! - `delay`: feedback and compensation delay lines.
//! - `plan`: a plan and how it runs.
//! - `runner`: [`Runner`], the real-time side.
//! - `error`: [`Error`].

#![forbid(unsafe_code)]

mod buffers;
mod compile;
mod delay;
mod error;
mod graph;
mod node;
mod param;
mod plan;
mod runner;
mod topology;
mod types;

pub use delay::{MAX_FEEDBACK_DELAY, MIN_FEEDBACK_DELAY, feedback_delay_frames};
pub use error::Error;
pub use graph::{Connection, ControlConnection, Graph};
pub use midi::{EVENT_CAPACITY, Event, EventBuffer, Message};
pub use node::{Inputs, Node, Outputs};
pub use param::{
    Parameter, ParameterInfo, RAMP_SECONDS, Ramp, Reading, Scale, Smoothed, map_to_parameter,
};
pub use plan::CONTROL_PERIOD;
pub use runner::Runner;
pub use types::{Config, ConnectionId, NodeId, Port, PortType};
