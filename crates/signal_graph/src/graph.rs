//! The graph as edited off the real-time thread, and committing it as a plan
//! (`docs/architecture/signal-graph.md` "Processing graph").

use std::collections::BTreeMap;
use std::sync::Arc;

use audio_buffers::{Consumer, Layout, Producer, queue};
use modulation::Curve;

use crate::compile;
use crate::delay::MAX_FEEDBACK_DELAY;
use crate::error::Error;
use crate::node::Node;
use crate::param::Parameter;
use crate::plan::{Kind, Plan};
use crate::runner::Runner;
use crate::types::{Config, ConnectionId, NodeId, Port};

/// What a node of the graph is.
pub(crate) enum EntryKind {
    /// A processor; holds the node until it is first committed, after which it lives in the plan.
    Processor(Option<Box<dyn Node>>),
    /// A feedback delay of a time in seconds.
    FeedbackDelay {
        /// As set, before rounding to frames.
        seconds: f64,
    },
    /// A signal the engine supplies, such as a track's playback.
    Source,
    /// A signal the engine takes, such as the project output.
    Sink,
    /// An automation curve, with one control output. Replaced whole on a change.
    Curve(Arc<Curve>),
}

/// A node of the graph and what it declared when added.
pub(crate) struct Entry {
    pub(crate) kind: EntryKind,
    pub(crate) inputs: Vec<Port>,
    pub(crate) outputs: Vec<Port>,
    pub(crate) latency: usize,
    pub(crate) parameters: Vec<Parameter>,
    /// Whether the node reads the project tempo.
    pub(crate) uses_tempo: bool,
}

/// A connection from a node's output to another node's input.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Connection {
    /// The node the signal leaves.
    pub from: NodeId,
    /// The output port it leaves by.
    pub output: usize,
    /// The node the signal enters.
    pub to: NodeId,
    /// The input port it enters by.
    pub input: usize,
    /// Where a mono signal entering a stereo input is panned, −1 (left) to 1 (right); 0 is
    /// centre. Unused otherwise.
    pub pan: f32,
}

/// A connection from an automation curve to a processor's parameter: the parameter's curve port
/// (`docs/architecture/automation.md` "Evaluation").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControlConnection {
    /// The curve.
    pub curve: NodeId,
    /// The processor.
    pub node: NodeId,
    /// The parameter's name.
    pub parameter: String,
}

/// The processing graph, edited off the real-time thread. Edits take effect on the real-time
/// thread when [`Graph::commit`] sends it a new plan.
pub struct Graph {
    config: Config,
    nodes: BTreeMap<NodeId, Entry>,
    connections: BTreeMap<ConnectionId, Connection>,
    /// Curves driving parameters, in the same id space as `connections`.
    controls: BTreeMap<ConnectionId, ControlConnection>,
    next_id: u64,
    /// The slot order of the last plan sent.
    live: Vec<NodeId>,
    /// The latency of the last plan sent.
    latency: usize,
    /// The project tempo, as the length of a whole note in frames.
    frames_per_whole_note: f64,
    plans: Producer<Box<Plan>>,
    retired: Consumer<Box<Plan>>,
}

impl Graph {
    /// An empty graph and the [`Runner`] that runs its plans on the real-time thread.
    ///
    /// # Errors
    ///
    /// [`Error::ZeroSampleRate`], or [`Error::Buffers`] if `max_frames` is outside the engine's
    /// block sizes or `plan_queue` is 0.
    pub fn new(config: Config) -> Result<(Graph, Runner), Error> {
        if config.sample_rate == 0 {
            return Err(Error::ZeroSampleRate);
        }
        // Checks `max_frames` the same way every buffer will.
        audio_buffers::Block::new(Layout::Mono, config.max_frames)?;
        let (plans, incoming) = queue::channel(config.plan_queue)?;
        let (retire, retired) = queue::channel(config.plan_queue + 1)?;
        let runner = Runner::new(
            Plan::new(Vec::new(), Vec::new(), Vec::new(), 0),
            incoming,
            retire,
            config,
        );
        let graph = Graph {
            config,
            nodes: BTreeMap::new(),
            connections: BTreeMap::new(),
            controls: BTreeMap::new(),
            next_id: 0,
            live: Vec::new(),
            latency: 0,
            frames_per_whole_note: default_whole_note(config.sample_rate),
            plans,
            retired,
        };
        Ok((graph, runner))
    }

