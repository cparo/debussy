//! [`Bridge`]: the running processing graph kept in step with the project's routing
//! (`docs/architecture/project.md` "Routing", ADR-0054).
//!
//! Every change is checked before it reaches the running graph: the target state is first built
//! into a scratch graph, which applies all of `signal_graph`'s rules (ports, cycles, delay
//! lengths). Only if that succeeds is the running graph changed, by the fewest calls from what it
//! holds, and the new plan committed. A change the scratch graph refuses leaves both the running
//! graph and the model as they were.

use std::collections::BTreeMap;

use project_model::{NodeId, Session, State, TrackId};
use signal_graph::{Config, Graph, NodeId as GraphNodeId, Parameter, Runner};

use crate::error::Error;
use crate::mirror::{Mirror, MirrorError};
use crate::target::{Key, Target};

/// The running processing graph for a project, kept in step with its routing.
pub struct Bridge {
    graph: Graph,
    mirror: Mirror,
    /// Whether the last sync's plan is still waiting to be committed.
    pending: bool,
}

/// A graph built from a state, with nothing running it yet: for offline render.
pub struct RenderGraph {
    /// The graph; keep it alive while rendering.
    pub graph: Graph,
    /// Its runner, for `audio_engine::render`.
    pub runner: Runner,
    /// The output sink.
    pub output: GraphNodeId,
    /// Each track's source.
    pub sources: BTreeMap<TrackId, GraphNodeId>,
}

fn refused(error: MirrorError) -> Error {
    match error {
        MirrorError::Graph(error) => Error::Refused(error),
        MirrorError::MissingEndpoint => Error::Inconsistent,
        MirrorError::SideOfMonoPort => Error::SideOfMonoPort,
    }
}

fn failed(error: MirrorError) -> Error {
    match error {
        MirrorError::Graph(error) => Error::Graph(error),
        MirrorError::MissingEndpoint | MirrorError::SideOfMonoPort => Error::Inconsistent,
    }
}

/// Builds `state`'s routing into a new graph, checking it.
fn build(state: &State, config: Config) -> Result<(Graph, Runner, Mirror), Error> {
    let (mut graph, runner) = Graph::new(config).map_err(Error::Graph)?;
    let mirror = Mirror::build(&mut graph, &Target::of(state)).map_err(refused)?;
    Ok((graph, runner, mirror))
}

/// The sources of every track in `mirror`.
fn sources(mirror: &Mirror) -> BTreeMap<TrackId, GraphNodeId> {
    mirror
        .nodes
        .iter()
        .filter_map(|(key, node)| match key {
            Key::Track(track) => Some((*track, node.id)),
            _ => None,
        })
        .collect()
}

impl Bridge {
    /// The graph for `state`, committed, and its runner for the real-time side.
    ///
    /// # Errors
    ///
    /// [`Error::Refused`] if the graph cannot hold the state's routing; [`Error::Graph`].
    pub fn new(state: &State, config: Config) -> Result<(Bridge, Runner), Error> {
        let (graph, runner, mirror) = build(state, config)?;
        let mut bridge = Bridge {
            graph,
            mirror,
            pending: true,
        };
        bridge.commit()?;
        Ok((bridge, runner))
    }

    /// Makes one change to `session` through `change` (one undo step, such as `connect` or
    /// `apply`) and brings the graph in step. If the graph refuses the result, the change is
    /// reverted and forgotten, and the graph is untouched.
    ///
    /// # Errors
    ///
    /// [`Error::Model`] if the model refuses it; [`Error::Refused`] if the graph does.
    pub fn edit<T>(
        &mut self,
        session: &mut Session,
        change: impl FnOnce(&mut Session) -> Result<T, project_model::Error>,
    ) -> Result<T, Error> {
        let value = change(session)?;
        if let Err(error) = self.sync(session.state()) {
            session.revert_last();
            return Err(error);
        }
        Ok(value)
    }

    /// Undoes the latest step and brings the graph in step. Returns `false` if there was nothing
    /// to undo.
    ///
    /// # Errors
    ///
    /// As [`Bridge::sync`]; an undone state was accepted before, so this does not happen.
    pub fn undo(&mut self, session: &mut Session) -> Result<bool, Error> {
        if !session.undo() {
            return Ok(false);
        }
        self.sync(session.state())?;
        Ok(true)
    }

    /// Redoes the latest undone step and brings the graph in step.
    ///
    /// # Errors
    ///
    /// As [`Bridge::undo`].
    pub fn redo(&mut self, session: &mut Session) -> Result<bool, Error> {
        if !session.redo() {
            return Ok(false);
        }
        self.sync(session.state())?;
        Ok(true)
    }

