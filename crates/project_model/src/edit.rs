//! Edits: reversible commands on the project state (`docs/architecture/project.md` "Undo
//! history").
//!
//! Applying an edit either changes the state and returns the edit that reverses it exactly, or
//! refuses with an error and leaves the state unchanged.

use std::collections::BTreeSet;

use audio_buffers::Layout;

use crate::error::Error;
use crate::routing::{self, Connection, ConnectionId, Endpoint, Node, NodeId, NodeKind};
use crate::state::{Clip, State, Track};
use crate::tempo::Tempo;
use crate::types::{ClipId, TrackId};

/// One reversible change to the project state. A gesture made of several changes is a
/// [`Edit::Batch`], undone as one.
#[derive(Debug, Clone, PartialEq)]
pub enum Edit {
    /// Inserts a whole track, with its clips, at `index` in the track order.
    InsertTrack {
        /// Where it goes; at most the number of tracks.
        index: usize,
        /// The track.
        track: Track,
    },
    /// Removes a track and its clips.
    RemoveTrack(TrackId),
    /// Renames a track.
    RenameTrack {
        /// The track.
        track: TrackId,
        /// Its new name.
        name: String,
    },
    /// Moves a track to `index` in the track order.
    MoveTrack {
        /// The track.
        track: TrackId,
        /// Where it ends up; less than the number of tracks.
        index: usize,
    },
    /// Inserts a clip on a track at `layer`, 0 being the bottom.
    InsertClip {
        /// The track.
        track: TrackId,
        /// The layer; at most the number of clips on the track.
        layer: usize,
        /// The clip's id, unused anywhere in the state.
        id: ClipId,
        /// The clip.
        clip: Clip,
    },
    /// Removes a clip.
    RemoveClip(ClipId),
    /// Changes a clip in place, keeping its track and layer.
    ReplaceClip {
        /// The clip.
        id: ClipId,
        /// Its new properties.
        clip: Clip,
    },
    /// Inserts a routing node at `index` in the node list.
    InsertNode {
        /// Where it goes; at most the number of nodes.
        index: usize,
        /// The node.
        node: Node,
    },
    /// Removes a routing node, which must have no connections.
    RemoveNode(NodeId),
    /// Sets an effect or mixer parameter, or with `None` returns it to its default.
    SetParameter {
        /// The effect or mixer node.
        node: NodeId,
        /// The parameter's name.
        name: String,
        /// The value.
        value: Option<f32>,
    },
    /// Sets a mixer's number of channels. Channels are added at the end; fewer channels, as when
    /// undoing an addition, are refused while a removed channel is connected. Parameter values
    /// are kept as they are.
    SetMixerChannels {
        /// The mixer node.
        node: NodeId,
        /// The number of channels, at least 1.
        channels: usize,
    },
    /// Sets a feedback delay's time.
    SetDelay {
        /// The feedback-delay node.
        node: NodeId,
        /// The time, in seconds.
        seconds: f64,
    },
    /// Adds a connection at `index` in the connection list.
    Connect {
        /// Where it goes; at most the number of connections.
        index: usize,
        /// The connection.
        connection: Connection,
    },
    /// Removes a connection.
    Disconnect(ConnectionId),
    /// Sets where a mono signal entering a stereo input through a connection is panned.
    SetPan {
        /// The connection.
        connection: ConnectionId,
        /// The pan, −1 to 1.
        pan: f32,
    },
    /// Moves a node on the patch board; `None` returns it to the automatic layout.
    MoveNode {
        /// The node.
        node: NodeId,
        /// Its new position.
        position: Option<crate::routing::Position>,
    },
    /// Moves the project output's L and R up or down the patch board; `None` returns them to
    /// the automatic layout.
    MoveOutput(Option<f32>),
    /// Sets the project's tempo and meter.
    SetTempo(Tempo),
    /// Replaces the whole state, as when restoring a named version.
    Restore(State),
    /// Several edits applied in order, as one.
    Batch(Vec<Edit>),
}