    /// The graph's settings.
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// The project tempo processors are given, as the length of a whole note in frames. Until
    /// [`Graph::set_tempo`], 120 BPM in 4/4: two seconds.
    pub fn tempo(&self) -> f64 {
        self.frames_per_whole_note
    }

    /// Sets the project tempo, as the length of a whole note in frames, for processors added from
    /// now on. Returns the processors already in the graph that use the tempo, for the caller to
    /// replace with [`Graph::replace_node`] so they are prepared at it; empty if the tempo is
    /// unchanged or not a positive finite length.
    pub fn set_tempo(&mut self, frames_per_whole_note: f64) -> Vec<NodeId> {
        if !(frames_per_whole_note.is_finite() && frames_per_whole_note > 0.0)
            || frames_per_whole_note == self.frames_per_whole_note
        {
            return Vec::new();
        }
        self.frames_per_whole_note = frames_per_whole_note;
        self.nodes
            .iter()
            .filter(|(_, entry)| entry.uses_tempo)
            .map(|(&id, _)| id)
            .collect()
    }

    /// Adds a processor, preparing it for the graph's rate and block size.
    pub fn add_node(&mut self, node: Box<dyn Node>) -> NodeId {
        let entry = self.declare(node, &[]);
        self.insert(entry)
    }

    /// Replaces processor `id` with `node`, for when its ports, latency or parameters change.
    /// The id stays. Each connection whose port still exists with the same signal type is kept;
    /// the others are removed and returned. Each parameter takes the value of the old parameter
    /// of the same name, clamped to its range; the rest keep their defaults. The new node starts
    /// with its own state, and the old one is dropped when its plan comes back from the real-time
    /// thread.
    ///
    /// # Errors
    ///
    /// [`Error::UnknownNode`], or [`Error::NotAProcessor`] for a source, sink or feedback delay.
    pub fn replace_node(
        &mut self,
        id: NodeId,
        node: Box<dyn Node>,
    ) -> Result<Vec<ConnectionId>, Error> {
        if !matches!(self.entry(id)?.kind, EntryKind::Processor(_)) {
            return Err(Error::NotAProcessor(id));
        }
        let entry = self.declare(node, &self.entry(id)?.parameters);
        let old = self.nodes.insert(id, entry).ok_or(Error::UnknownNode(id))?;
        let new = &self.nodes[&id];
        let fits = |ports: &[Port], old_ports: &[Port], index: usize| {
            let (port, old_port) = (ports.get(index), old_ports.get(index));
            matches!((port, old_port), (Some(p), Some(o)) if p.kind == o.kind)
        };
        let dropped: Vec<ConnectionId> = self
            .connections
            .iter()
            .filter(|(_, c)| {
                (c.from == id && !fits(&new.outputs, &old.outputs, c.output))
                    || (c.to == id && !fits(&new.inputs, &old.inputs, c.input))
            })
            .map(|(&connection, _)| connection)
            .collect();
        for connection in &dropped {
            self.connections.remove(connection);
        }
        let new_parameters = &self.nodes[&id].parameters;
        let lost_controls: Vec<ConnectionId> = self
            .controls
            .iter()
            .filter(|(_, c)| c.node == id && automatable(new_parameters, &c.parameter).is_none())
            .map(|(&connection, _)| connection)
            .collect();
        for connection in &lost_controls {
            self.controls.remove(connection);
        }
        let mut dropped = dropped;
        dropped.extend(lost_controls);
        Ok(dropped)
    }

    /// Prepares a processor for the graph and records what it declares.
    /// Parameters are read first and given the values of same-named parameters in `previous`,
    /// so that `prepare` sees them.
    fn declare(&self, mut node: Box<dyn Node>, previous: &[Parameter]) -> Entry {
        let parameters = node.parameters();
        for parameter in &parameters {
            let name = &parameter.info().name;
            if let Some(old) = previous.iter().find(|p| &p.info().name == name) {
                parameter.set(old.base());
            }
        }
        node.set_tempo(self.frames_per_whole_note);
        node.prepare(self.config.sample_rate, self.config.max_frames);
        Entry {
            uses_tempo: node.uses_tempo(),
            inputs: node.inputs(),
            outputs: node.outputs(),
            latency: node.latency(),
            parameters,
            kind: EntryKind::Processor(Some(node)),
        }
    }

