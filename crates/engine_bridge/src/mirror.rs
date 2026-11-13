//! The graph as last synced: which graph node and connection stands for each model element.
//! [`Mirror::build`] makes a graph hold a [`Target`] from nothing; [`Mirror::sync`] changes it
//! from what it holds to a new target with as few graph calls as it can.

use std::collections::BTreeMap;

use audio_buffers::Layout;
use project_model::{ConnectionId, Side, Tempo};
use signal_graph::{ConnectionId as GraphConnectionId, Graph, Node, NodeId as GraphNodeId};

use crate::target::{ConnectionTarget, Key, NodeTarget, Shape, Target, frames_per_whole_note};

/// Why the mirror could not make the graph hold a target.
#[derive(Debug)]
pub(crate) enum MirrorError {
    /// The graph refused a call.
    Graph(signal_graph::Error),
    /// A connection names an element the target has no node for; the model's reference checks
    /// make this impossible for a state it accepted.
    MissingEndpoint,
    /// A connection names a side of a port that is mono.
    SideOfMonoPort,
}

impl From<signal_graph::Error> for MirrorError {
    fn from(error: signal_graph::Error) -> Self {
        MirrorError::Graph(error)
    }
}

/// A graph node standing for a model element, and what it was made as.
#[derive(Debug, Clone)]
pub(crate) struct MirroredNode {
    pub id: GraphNodeId,
    pub target: NodeTarget,
}

/// The graph connections and helper nodes standing for one model connection: for a sided end,
/// a `Pick` or `Place` node between it and the main connection (ADR-0058).
#[derive(Debug, Clone)]
pub(crate) struct MirroredConnection {
    /// The connection the pan is set on.
    pub main: GraphConnectionId,
    /// The connections to and from helper nodes.
    pub extra: Vec<GraphConnectionId>,
    /// The helper nodes.
    pub helpers: Vec<GraphNodeId>,
    pub target: ConnectionTarget,
}

/// The graph's elements, by the model's keys and ids.
#[derive(Debug, Clone, Default)]
pub(crate) struct Mirror {
    pub nodes: BTreeMap<Key, MirroredNode>,
    pub connections: BTreeMap<ConnectionId, MirroredConnection>,
    /// The tempo the graph was last given.
    pub tempo: Tempo,
}

/// The processor for a shape; `None` for sources, sinks and delays, which the graph makes.
fn processor(shape: &Shape) -> Option<Box<dyn Node>> {
    match shape {
        Shape::Effect(kind) => audio_effects::effect(kind),
        Shape::Unknown { inputs, outputs } => {
            Some(Box::new(audio_effects::Silence::new(*inputs, *outputs)))
        }
        // A mixer of 0 channels is refused by the model; a failure here becomes a stand-in.
        Shape::Mixer(channels) => audio_effects::Mixer::new(*channels)
            .map(|m| Box::new(m) as Box<dyn Node>)
            .ok()
            .or_else(|| Some(Box::new(audio_effects::Silence::new(1, 1)))),
        Shape::Bus(layout) => Some(Box::new(audio_effects::Bus::new(*layout))),
        Shape::Source(_) | Shape::Sink | Shape::Delay(_) => None,
    }
}

/// Adds the node for `target` to `graph`.
fn add(graph: &mut Graph, target: &NodeTarget) -> Result<GraphNodeId, MirrorError> {
    let id = match &target.shape {
        Shape::Source(layout) => graph.add_source(*layout),
        Shape::Sink => graph.add_sink(Layout::Stereo),
        Shape::Delay(layout) => graph.add_feedback_delay(*layout, target.seconds.unwrap_or(0.0))?,
        shape => {
            let node =
                processor(shape).unwrap_or_else(|| Box::new(audio_effects::Silence::new(1, 1)));
            // Values set before the node is added are what it starts from, with no ramp in.
            set_values(&node.parameters(), &target.parameters);
            graph.add_node(node)
        }
    };
    Ok(id)
}

/// Sets each of `parameters` to its value in `wanted`, or its default.
fn set_values(parameters: &[signal_graph::Parameter], wanted: &BTreeMap<String, f32>) {
    for parameter in parameters {
        let info = parameter.info();
        parameter.set(wanted.get(&info.name).copied().unwrap_or(info.default));
    }
}

