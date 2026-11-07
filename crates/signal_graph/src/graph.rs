//! The graph as edited off the real-time thread, and committing it as a plan
//! (`docs/architecture/signal-graph.md` "Processing graph").

use std::collections::BTreeMap;

use audio_buffers::{Consumer, Layout, Producer, queue};

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
}

/// A node of the graph and what it declared when added.
pub(crate) struct Entry {
    pub(crate) kind: EntryKind,
    pub(crate) inputs: Vec<Port>,
    pub(crate) outputs: Vec<Port>,
    pub(crate) latency: usize,
    pub(crate) parameters: Vec<Parameter>,
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

/// The processing graph, edited off the real-time thread. Edits take effect on the real-time
/// thread when [`Graph::commit`] sends it a new plan.
pub struct Graph {
    config: Config,
    nodes: BTreeMap<NodeId, Entry>,
    connections: BTreeMap<ConnectionId, Connection>,
    next_id: u64,
    /// The slot order of the last plan sent.
    live: Vec<NodeId>,
    /// The latency of the last plan sent.
    latency: usize,
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
            Plan::new(Vec::new(), Vec::new(), 0),
            incoming,
            retire,
            config,
        );
        let graph = Graph {
            config,
            nodes: BTreeMap::new(),
            connections: BTreeMap::new(),
            next_id: 0,
            live: Vec::new(),
            latency: 0,
            plans,
            retired,
        };
        Ok((graph, runner))
    }

    /// The graph's settings.
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Adds a processor, preparing it for the graph's rate and block size.
    pub fn add_node(&mut self, mut node: Box<dyn Node>) -> NodeId {
        node.prepare(self.config.sample_rate, self.config.max_frames);
        let entry = Entry {
            inputs: node.inputs(),
            outputs: node.outputs(),
            latency: node.latency(),
            parameters: node.parameters(),
            kind: EntryKind::Processor(Some(node)),
        };
        self.insert(entry)
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
        })
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

    /// Removes a connection.
    ///
    /// # Errors
    ///
    /// [`Error::UnknownConnection`].
    pub fn disconnect(&mut self, id: ConnectionId) -> Result<(), Error> {
        self.connections
            .remove(&id)
            .map(|_| ())
            .ok_or(Error::UnknownConnection(id))
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
        let plan = compile::compile(&mut self.nodes, &self.connections, &self.live, &self.config)?;
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

/// Checks a feedback-delay time is finite and within the longest allowed.
fn check_delay(seconds: f64) -> Result<(), Error> {
    if seconds.is_finite() && seconds <= MAX_FEEDBACK_DELAY {
        Ok(())
    } else {
        Err(Error::InvalidDelay(seconds))
    }
}