impl Edit {
    /// Applies the edit to `state`, returning the edit that reverses it.
    ///
    /// # Errors
    ///
    /// Any [`Error`] for an edit that does not fit the state, which is then unchanged.
    pub fn apply(self, state: &mut State) -> Result<Edit, Error> {
        match self {
            Edit::InsertTrack { index, track } => {
                check_index(index, state.tracks.len() + 1)?;
                if state.track(track.id).is_some() {
                    return Err(Error::TrackExists(track.id));
                }
                check_track(&track, &clip_ids(state))?;
                let id = track.id;
                state.tracks.insert(index, track);
                Ok(Edit::RemoveTrack(id))
            }
            Edit::RemoveTrack(id) => {
                let index = state.track_index(id)?;
                check_unconnected(state, Endpoint::Track(id))?;
                let track = state.tracks.remove(index);
                Ok(Edit::InsertTrack { index, track })
            }
            Edit::RenameTrack { track, name } => {
                let old = std::mem::replace(&mut state.track_mut(track)?.name, name);
                Ok(Edit::RenameTrack { track, name: old })
            }
            Edit::MoveTrack { track, index } => {
                let from = state.track_index(track)?;
                check_index(index, state.tracks.len())?;
                let moved = state.tracks.remove(from);
                state.tracks.insert(index, moved);
                Ok(Edit::MoveTrack { track, index: from })
            }
            Edit::InsertClip {
                track,
                layer,
                id,
                clip,
            } => {
                if state.clip(id).is_some() {
                    return Err(Error::ClipExists(id));
                }
                let target = state.track_mut(track)?;
                check_index(layer, target.clips.len() + 1)?;
                clip.check(target.layout)?;
                target.clips.insert(layer, (id, clip));
                Ok(Edit::RemoveClip(id))
            }
            Edit::RemoveClip(id) => {
                let (track, layer) = state.find_clip(id).ok_or(Error::UnknownClip(id))?;
                let (_, clip) = state.track_mut(track)?.clips.remove(layer);
                Ok(Edit::InsertClip {
                    track,
                    layer,
                    id,
                    clip,
                })
            }
            Edit::ReplaceClip { id, clip } => {
                let (track, layer) = state.find_clip(id).ok_or(Error::UnknownClip(id))?;
                let target = state.track_mut(track)?;
                clip.check(target.layout)?;
                let old = std::mem::replace(&mut target.clips[layer].1, clip);
                Ok(Edit::ReplaceClip { id, clip: old })
            }
            Edit::InsertNode { index, node } => {
                check_index(index, state.nodes.len() + 1)?;
                if state.node(node.id).is_some() {
                    return Err(Error::NodeExists(node.id));
                }
                routing::check_node(&node)?;
                let id = node.id;
                state.nodes.insert(index, node);
                Ok(Edit::RemoveNode(id))
            }
            Edit::RemoveNode(id) => {
                let index = node_index(state, id)?;
                check_unconnected(state, Endpoint::Node(id))?;
                let node = state.nodes.remove(index);
                Ok(Edit::InsertNode { index, node })
            }
            Edit::SetParameter { node, name, value } => {
                if value.is_some_and(|v| !v.is_finite()) {
                    return Err(Error::InvalidValue);
                }
                let index = node_index(state, node)?;
                let (NodeKind::Effect { parameters, .. } | NodeKind::Mixer { parameters, .. }) =
                    &mut state.nodes[index].kind
                else {
                    return Err(Error::WrongNodeKind(node));
                };
                let old = match value {
                    Some(value) => parameters.insert(name.clone(), value),
                    None => parameters.remove(&name),
                };
                Ok(Edit::SetParameter {
                    node,
                    name,
                    value: old,
                })
            }
            Edit::SetMixerChannels { node, channels } => {
                routing::check_channels(channels)?;
                let endpoint = Endpoint::Node(node);
                let removed = |c: &Connection| c.to == endpoint && c.input >= channels;
                if state.connections.iter().any(removed) {
                    return Err(Error::StillConnected(endpoint));
                }
                let index = node_index(state, node)?;
                let NodeKind::Mixer {
                    channels: current, ..
                } = &mut state.nodes[index].kind
                else {
                    return Err(Error::WrongNodeKind(node));
                };
                let old = std::mem::replace(current, channels);
                Ok(Edit::SetMixerChannels {
                    node,
                    channels: old,
                })
            }
            Edit::SetDelay { node, seconds } => {
                routing::check_delay(seconds)?;
                let index = node_index(state, node)?;
                let NodeKind::FeedbackDelay {
                    seconds: current, ..
                } = &mut state.nodes[index].kind
                else {
                    return Err(Error::WrongNodeKind(node));
                };
                let old = std::mem::replace(current, seconds);
                Ok(Edit::SetDelay { node, seconds: old })
            }
            Edit::Connect { index, connection } => {
                check_index(index, state.connections.len() + 1)?;
                check_connection(state, &connection)?;
                let id = connection.id;
                state.connections.insert(index, connection);
                Ok(Edit::Disconnect(id))
            }
            Edit::Disconnect(id) => {
                let index = connection_index(state, id)?;
                let connection = state.connections.remove(index);
                Ok(Edit::Connect { index, connection })
            }
            Edit::SetPan { connection, pan } => {
                routing::check_pan(pan)?;
                let index = connection_index(state, connection)?;
                let old = std::mem::replace(&mut state.connections[index].pan, pan);
                Ok(Edit::SetPan {
                    connection,
                    pan: old,
                })
            }
            Edit::MoveNode { node, position } => {
                if let Some(p) = position {
                    routing::check_coordinate(p.x)?;
                    routing::check_coordinate(p.y)?;
                }
                let index = node_index(state, node)?;
                let old = std::mem::replace(&mut state.nodes[index].position, position);
                Ok(Edit::MoveNode {
                    node,
                    position: old,
                })
            }
            Edit::MoveOutput(y) => {
                if let Some(y) = y {
                    routing::check_coordinate(y)?;
                }
                Ok(Edit::MoveOutput(std::mem::replace(&mut state.output_y, y)))
            }
            Edit::SetTempo(tempo) => {
                tempo.check()?;
                Ok(Edit::SetTempo(std::mem::replace(&mut state.tempo, tempo)))
            }
            Edit::Restore(new) => {
                check_state(&new)?;
                Ok(Edit::Restore(std::mem::replace(state, new)))
            }
            Edit::Batch(edits) => {
                let mut inverses = Vec::with_capacity(edits.len());
                for edit in edits {
                    match edit.apply(state) {
                        Ok(inverse) => inverses.push(inverse),
                        Err(error) => {
                            roll_back(state, inverses);
                            return Err(error);
                        }
                    }
                }
                inverses.reverse();
                Ok(Edit::Batch(inverses))
            }
        }
    }
}

