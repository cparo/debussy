//! The graph as last synced: which graph node and connection stands for each model element.
//! [`Mirror::build`] makes a graph hold a [`Target`] from nothing; [`Mirror::sync`] changes it
//! from what it holds to a new target with as few graph calls as it can.

use std::collections::BTreeMap;

use audio_buffers::Layout;
use project_model::ConnectionId;
use signal_graph::{ConnectionId as GraphConnectionId, Graph, Node, NodeId as GraphNodeId};

use crate::target::{ConnectionTarget, Key, NodeTarget, Shape, Target};

/// Why the mirror could not make the graph hold a target.
#[derive(Debug)]
pub(crate) enum MirrorError {
    /// The graph refused a call.
    Graph(signal_graph::Error),
    /// A connection names an element the target has no node for; the model's reference checks
    /// make this impossible for a state it accepted.
    MissingEndpoint,
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

/// The graph's elements, by the model's keys and ids.
#[derive(Debug, Clone, Default)]
pub(crate) struct Mirror {
    pub nodes: BTreeMap<Key, MirroredNode>,
    pub connections: BTreeMap<ConnectionId, (GraphConnectionId, ConnectionTarget)>,
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
        shape => match processor(shape) {
            Some(node) => graph.add_node(node),
            None => graph.add_node(Box::new(audio_effects::Silence::new(1, 1))),
        },
    };
    set_parameters(graph, id, &target.parameters, None);
    Ok(id)
}

/// Sets every parameter of `id` whose value in `wanted` (or its default) differs from its value
/// in `previous`; with no `previous`, sets them all.
fn set_parameters(
    graph: &Graph,
    id: GraphNodeId,
    wanted: &BTreeMap<String, f32>,
    previous: Option<&BTreeMap<String, f32>>,
) {
    let Some(parameters) = graph.parameters(id) else {
        return;
    };
    for parameter in parameters {
        let info = parameter.info();
        let value = wanted.get(&info.name).copied().unwrap_or(info.default);
        let before = previous.map(|p| p.get(&info.name).copied().unwrap_or(info.default));
        if before != Some(value) {
            parameter.set(value);
        }
    }
}

/// Connects `target` in `graph`, whose nodes for its keys are in `nodes`.
fn connect(
    graph: &mut Graph,
    nodes: &BTreeMap<Key, MirroredNode>,
    target: &ConnectionTarget,
) -> Result<GraphConnectionId, MirrorError> {
    let (Some(from), Some(to)) = (nodes.get(&target.from), nodes.get(&target.to)) else {
        return Err(MirrorError::MissingEndpoint);
    };
    let id = graph.connect(from.id, target.output, to.id, target.input)?;
    if target.pan != 0.0 {
        graph.set_pan(id, target.pan)?;
    }
    Ok(id)
}

impl Mirror {
    /// Makes `graph` (empty of the model's elements) hold `target`.
    pub(crate) fn build(graph: &mut Graph, target: &Target) -> Result<Mirror, MirrorError> {
        let mut mirror = Mirror::default();
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
            let graph_id = connect(graph, &mirror.nodes, connection)?;
            mirror.connections.insert(*id, (graph_id, *connection));
        }
        Ok(mirror)
    }

    /// Changes `graph` from what this mirror holds to `target`. `target` must already have been
    /// built successfully into a scratch graph, so every call here succeeds.
    pub(crate) fn sync(&mut self, graph: &mut Graph, target: &Target) -> Result<(), MirrorError> {
        // Connections that go or change ports.
        let stale: Vec<ConnectionId> = self
            .connections
            .iter()
            .filter(|(id, (_, old))| {
                target
                    .connections
                    .get(id)
                    .is_none_or(|new| !new.same_ports(old))
            })
            .map(|(id, _)| *id)
            .collect();
        for id in stale {
            if let Some((graph_id, _)) = self.connections.remove(&id) {
                graph.disconnect(graph_id)?;
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
                let dropped = graph.replace_node(id, node)?;
                self.connections
                    .retain(|_, (graph_id, _)| !dropped.contains(graph_id));
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
                        set_parameters(
                            graph,
                            old.id,
                            &new.parameters,
                            Some(&old.target.parameters),
                        );
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
                Some((graph_id, old)) => {
                    if old.pan != new.pan {
                        graph.set_pan(*graph_id, new.pan)?;
                    }
                    *old = *new;
                }
                None => {
                    let graph_id = connect(graph, &self.nodes, new)?;
                    self.connections.insert(*id, (graph_id, *new));
                }
            }
        }
        Ok(())
    }

    /// Removes the node for `key` and forgets its connections.
    fn remove_node(&mut self, graph: &mut Graph, key: Key) -> Result<(), MirrorError> {
        if let Some(node) = self.nodes.remove(&key) {
            graph.remove_node(node.id)?;
            self.connections
                .retain(|_, (_, c)| c.from != key && c.to != key);
        }
        Ok(())
    }
}
