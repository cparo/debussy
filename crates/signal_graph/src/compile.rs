//! Compiling a graph into a plan, off the real-time thread
//! (`docs/architecture/signal-graph.md` "Processing graph" and "Delay compensation").
//!
//! Latency is worked out over the connections that do not close a loop, which form an acyclic
//! graph. A node is *live-only* when it is a live source, or when it has such inputs and every
//! one comes from a live-only node. A node's signals *arrive* at the latest time any of its
//! inputs from nodes that are not live-only do, and leave after its own latency. Each connection
//! from a node that is not live-only is delayed by the difference between its source's departure
//! and its target's arrival, so every node sees its playback inputs aligned. Sinks are all aligned
//! to the latest arrival at any sink, which is the plan's latency. A loop-closing connection, or
//! one from a live-only node, is not compensated. An event connection is compensated like an
//! audio one, by shifting its events' frames.
//!
//! Each curve-to-parameter connection becomes a binding that evaluates its curve at the timeline
//! frame minus the node's arrival time (`docs/architecture/automation.md` "Evaluation"). For a
//! live-only node that is the arrival of its own live signals.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

use midi::{EVENT_CAPACITY, EventBuffer};

use dsp::pan_gains;

use crate::buffers::{EventDelay, PortBuffer};
use crate::delay::{DelayLine, feedback_delay_frames};
use crate::error::Error;
use crate::graph::{Connection, ControlConnection, Entry, EntryKind, automatable};
use crate::plan::{Binding, Feed, Kind, Plan, Signal, Slot, Step};
use crate::topology::{self, Group};
use crate::types::{Config, ConnectionId, NodeId, Port, PortType};

/// Checks that the connections form no cycle without a feedback delay.
///
/// # Errors
///
/// [`Error::CycleWithoutDelay`] if they do.
pub(crate) fn check_cycles(
    nodes: &BTreeMap<NodeId, Entry>,
    connections: &BTreeMap<ConnectionId, Connection>,
) -> Result<(), Error> {
    let (edges, is_delay) = numbered(nodes, connections);
    topology::analyse(nodes.len(), &edges, &is_delay).map(|_| ())
}

/// The connections as `(from, to)` slot numbers, in id order, and which slots are delays.
fn numbered(
    nodes: &BTreeMap<NodeId, Entry>,
    connections: &BTreeMap<ConnectionId, Connection>,
) -> (Vec<(usize, usize)>, Vec<bool>) {
    let index: BTreeMap<NodeId, usize> = nodes.keys().enumerate().map(|(i, &id)| (id, i)).collect();
    let edges = connections
        .values()
        .filter_map(|c| Some((*index.get(&c.from)?, *index.get(&c.to)?)))
        .collect();
    let is_delay = nodes
        .values()
        .map(|entry| matches!(entry.kind, EntryKind::FeedbackDelay { .. }))
        .collect();
    (edges, is_delay)
}