    /// Brings the graph in step with `state`, as after loading or restoring a version. If the
    /// graph refuses it, nothing changes.
    ///
    /// # Errors
    ///
    /// [`Error::Refused`]; [`Error::Graph`].
    pub fn sync(&mut self, state: &State) -> Result<(), Error> {
        let target = Target::of(state);
        if target.nodes == self.target_nodes()
            && target.connections == self.target_connections()
            && target.controls == self.target_controls()
            && target.tempo == self.mirror.tempo
        {
            return self.commit();
        }
        // Check it all in a scratch graph first, so the running graph changes only if it will
        // all succeed.
        let (mut scratch, _runner) = Graph::new(*self.graph.config()).map_err(Error::Graph)?;
        Mirror::build(&mut scratch, &target).map_err(refused)?;
        self.mirror.sync(&mut self.graph, &target).map_err(failed)?;
        self.pending = true;
        self.commit()
    }

    /// Sends the synced graph to the real-time side if a plan is waiting. A full plan queue is
    /// not an error: the plan stays waiting for the next call.
    ///
    /// # Errors
    ///
    /// [`Error::Graph`] for a failure other than a full queue.
    pub fn commit(&mut self) -> Result<(), Error> {
        if !self.pending {
            self.graph.collect();
            return Ok(());
        }
        match self.graph.commit() {
            Ok(()) => {
                self.pending = false;
                Ok(())
            }
            Err(signal_graph::Error::Busy) => Ok(()),
            Err(error) => Err(Error::Graph(error)),
        }
    }

    /// Whether a synced change has not yet reached the real-time side; call
    /// [`Bridge::commit`] again.
    pub fn is_pending(&self) -> bool {
        self.pending
    }

    /// The project output's sink.
    pub fn output(&self) -> Option<GraphNodeId> {
        self.mirror.nodes.get(&Key::Output).map(|n| n.id)
    }

    /// Each track's source.
    pub fn sources(&self) -> BTreeMap<TrackId, GraphNodeId> {
        sources(&self.mirror)
    }

    /// The running handle of parameter `name` on routing node `node`, for setting live while a
    /// control is dragged; the edit is recorded on release (ADR-0043).
    pub fn parameter(&self, node: NodeId, name: &str) -> Option<&Parameter> {
        let id = self.mirror.nodes.get(&Key::Node(node))?.id;
        self.graph
            .parameters(id)?
            .iter()
            .find(|p| p.info().name == name)
    }

    /// The latency of the last committed plan, in frames.
    pub fn latency(&self) -> usize {
        self.graph.latency()
    }

    /// The running graph, for inspection.
    pub fn graph(&self) -> &Graph {
        &self.graph
    }

    /// The graph's curve-to-parameter connection standing for model control wire `id`.
    pub fn control(&self, id: project_model::ConnectionId) -> Option<signal_graph::ConnectionId> {
        self.mirror.controls.get(&id).map(|c| c.id)
    }

    /// The graph connection standing for model connection `id`.
    pub fn connection(
        &self,
        id: project_model::ConnectionId,
    ) -> Option<signal_graph::ConnectionId> {
        self.mirror.connections.get(&id).map(|c| c.main)
    }

    /// The graph node standing for a track, routing node, or (with neither) the output; `None`
    /// for a curve or curve port (see [`Bridge::curve`]).
    pub fn node(&self, endpoint: project_model::Endpoint) -> Option<GraphNodeId> {
        self.mirror
            .nodes
            .get(&Key::of_audio(endpoint)?)
            .map(|n| n.id)
    }

    /// The graph node standing for automation curve `id`.
    pub fn curve(&self, id: project_model::CurveId) -> Option<GraphNodeId> {
        self.mirror.nodes.get(&Key::Curve(id)).map(|n| n.id)
    }

    fn target_nodes(&self) -> BTreeMap<Key, crate::target::NodeTarget> {
        self.mirror
            .nodes
            .iter()
            .map(|(key, node)| (*key, node.target.clone()))
            .collect()
    }

    fn target_controls(
        &self,
    ) -> BTreeMap<project_model::ConnectionId, crate::target::ControlTarget> {
        self.mirror
            .controls
            .iter()
            .map(|(id, c)| (*id, c.target.clone()))
            .collect()
    }

    fn target_connections(
        &self,
    ) -> BTreeMap<project_model::ConnectionId, crate::target::ConnectionTarget> {
        self.mirror
            .connections
            .iter()
            .map(|(id, c)| (*id, c.target))
            .collect()
    }
}

/// A new graph holding `state`'s routing, for offline render (export).
///
/// # Errors
///
/// [`Error::Refused`]; [`Error::Graph`].
pub fn render_graph(state: &State, config: Config) -> Result<RenderGraph, Error> {
    let (mut graph, runner, mirror) = build(state, config)?;
    graph.commit().map_err(Error::Graph)?;
    let output = mirror
        .nodes
        .get(&Key::Output)
        .map(|n| n.id)
        .ok_or(Error::Inconsistent)?;
    Ok(RenderGraph {
        graph,
        runner,
        output,
        sources: sources(&mirror),
    })
}