/// Sets every parameter of `id` whose value in `wanted` (or its default) differs from its value
/// in `previous`; with no `previous`, sets them all. Returns whether a parameter that sets the
/// node's latency changed value, so the node must be replaced to re-read it.
fn set_parameters(
    graph: &Graph,
    id: GraphNodeId,
    wanted: &BTreeMap<String, f32>,
    previous: Option<&BTreeMap<String, f32>>,
) -> bool {
    let Some(parameters) = graph.parameters(id) else {
        return false;
    };
    let mut latency_changed = false;
    for parameter in parameters {
        let info = parameter.info();
        let value = wanted.get(&info.name).copied().unwrap_or(info.default);
        let before = previous.map(|p| p.get(&info.name).copied().unwrap_or(info.default));
        if before != Some(value) {
            parameter.set(value);
            // Compared with the model's last value, not the handle's: a control may have set the
            // handle live before the edit was recorded (ADR-0043).
            latency_changed |= info.sets_latency && before.is_some();
        }
    }
    latency_changed
}

/// Whether port `port` of `node` (an output if `output`) is mono, as the graph declares it.
fn is_mono(graph: &Graph, node: GraphNodeId, output: bool, port: usize) -> bool {
    let ports = if output {
        graph.outputs(node)
    } else {
        graph.inputs(node)
    };
    ports
        .and_then(|p| p.get(port))
        .is_some_and(|p| p.layout == Layout::Mono)
}

/// Connects `target` in `graph`, whose nodes for its keys are in `nodes`, with a `Pick` after a
/// sided output and a `Place` before a sided input.
fn connect(
    graph: &mut Graph,
    nodes: &BTreeMap<Key, MirroredNode>,
    target: &ConnectionTarget,
) -> Result<MirroredConnection, MirrorError> {
    let (Some(from), Some(to)) = (nodes.get(&target.from), nodes.get(&target.to)) else {
        return Err(MirrorError::MissingEndpoint);
    };
    if (target.from_side.is_some() && is_mono(graph, from.id, true, target.output))
        || (target.to_side.is_some() && is_mono(graph, to.id, false, target.input))
    {
        return Err(MirrorError::SideOfMonoPort);
    }
    let mut helpers = Vec::new();
    let mut extra = Vec::new();
    let mut made = |graph: &mut Graph| -> Result<GraphConnectionId, signal_graph::Error> {
        let (mut source, mut output) = (from.id, target.output);
        if let Some(side) = target.from_side {
            let pick = graph.add_node(Box::new(audio_effects::Pick::new(side == Side::Right)));
            helpers.push(pick);
            extra.push(graph.connect(source, output, pick, 0)?);
            (source, output) = (pick, 0);
        }
        let (mut sink, mut input) = (to.id, target.input);
        if let Some(side) = target.to_side {
            let place = graph.add_node(Box::new(audio_effects::Place::new(side == Side::Right)));
            helpers.push(place);
            extra.push(graph.connect(place, 0, sink, input)?);
            (sink, input) = (place, 0);
        }
        let main = graph.connect(source, output, sink, input)?;
        if target.pan != 0.0 {
            graph.set_pan(main, target.pan)?;
        }
        Ok(main)
    };
    let result = made(graph);
    match result {
        Ok(main) => Ok(MirroredConnection {
            main,
            extra,
            helpers,
            target: *target,
        }),
        Err(error) => {
            // Leave nothing half made behind.
            for helper in helpers {
                let _ = graph.remove_node(helper);
            }
            Err(error.into())
        }
    }
}

/// Removes a mirrored connection's graph connections and helper nodes, ignoring parts already
/// gone with a removed node.
fn remove_connection(graph: &mut Graph, connection: &MirroredConnection) {
    for &helper in &connection.helpers {
        let _ = graph.remove_node(helper);
    }
    for &id in std::iter::once(&connection.main).chain(&connection.extra) {
        if graph.connection(id).is_some() {
            let _ = graph.disconnect(id);
        }
    }
}

impl Mirror {
    /// Makes `graph` (empty of the model's elements) hold `target`.
    pub(crate) fn build(graph: &mut Graph, target: &Target) -> Result<Mirror, MirrorError> {
        let mut mirror = Mirror {
            tempo: target.tempo,
            ..Mirror::default()
        };
        graph.set_tempo(frames_per_whole_note(
            &target.tempo,
            graph.config().sample_rate,
        ));
        for (key, node) in &target.nodes {
            let id = add(graph, node)?;
            mirror.nodes.insert(
                *key,
                MirroredNode {
                    id,
                    target: node.clone(),
                },
            );
        }
        for (id, connection) in &target.connections {
            let mirrored = connect(graph, &mirror.nodes, connection)?;
            mirror.connections.insert(*id, mirrored);
        }
        Ok(mirror)
    }