/// Compiles the graph into a plan that counts dropped events in `overflows`. Nodes not yet in a
/// plan move into it; nodes already in the plan at `live` (its slot order) are marked to be
/// carried over from it.
///
/// # Errors
///
/// [`Error::CycleWithoutDelay`] if a cycle passes through no feedback delay, or
/// [`Error::Buffers`] if a buffer cannot be made.
pub(crate) fn compile(
    nodes: &mut BTreeMap<NodeId, Entry>,
    connections: &BTreeMap<ConnectionId, Connection>,
    controls: &BTreeMap<ConnectionId, ControlConnection>,
    live: &[NodeId],
    config: &Config,
    overflows: &Arc<AtomicU64>,
) -> Result<Plan, Error> {
    let (edges, is_delay) = numbered(nodes, connections);
    let analysis = topology::analyse(nodes.len(), &edges, &is_delay)?;
    let entries: Vec<&Entry> = nodes.values().collect();
    let delay_frames = |entry: &Entry| match entry.kind {
        EntryKind::FeedbackDelay { seconds } => feedback_delay_frames(seconds, config.sample_rate),
        _ => 0,
    };

    // Arrival times, visiting groups in order and each cycle's delays before its members.
    // `arrival` counts only playback inputs; `live_arrival` is a live-only node's own.
    let mut arrival = vec![0_usize; entries.len()];
    let mut live_arrival = vec![0_usize; entries.len()];
    let mut live_only = vec![false; entries.len()];
    let order = analysis.groups.iter().flat_map(|group| match group {
        Group::Single(node) => vec![*node],
        Group::Cycle { delays, members } => delays.iter().chain(members).copied().collect(),
    });
    for node in order {
        let inputs: Vec<usize> = edges
            .iter()
            .zip(&analysis.closes_loop)
            .filter(|&(&(_, to), &closes)| to == node && !closes)
            .map(|(&(from, _), _)| from)
            .collect();
        live_only[node] = match entries[node].kind {
            EntryKind::Source { live } => live,
            _ => !inputs.is_empty() && inputs.iter().all(|&from| live_only[from]),
        };
        let departure = |times: &[usize], from: usize| times[from] + entries[from].latency;
        arrival[node] = inputs
            .iter()
            .filter(|&&from| !live_only[from])
            .map(|&from| departure(&arrival, from))
            .max()
            .unwrap_or(0);
        live_arrival[node] = if live_only[node] {
            inputs
                .iter()
                .map(|&from| departure(&live_arrival, from))
                .max()
                .unwrap_or(0)
        } else {
            arrival[node]
        };
    }
    let is_sink = |node: usize| matches!(entries[node].kind, EntryKind::Sink);
    let latency = (0..entries.len())
        .filter(|&n| is_sink(n) && !live_only[n])
        .map(|n| arrival[n])
        .max()
        .unwrap_or(0);

    let mut slots = Vec::with_capacity(entries.len());
    for (&id, entry) in nodes.keys().zip(&entries) {
        let blocks = |ports: &[Port]| -> Result<Vec<PortBuffer>, Error> {
            ports
                .iter()
                .map(|port| {
                    Ok(match port.kind {
                        PortType::Audio => PortBuffer::Audio(audio_buffers::Block::new(
                            port.layout,
                            config.max_frames,
                        )?),
                        PortType::Event => PortBuffer::Events(EventBuffer::new(EVENT_CAPACITY)),
                        PortType::Control => PortBuffer::None,
                    })
                })
                .collect()
        };
        let kind = match entry.kind {
            EntryKind::FeedbackDelay { .. } => Kind::Delay(DelayLine::new(
                entry.outputs[0].layout.channels(),
                delay_frames(entry),
            )),
            EntryKind::Source { .. } => Kind::Source,
            EntryKind::Sink => Kind::Sink,
            EntryKind::Curve(_) => Kind::Curve,
            EntryKind::Processor(_) => Kind::Node(None),
        };
        slots.push(Slot {
            id,
            kind,
            inputs: blocks(&entry.inputs)?,
            outputs: blocks(&entry.outputs)?,
            feeds: Vec::new(),
            carry_from: live.iter().position(|&live_id| live_id == id),
        });
    }
    for ((&(from, to), connection), &closes) in edges
        .iter()
        .zip(connections.values())
        .zip(&analysis.closes_loop)
    {
        let target = if is_sink(to) { latency } else { arrival[to] };
        let departure = arrival[from] + entries[from].latency;
        let compensation = if closes || live_only[from] {
            0
        } else {
            target - departure
        };
        let port = &entries[from].outputs[connection.output];
        let signal = match port.kind {
            PortType::Event => Signal::Events(EventDelay::new(compensation)),
            _ => Signal::Audio {
                line: DelayLine::new(port.layout.channels(), compensation),
                pan: pan_gains(connection.pan),
            },
        };
        slots[to].feeds.push(Feed {
            from,
            output: connection.output,
            input: connection.input,
            signal,
        });
    }

    let steps = analysis
        .groups
        .into_iter()
        .map(|group| match group {
            Group::Single(node) => Step::Run(node),
            Group::Cycle { delays, members } => {
                let sub_block = delays
                    .iter()
                    .map(|&d| delay_frames(entries[d]))
                    .min()
                    .unwrap_or(config.max_frames);
                Step::Cycle {
                    delays,
                    members,
                    sub_block,
                }
            }
        })
        .collect();

    let slot_of: BTreeMap<NodeId, usize> =
        nodes.keys().enumerate().map(|(i, &id)| (id, i)).collect();
    let bindings = controls
        .values()
        .filter_map(|control| {
            let EntryKind::Curve(curve) = &nodes.get(&control.curve)?.kind else {
                return None;
            };
            let node = *slot_of.get(&control.node)?;
            let parameter = automatable(&entries[node].parameters, &control.parameter)?;
            Some(Binding::new(
                Arc::clone(curve),
                parameter.clone(),
                live_arrival[node],
            ))
        })
        .collect();

    // Nothing above can fail once nodes start moving, so none is lost.
    for (slot, entry) in slots.iter_mut().zip(nodes.values_mut()) {
        if let (Kind::Node(target), EntryKind::Processor(pending)) =
            (&mut slot.kind, &mut entry.kind)
            && pending.is_some()
        {
            *target = pending.take();
            slot.carry_from = None;
        }
    }
    Ok(Plan::new(
        slots,
        steps,
        bindings,
        latency,
        Arc::clone(overflows),
    ))
}