    /// Adds a feedback delay of `seconds` with one input and one output of `layout`. A delay
    /// shorter than 1 ms is raised to 1 ms.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidDelay`] if `seconds` is not finite or is above 10 s.
    pub fn add_feedback_delay(&mut self, layout: Layout, seconds: f64) -> Result<NodeId, Error> {
        check_delay(seconds)?;
        Ok(self.insert(Entry {
            kind: EntryKind::FeedbackDelay { seconds },
            inputs: vec![Port::audio("in", layout)],
            outputs: vec![Port::audio("out", layout)],
            latency: 0,
            parameters: Vec::new(),
            uses_tempo: false,
        }))
    }

    /// Changes a feedback delay's time.
    ///
    /// # Errors
    ///
    /// [`Error::UnknownNode`], [`Error::NotAFeedbackDelay`], or [`Error::InvalidDelay`].
    pub fn set_feedback_delay(&mut self, id: NodeId, seconds: f64) -> Result<(), Error> {
        check_delay(seconds)?;
        match &mut self.entry_mut(id)?.kind {
            EntryKind::FeedbackDelay { seconds: current } => {
                *current = seconds;
                Ok(())
            }
            _ => Err(Error::NotAFeedbackDelay(id)),
        }
    }

    /// Adds a source: a signal of `layout` the engine writes each block with
    /// [`Runner::source_mut`].
    pub fn add_source(&mut self, layout: Layout) -> NodeId {
        self.insert(Entry {
            kind: EntryKind::Source,
            inputs: Vec::new(),
            outputs: vec![Port::audio("out", layout)],
            latency: 0,
            parameters: Vec::new(),
            uses_tempo: false,
        })
    }

    /// Adds a sink: a signal of `layout` the engine reads each block with [`Runner::sink`].
    pub fn add_sink(&mut self, layout: Layout) -> NodeId {
        self.insert(Entry {
            kind: EntryKind::Sink,
            inputs: vec![Port::audio("in", layout)],
            outputs: Vec::new(),
            latency: 0,
            parameters: Vec::new(),
            uses_tempo: false,
        })
    }

    /// Adds an automation curve: a source with one control output, which drives the parameters
    /// it is connected to with [`Graph::connect_curve`].
    pub fn add_curve(&mut self, curve: Arc<Curve>) -> NodeId {
        self.insert(Entry {
            kind: EntryKind::Curve(curve),
            inputs: Vec::new(),
            outputs: vec![Port::control("curve")],
            latency: 0,
            parameters: Vec::new(),
            uses_tempo: false,
        })
    }

    /// Replaces curve `id`'s points with `curve`. The id and its connections stay.
    ///
    /// # Errors
    ///
    /// [`Error::UnknownNode`] or [`Error::NotACurve`].
    pub fn set_curve(&mut self, id: NodeId, curve: Arc<Curve>) -> Result<(), Error> {
        match &mut self.entry_mut(id)?.kind {
            EntryKind::Curve(current) => {
                *current = curve;
                Ok(())
            }
            _ => Err(Error::NotACurve(id)),
        }
    }