/// Undoes already-applied edits of a refused batch, latest first.
fn roll_back(state: &mut State, inverses: Vec<Edit>) {
    for inverse in inverses.into_iter().rev() {
        // An inverse always fits the state its edit produced.
        let _ = inverse.apply(state);
    }
}

/// Checks `index` is below `limit`.
fn check_index(index: usize, limit: usize) -> Result<(), Error> {
    if index < limit {
        Ok(())
    } else {
        Err(Error::IndexOutOfRange { index, limit })
    }
}

/// Every clip id in the state.
fn clip_ids(state: &State) -> BTreeSet<ClipId> {
    state
        .tracks
        .iter()
        .flat_map(|track| track.clips.iter().map(|(id, _)| *id))
        .collect()
}

/// Checks a track's clips are valid and their ids are not in `taken` or repeated.
fn check_track(track: &Track, taken: &BTreeSet<ClipId>) -> Result<(), Error> {
    let mut seen = BTreeSet::new();
    for (id, clip) in &track.clips {
        if taken.contains(id) || !seen.insert(*id) {
            return Err(Error::ClipExists(*id));
        }
        clip.check(track.layout)?;
    }
    Ok(())
}

fn node_index(state: &State, id: NodeId) -> Result<usize, Error> {
    state
        .nodes
        .iter()
        .position(|node| node.id == id)
        .ok_or(Error::UnknownNode(id))
}

fn connection_index(state: &State, id: ConnectionId) -> Result<usize, Error> {
    state
        .connections
        .iter()
        .position(|c| c.id == id)
        .ok_or(Error::UnknownConnection(id))
}

fn check_unconnected(state: &State, endpoint: Endpoint) -> Result<(), Error> {
    match state.connections_of(endpoint).next() {
        Some(_) => Err(Error::StillConnected(endpoint)),
        None => Ok(()),
    }
}