    /// Changes `graph` from what this mirror holds to `target`. `target` must already have been
    /// built successfully into a scratch graph, so every call here succeeds.
    pub(crate) fn sync(&mut self, graph: &mut Graph, target: &Target) -> Result<(), MirrorError> {
        self.sync_tempo(graph, target.tempo)?;

        // Connections that go or change ports.
        let stale: Vec<ConnectionId> = self
            .connections
            .iter()
            .filter(|(id, old)| {
                target
                    .connections
                    .get(id)
                    .is_none_or(|new| !new.same_ports(&old.target))
            })
            .map(|(id, _)| *id)
            .collect();
        for id in stale {
            if let Some(old) = self.connections.remove(&id) {
                remove_connection(graph, &old);
            }
        }

        // Nodes that go, or change in a way that needs a new node.
        let keys: Vec<Key> = self.nodes.keys().copied().collect();
        for key in keys {
            let Some(old) = self.nodes.get(&key) else {
                continue;
            };
            let replace = match target.nodes.get(&key) {
                None => {
                    self.remove_node(graph, key)?;
                    continue;
                }
                Some(new) if new.shape == old.target.shape => None,
                Some(new) => Some(new.clone()),
            };
            let Some(new) = replace else {
                continue;
            };
            if old.target.shape.is_processor() && new.shape.is_processor() {
                let id = old.id;
                let node = processor(&new.shape)
                    .unwrap_or_else(|| Box::new(audio_effects::Silence::new(1, 1)));
                // replace_node carries values over by name; the model's values win after.
                let dropped = graph.replace_node(id, node)?;
                // A model connection that lost any of its graph connections is remade later.
                let broken: Vec<ConnectionId> = self
                    .connections
                    .iter()
                    .filter(|(_, c)| {
                        std::iter::once(&c.main)
                            .chain(&c.extra)
                            .any(|g| dropped.contains(g))
                    })
                    .map(|(id, _)| *id)
                    .collect();
                for broken in broken {
                    if let Some(old) = self.connections.remove(&broken) {
                        remove_connection(graph, &old);
                    }
                }
                set_parameters(graph, id, &new.parameters, None);
                self.nodes.insert(key, MirroredNode { id, target: new });
            } else {
                self.remove_node(graph, key)?;
            }
        }

        // New nodes, and changes to the ones kept.
        for (key, new) in &target.nodes {
            match self.nodes.get_mut(key) {
                None => {
                    let id = add(graph, new)?;
                    self.nodes.insert(
                        *key,
                        MirroredNode {
                            id,
                            target: new.clone(),
                        },
                    );
                }
                Some(old) => {
                    if old.target.parameters != new.parameters {
                        let relatch = set_parameters(
                            graph,
                            old.id,
                            &new.parameters,
                            Some(&old.target.parameters),
                        );
                        // The new node takes the values just set, by name, before it is
                        // prepared, so its latency is read at the new value.
                        if relatch && let Some(node) = processor(&new.shape) {
                            graph.replace_node(old.id, node)?;
                        }
                    }
                    if let (Some(seconds), true) = (new.seconds, old.target.seconds != new.seconds)
                    {
                        graph.set_feedback_delay(old.id, seconds)?;
                    }
                    old.target = new.clone();
                }
            }
        }

        // New connections, and pans.
        for (id, new) in &target.connections {
            match self.connections.get_mut(id) {
                Some(old) => {
                    if old.target.pan != new.pan {
                        graph.set_pan(old.main, new.pan)?;
                    }
                    old.target = *new;
                }
                None => {
                    let mirrored = connect(graph, &self.nodes, new)?;
                    self.connections.insert(*id, mirrored);
                }
            }
        }
        Ok(())
    }

    /// Gives `graph` the tempo `tempo` and replaces each node that uses it, as the tempo changes
    /// only while stopped (ADR-0082). Nodes added after this are prepared at it.
    fn sync_tempo(&mut self, graph: &mut Graph, tempo: Tempo) -> Result<(), MirrorError> {
        if tempo == self.tempo {
            return Ok(());
        }
        self.tempo = tempo;
        let users = graph.set_tempo(frames_per_whole_note(&tempo, graph.config().sample_rate));
        for node in self.nodes.values().filter(|n| users.contains(&n.id)) {
            if let Some(processor) = processor(&node.target.shape) {
                // Same kind, so the same ports: no connection is dropped. Values carry over by
                // name.
                graph.replace_node(node.id, processor)?;
            }
        }
        Ok(())
    }

    /// Removes the node for `key` and forgets its connections.
    fn remove_node(&mut self, graph: &mut Graph, key: Key) -> Result<(), MirrorError> {
        if let Some(node) = self.nodes.remove(&key) {
            let gone: Vec<ConnectionId> = self
                .connections
                .iter()
                .filter(|(_, c)| c.target.from == key || c.target.to == key)
                .map(|(id, _)| *id)
                .collect();
            for id in gone {
                if let Some(old) = self.connections.remove(&id) {
                    remove_connection(graph, &old);
                }
            }
            graph.remove_node(node.id)?;
        }
        Ok(())
    }
}