    /// Connects curve `curve` to parameter `parameter` of processor `node`, which the curve then
    /// drives. A parameter takes one curve; a curve may drive many.
    ///
    /// # Errors
    ///
    /// [`Error::UnknownNode`], [`Error::NotACurve`], [`Error::NotAProcessor`],
    /// [`Error::UnknownParameter`], [`Error::NotAutomatable`], or [`Error::AlreadyConnected`]
    /// if the parameter already has a curve.
    pub fn connect_curve(
        &mut self,
        curve: NodeId,
        node: NodeId,
        parameter: &str,
    ) -> Result<ConnectionId, Error> {
        if !matches!(self.entry(curve)?.kind, EntryKind::Curve(_)) {
            return Err(Error::NotACurve(curve));
        }
        let target = self.entry(node)?;
        if !matches!(target.kind, EntryKind::Processor(_)) {
            return Err(Error::NotAProcessor(node));
        }
        match target
            .parameters
            .iter()
            .find(|p| p.info().name == parameter)
        {
            None => {
                return Err(Error::UnknownParameter {
                    node,
                    name: parameter.to_owned(),
                });
            }
            Some(p) if !p.info().is_automatable() => {
                return Err(Error::NotAutomatable {
                    node,
                    name: parameter.to_owned(),
                });
            }
            Some(_) => {}
        }
        if self
            .controls
            .values()
            .any(|c| c.node == node && c.parameter == parameter)
        {
            return Err(Error::AlreadyConnected);
        }
        let id = ConnectionId(self.take_id());
        self.controls.insert(
            id,
            ControlConnection {
                curve,
                node,
                parameter: parameter.to_owned(),
            },
        );
        Ok(id)
    }

    /// A curve-to-parameter connection, by id.
    pub fn control_connection(&self, id: ConnectionId) -> Option<&ControlConnection> {
        self.controls.get(&id)
    }