/// Whether `endpoint` can have an output (`output` true) or input port `port`. Effects' ports are
/// not known to the model and are checked by `signal_graph`; a mixer has an input per channel and
/// one output.
fn has_port(state: &State, endpoint: Endpoint, output: bool, port: usize) -> bool {
    match endpoint {
        Endpoint::Track(_) => output && port == 0,
        Endpoint::Output => !output && port == 0,
        Endpoint::Node(id) => match state.node(id).map(|node| &node.kind) {
            Some(NodeKind::Effect { .. }) => true,
            Some(NodeKind::Mixer { channels, .. }) => match output {
                true => port == 0,
                false => port < *channels,
            },
            Some(NodeKind::Bus(_) | NodeKind::FeedbackDelay { .. }) => port == 0,
            None => false,
        },
    }
}

/// The layout of a port, where the model knows it; effects' ports are `signal_graph`'s.
fn port_layout(state: &State, endpoint: Endpoint, output: bool) -> Option<Layout> {
    match endpoint {
        Endpoint::Track(id) => state.track(id).map(|t| t.layout),
        Endpoint::Output => Some(Layout::Stereo),
        Endpoint::Node(id) => match state.node(id).map(|node| &node.kind) {
            Some(NodeKind::Mixer { .. }) => {
                Some(if output { Layout::Stereo } else { Layout::Mono })
            }
            Some(NodeKind::Bus(layout) | NodeKind::FeedbackDelay { layout, .. }) => Some(*layout),
            Some(NodeKind::Effect { .. }) | None => None,
        },
    }
}

/// Checks a new connection against the state: a fresh id, existing endpoints with such ports,
/// sides only of ports that may be stereo, and no duplicate.
fn check_connection(state: &State, connection: &Connection) -> Result<(), Error> {
    if state.connection(connection.id).is_some() {
        return Err(Error::ConnectionExists(connection.id));
    }
    routing::check_pan(connection.pan)?;
    let ends = [
        (connection.from, true, connection.output),
        (connection.to, false, connection.input),
    ];
    for (endpoint, output, port) in ends {
        match endpoint {
            Endpoint::Track(id) if state.track(id).is_none() => {
                return Err(Error::UnknownTrack(id));
            }
            Endpoint::Node(id) if state.node(id).is_none() => return Err(Error::UnknownNode(id)),
            _ if !has_port(state, endpoint, output, port) => {
                return Err(Error::NoSuchPort(endpoint));
            }
            _ => {}
        }
    }
    let sides = [
        (connection.from, true, connection.from_side),
        (connection.to, false, connection.to_side),
    ];
    for (endpoint, output, side) in sides {
        if side.is_some() && port_layout(state, endpoint, output) == Some(Layout::Mono) {
            return Err(Error::SideOfMonoPort(endpoint));
        }
    }
    let same = |c: &Connection| {
        (c.from, c.output, c.from_side, c.to, c.input, c.to_side)
            == (
                connection.from,
                connection.output,
                connection.from_side,
                connection.to,
                connection.input,
                connection.to_side,
            )
    };
    if state.connections.iter().any(same) {
        return Err(Error::AlreadyConnected);
    }
    Ok(())
}

/// Checks a whole state: unique ids, valid clips and nodes, connections that fit.
fn check_state(state: &State) -> Result<(), Error> {
    let mut tracks = BTreeSet::new();
    let mut clips = BTreeSet::new();
    for track in &state.tracks {
        if !tracks.insert(track.id) {
            return Err(Error::TrackExists(track.id));
        }
        check_track(track, &clips)?;
        clips.extend(track.clips.iter().map(|(id, _)| *id));
    }
    let mut nodes = BTreeSet::new();
    for node in &state.nodes {
        if !nodes.insert(node.id) {
            return Err(Error::NodeExists(node.id));
        }
        routing::check_node(node)?;
    }
    let mut built = State {
        tracks: state.tracks.clone(),
        nodes: state.nodes.clone(),
        connections: Vec::with_capacity(state.connections.len()),
        output_y: state.output_y,
        tempo: state.tempo,
    };
    state.tempo.check()?;
    if let Some(y) = state.output_y {
        routing::check_coordinate(y)?;
    }
    for connection in &state.connections {
        check_connection(&built, connection)?;
        built.connections.push(*connection);
    }
    Ok(())
}