    /// Every curve-to-parameter connection, in id order.
    pub fn control_connections(
        &self,
    ) -> impl Iterator<Item = (ConnectionId, &ControlConnection)> + '_ {
        self.controls.iter().map(|(id, c)| (*id, c))
    }

    /// Removes a node and its connections. A node already in a plan is dropped when that plan
    /// comes back from the real-time thread.
    ///
    /// # Errors
    ///
    /// [`Error::UnknownNode`].
    pub fn remove_node(&mut self, id: NodeId) -> Result<(), Error> {
        self.nodes.remove(&id).ok_or(Error::UnknownNode(id))?;
        self.connections.retain(|_, c| c.from != id && c.to != id);
        self.controls.retain(|_, c| c.curve != id && c.node != id);
        Ok(())
    }

    /// Connects output `output` of `from` to input `input` of `to`, panned centre.
    ///
    /// # Errors
    ///
    /// [`Error::UnknownNode`], [`Error::NoSuchOutput`], [`Error::NoSuchInput`],
    /// [`Error::PortTypeMismatch`], [`Error::AlreadyConnected`], or [`Error::CycleWithoutDelay`]
    /// if the connection would close a cycle that passes through no feedback delay.
    pub fn connect(
        &mut self,
        from: NodeId,
        output: usize,
        to: NodeId,
        input: usize,
    ) -> Result<ConnectionId, Error> {
        let source = self.entry(from)?.outputs.get(output);
        let source = source.ok_or(Error::NoSuchOutput {
            node: from,
            port: output,
        })?;
        let target = self.entry(to)?.inputs.get(input);
        let target = target.ok_or(Error::NoSuchInput {
            node: to,
            port: input,
        })?;
        if source.kind != target.kind {
            return Err(Error::PortTypeMismatch);
        }
        let connection = Connection {
            from,
            output,
            to,
            input,
            pan: 0.0,
        };
        let same = |c: &Connection| (c.from, c.output, c.to, c.input) == (from, output, to, input);
        if self.connections.values().any(same) {
            return Err(Error::AlreadyConnected);
        }
        let id = ConnectionId(self.take_id());
        self.connections.insert(id, connection);
        if let Err(error) = compile::check_cycles(&self.nodes, &self.connections) {
            self.connections.remove(&id);
            return Err(error);
        }
        Ok(id)
    }

    /// Sets where a mono signal entering a stereo input through `id` is panned.
    ///
    /// # Errors
    ///
    /// [`Error::UnknownConnection`].
    pub fn set_pan(&mut self, id: ConnectionId, pan: f32) -> Result<(), Error> {
        let connection = self
            .connections
            .get_mut(&id)
            .ok_or(Error::UnknownConnection(id))?;
        connection.pan = if pan.is_nan() {
            0.0
        } else {
            pan.clamp(-1.0, 1.0)
        };
        Ok(())
    }

    /// Removes a connection, audio or control.
    ///
    /// # Errors
    ///
    /// [`Error::UnknownConnection`].
    pub fn disconnect(&mut self, id: ConnectionId) -> Result<(), Error> {
        let removed = self.connections.remove(&id).is_some() || self.controls.remove(&id).is_some();
        match removed {
            true => Ok(()),
            false => Err(Error::UnknownConnection(id)),
        }
    }

    /// A connection, by id.
    pub fn connection(&self, id: ConnectionId) -> Option<&Connection> {
        self.connections.get(&id)
    }

    /// A node's input ports.
    pub fn inputs(&self, id: NodeId) -> Option<&[Port]> {
        self.nodes.get(&id).map(|entry| entry.inputs.as_slice())
    }

    /// A node's output ports.
    pub fn outputs(&self, id: NodeId) -> Option<&[Port]> {
        self.nodes.get(&id).map(|entry| entry.outputs.as_slice())
    }

    /// A node's parameters, for the UI to set.
    pub fn parameters(&self, id: NodeId) -> Option<&[Parameter]> {
        self.nodes.get(&id).map(|entry| entry.parameters.as_slice())
    }

    /// Compiles the graph into a plan and sends it to the real-time thread, which starts it at
    /// its next block. First drops the plans the real-time thread has finished with.
    ///
    /// # Errors
    ///
    /// [`Error::Busy`] if earlier plans are still waiting; the graph is unchanged and the commit
    /// can be retried. [`Error::Buffers`] if a buffer cannot be made.
    pub fn commit(&mut self) -> Result<(), Error> {
        self.collect();
        let plan = compile::compile(
            &mut self.nodes,
            &self.connections,
            &self.controls,
            &self.live,
            &self.config,
        )?;
        let (ids, latency) = (plan.ids(), plan.latency);
        match self.plans.try_push(Box::new(plan)) {
            Ok(()) => {
                self.live = ids;
                self.latency = latency;
                Ok(())
            }
            Err(plan) => {
                self.restore(*plan);
                Err(Error::Busy)
            }
        }
    }

    /// Drops the plans the real-time thread has finished with, and the nodes removed with them.
    pub fn collect(&mut self) {
        self.retired.drain().for_each(drop);
    }

    /// Every node's id, in id order.
    pub fn node_ids(&self) -> impl Iterator<Item = NodeId> + '_ {
        self.nodes.keys().copied()
    }

    /// Every connection, in id order.
    pub fn connections(&self) -> impl Iterator<Item = (ConnectionId, &Connection)> + '_ {
        self.connections.iter().map(|(id, c)| (*id, c))
    }

    /// The latency from sources to sinks of the last committed plan, in frames.
    pub fn latency(&self) -> usize {
        self.latency
    }

    /// Takes back the nodes of a plan that was never sent.
    fn restore(&mut self, plan: Plan) {
        for slot in plan.slots {
            if let (Kind::Node(Some(node)), Some(entry)) = (slot.kind, self.nodes.get_mut(&slot.id))
            {
                entry.kind = EntryKind::Processor(Some(node));
            }
        }
    }

    fn insert(&mut self, entry: Entry) -> NodeId {
        let id = NodeId(self.take_id());
        self.nodes.insert(id, entry);
        id
    }

    fn take_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    fn entry(&self, id: NodeId) -> Result<&Entry, Error> {
        self.nodes.get(&id).ok_or(Error::UnknownNode(id))
    }

    fn entry_mut(&mut self, id: NodeId) -> Result<&mut Entry, Error> {
        self.nodes.get_mut(&id).ok_or(Error::UnknownNode(id))
    }
}

/// The length of a whole note at 120 BPM in 4/4 at `sample_rate`: two seconds.
fn default_whole_note(sample_rate: u32) -> f64 {
    2.0 * f64::from(sample_rate)
}

/// The parameter named `name` among `parameters`, if a curve may drive it.
pub(crate) fn automatable<'a>(parameters: &'a [Parameter], name: &str) -> Option<&'a Parameter> {
    parameters
        .iter()
        .find(|p| p.info().name == name && p.info().is_automatable())
}

/// Checks a feedback-delay time is finite and within the longest allowed.
fn check_delay(seconds: f64) -> Result<(), Error> {
    if seconds.is_finite() && seconds <= MAX_FEEDBACK_DELAY {
        Ok(())
    } else {
        Err(Error::InvalidDelay(seconds))
    }
}
