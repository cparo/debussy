//! An editing session: the current state, its undo history and the named versions
//! (`docs/architecture/project.md` "Undo history" and "Named versions").

use std::collections::{BTreeMap, BTreeSet};

use audio_buffers::Layout;

use crate::edit::Edit;
use crate::error::Error;
use crate::range::{self, Excerpt, Range};
use crate::routing::{Connection, ConnectionId, Endpoint, Node, NodeId, NodeKind, Position, Side};
use crate::state::{Clip, State, Track};
use crate::types::{ClipId, TrackId};

/// A project open for editing. Every change goes through [`Session::apply`] (or a method built
/// on it) and is one step of an unlimited undo history, which is never saved.
#[derive(Debug, Clone, Default)]
pub struct Session {
    state: State,
    /// Edits that undo the applied ones, latest last.
    undo: Vec<Edit>,
    /// Edits that redo the undone ones, latest last.
    redo: Vec<Edit>,
    versions: BTreeMap<String, State>,
    /// The next unused id, shared by tracks and clips.
    next_id: u64,
    /// Changes to the state or the named versions so far.
    revision: u64,
}

impl Session {
    /// An empty project.
    pub fn new() -> Self {
        Session::default()
    }

    /// A session over a loaded state and named versions, with an empty history.
    pub fn open(state: State, versions: BTreeMap<String, State>) -> Self {
        let ids = std::iter::once(&state)
            .chain(versions.values())
            .flat_map(|s| {
                let tracks = s.tracks.iter().flat_map(|track| {
                    std::iter::once(track.id.0).chain(track.clips.iter().map(|(id, _)| id.0))
                });
                let nodes = s.nodes.iter().map(|node| node.id.0);
                let connections = s.connections.iter().map(|c| c.id.0);
                tracks.chain(nodes).chain(connections).collect::<Vec<_>>()
            });
        let next_id = ids.max().map_or(0, |max| max.saturating_add(1));
        Session {
            state,
            undo: Vec::new(),
            redo: Vec::new(),
            versions,
            next_id,
            revision: 0,
        }
    }

    /// How many times the state or the named versions have changed since the session began: an
    /// applied edit, an undo, a redo, a revert, or a version saved or removed. A failed change
    /// leaves it alone; the interface saves the project when it moves.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Counts one change, and passes `changed` through.
    fn bump(&mut self, changed: bool) -> bool {
        if changed {
            self.revision += 1;
        }
        changed
    }

    /// The current state.
    pub fn state(&self) -> &State {
        &self.state
    }

    /// Applies one edit as one undoable step, and forgets anything undone.
    ///
    /// # Errors
    ///
    /// Any [`Error`] for an edit that does not fit; the session is then unchanged.
    pub fn apply(&mut self, edit: Edit) -> Result<(), Error> {
        let inverse = edit.apply(&mut self.state)?;
        self.undo.push(inverse);
        self.redo.clear();
        self.bump(true);
        Ok(())
    }

    /// Reverts the latest step and forgets it, adding nothing to the redo history; for a step
    /// something downstream refused, such as a routing edit the running graph cannot take. The
    /// redo history that step cleared is not restored. Returns `false` if there is nothing to
    /// revert.
    pub fn revert_last(&mut self) -> bool {
        let mut discarded = Vec::new();
        let changed = step(&mut self.state, &mut self.undo, &mut discarded);
        self.bump(changed)
    }

    /// Undoes the latest step. Returns `false` if there is nothing to undo.
    pub fn undo(&mut self) -> bool {
        let changed = step(&mut self.state, &mut self.undo, &mut self.redo);
        self.bump(changed)
    }

    /// Redoes the latest undone step. Returns `false` if there is nothing to redo.
    pub fn redo(&mut self) -> bool {
        let changed = step(&mut self.state, &mut self.redo, &mut self.undo);
        self.bump(changed)
    }

    /// Whether there is a step to undo.
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    /// Whether there is a step to redo.
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// A new track id, for building edits.
    pub fn new_track_id(&mut self) -> TrackId {
        TrackId(self.take_id())
    }

    /// A new clip id, for building edits.
    pub fn new_clip_id(&mut self) -> ClipId {
        ClipId(self.take_id())
    }

    /// A new project: no tracks, and one mixer feeding the project output, which new tracks
    /// join (ADR-0058). The history starts empty.
    pub fn new_project() -> Self {
        let mut session = Session::new();
        // Neither can fail on an empty state.
        if let Ok(mixer) = session.add_node(NodeKind::Mixer {
            channels: 1,
            parameters: BTreeMap::new(),
        }) {
            let _ = session.connect(Endpoint::Node(mixer), 0, Endpoint::Output, 0);
        }
        Session::open(session.state, BTreeMap::new())
    }

    /// Adds an empty track of `layout` after the others, as one step. With a project mixer
    /// ([`State::project_mixer`]) it joins it: a mono track on the first free channel, a
    /// stereo track's left and right on two channels panned hard left and right, adding
    /// channels as needed (ADR-0058). Without one it connects straight to the project output.
    ///
    /// # Errors
    ///
    /// None in practice; see [`Session::apply`].
    pub fn add_track(&mut self, name: &str, layout: Layout) -> Result<TrackId, Error> {
        let (id, edits) = self.new_track_edits(name, layout);
        self.apply(Edit::Batch(edits))?;
        Ok(id)
    }

    /// Adds a track as [`Session::add_track`] does, holding `clip`, as one step: an import.
    ///
    /// # Errors
    ///
    /// As [`Session::add_clip`] for a clip that does not fit the track.
    pub fn add_track_with_clip(
        &mut self,
        name: &str,
        layout: Layout,
        clip: Clip,
    ) -> Result<(TrackId, ClipId), Error> {
        let (id, mut edits) = self.new_track_edits(name, layout);
        let clip_id = self.new_clip_id();
        edits.push(Edit::InsertClip {
            track: id,
            layer: 0,
            id: clip_id,
            clip,
        });
        self.apply(Edit::Batch(edits))?;
        Ok((id, clip_id))
    }

    /// The edits that add a new track and route it, and its id.
    fn new_track_edits(&mut self, name: &str, layout: Layout) -> (TrackId, Vec<Edit>) {
        let id = self.new_track_id();
        let index = self.state.tracks.len();
        let track = Track::new(id, name, layout);
        let mut edits = vec![Edit::InsertTrack { index, track }];
        // New connections go at the end of the list, one after another.
        let mut next_index = self.state.connections.len();
        match self.state.project_mixer() {
            Some(mixer) => {
                let sides: &[(Option<Side>, f32)] = match layout {
                    Layout::Mono => &[(None, 0.0)],
                    Layout::Stereo => &[(Some(Side::Left), -1.0), (Some(Side::Right), 1.0)],
                };
                let mut free = self.state.free_mixer_channels(mixer);
                let channels = match self.state.node(mixer).map(|n| &n.kind) {
                    Some(NodeKind::Mixer { channels, .. }) => *channels,
                    _ => 0,
                };
                let missing = sides.len().saturating_sub(free.len());
                if missing > 0 {
                    edits.push(Edit::SetMixerChannels {
                        node: mixer,
                        channels: channels + missing,
                    });
                    free.extend(channels..channels + missing);
                }
                for (&(side, pan), &channel) in sides.iter().zip(&free) {
                    let connection = Connection::new(
                        self.new_connection_id(),
                        Endpoint::Track(id),
                        0,
                        Endpoint::Node(mixer),
                        channel,
                    )
                    .with_sides(side, None);
                    edits.push(Edit::Connect {
                        index: next_index,
                        connection,
                    });
                    next_index += 1;
                    // A free channel may hold an old value; set the pan this track wants and
                    // clear the rest.
                    for control in ["gain", "mode"] {
                        edits.push(Edit::SetParameter {
                            node: mixer,
                            name: format!("channel {} {control}", channel + 1),
                            value: None,
                        });
                    }
                    edits.push(Edit::SetParameter {
                        node: mixer,
                        name: format!("channel {} pan", channel + 1),
                        value: (pan != 0.0).then_some(pan),
                    });
                }
            }
            None => {
                let connection = Connection::new(
                    self.new_connection_id(),
                    Endpoint::Track(id),
                    0,
                    Endpoint::Output,
                    0,
                );
                edits.push(Edit::Connect {
                    index: next_index,
                    connection,
                });
            }
        }
        (id, edits)
    }

    /// Connects `from_side` of output `output` of `from` (or the whole port, with `None`) to
    /// `to_side` of input `input` of `to`, panned centre.
    ///
    /// # Errors
    ///
    /// As [`Session::connect`], and [`Error::SideOfMonoPort`].
    pub fn connect_sides(
        &mut self,
        from: Endpoint,
        output: usize,
        from_side: Option<Side>,
        to: Endpoint,
        input: usize,
        to_side: Option<Side>,
    ) -> Result<ConnectionId, Error> {
        let id = self.new_connection_id();
        self.apply(Edit::Connect {
            index: self.state.connections.len(),
            connection: Connection::new(id, from, output, to, input).with_sides(from_side, to_side),
        })?;
        Ok(id)
    }

    /// Removes a track, its clips and its connections, as one step.
    ///
    /// # Errors
    ///
    /// [`Error::UnknownTrack`].
    pub fn remove_track(&mut self, id: TrackId) -> Result<(), Error> {
        self.state.track(id).ok_or(Error::UnknownTrack(id))?;
        let mut edits = self.disconnect_all(Endpoint::Track(id));
        edits.push(Edit::RemoveTrack(id));
        self.apply(Edit::Batch(edits))
    }

    /// Adds a routing node.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidValue`] for a non-finite parameter value or a bad delay time.
    pub fn add_node(&mut self, kind: NodeKind) -> Result<NodeId, Error> {
        let id = NodeId(self.take_id());
        let index = self.state.nodes.len();
        self.apply(Edit::InsertNode {
            index,
            node: Node {
                id,
                kind,
                position: None,
            },
        })?;
        Ok(id)
    }

    /// Moves a node on the patch board, as one step; `None` returns it to the automatic layout.
    ///
    /// # Errors
    ///
    /// [`Error::UnknownNode`]; [`Error::InvalidValue`] for a coordinate that is not finite or is
    /// beyond [`MAX_BOARD_COORDINATE`](crate::MAX_BOARD_COORDINATE).
    pub fn move_node(&mut self, id: NodeId, position: Option<Position>) -> Result<(), Error> {
        self.apply(Edit::MoveNode { node: id, position })
    }

    /// Moves the project output's L and R on the patch board, as one step; `None` returns them
    /// to the automatic layout.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidValue`] as for [`Session::move_node`].
    pub fn move_output(&mut self, y: Option<f32>) -> Result<(), Error> {
        self.apply(Edit::MoveOutput(y))
    }

    /// Removes a connection, as one step.
    ///
    /// # Errors
    ///
    /// [`Error::UnknownConnection`].
    pub fn disconnect(&mut self, id: ConnectionId) -> Result<(), Error> {
        self.apply(Edit::Disconnect(id))
    }

    /// Removes a routing node and its connections, as one step.
    ///
    /// # Errors
    ///
    /// [`Error::UnknownNode`].
    pub fn remove_node(&mut self, id: NodeId) -> Result<(), Error> {
        self.state.node(id).ok_or(Error::UnknownNode(id))?;
        let mut edits = self.disconnect_all(Endpoint::Node(id));
        edits.push(Edit::RemoveNode(id));
        self.apply(Edit::Batch(edits))
    }

    /// Connects output `output` of `from` to input `input` of `to`, panned centre.
    ///
    /// # Errors
    ///
    /// [`Error::UnknownTrack`], [`Error::UnknownNode`], [`Error::NoSuchPort`], or
    /// [`Error::AlreadyConnected`].
    pub fn connect(
        &mut self,
        from: Endpoint,
        output: usize,
        to: Endpoint,
        input: usize,
    ) -> Result<ConnectionId, Error> {
        let id = self.new_connection_id();
        self.apply(Edit::Connect {
            index: self.state.connections.len(),
            connection: Connection::new(id, from, output, to, input),
        })?;
        Ok(id)
    }

    /// Records a parameter's value at the end of a gesture; `None` returns it to its default.
    ///
    /// # Errors
    ///
    /// [`Error::UnknownNode`], [`Error::WrongNodeKind`], or [`Error::InvalidValue`].
    pub fn set_parameter(
        &mut self,
        node: NodeId,
        name: &str,
        value: Option<f32>,
    ) -> Result<(), Error> {
        self.apply(Edit::SetParameter {
            node,
            name: name.to_owned(),
            value,
        })
    }

    /// Adds `count` channels to the end of a mixer, as one undo step. Existing channels keep their
    /// connections and values.
    ///
    /// # Errors
    ///
    /// [`Error::UnknownNode`], [`Error::WrongNodeKind`], or [`Error::InvalidValue`] if `count`
    /// is 0 or the total would overflow.
    pub fn add_mixer_channels(&mut self, node: NodeId, count: usize) -> Result<(), Error> {
        let current = match self.state.node(node).map(|n| &n.kind) {
            Some(NodeKind::Mixer { channels, .. }) => *channels,
            Some(_) => return Err(Error::WrongNodeKind(node)),
            None => return Err(Error::UnknownNode(node)),
        };
        let channels = match count {
            0 => None,
            _ => current.checked_add(count),
        }
        .ok_or(Error::InvalidValue)?;
        self.apply(Edit::SetMixerChannels { node, channels })
    }

    /// Edits that remove every connection of `endpoint`.
    fn disconnect_all(&self, endpoint: Endpoint) -> Vec<Edit> {
        self.state
            .connections_of(endpoint)
            .map(|c| Edit::Disconnect(c.id))
            .collect()
    }

    fn new_connection_id(&mut self) -> ConnectionId {
        ConnectionId(self.take_id())
    }

    /// Adds a clip to a track, on top of any it overlaps.
    ///
    /// # Errors
    ///
    /// [`Error::UnknownTrack`], or the clip's [`Clip::check`] error.
    pub fn add_clip(&mut self, track: TrackId, clip: Clip) -> Result<ClipId, Error> {
        let layer = self.top_layer(track, None)?;
        let id = self.new_clip_id();
        self.apply(Edit::InsertClip {
            track,
            layer,
            id,
            clip,
        })?;
        Ok(id)
    }

    /// Moves a clip to `position` on `track` (its own or another of the same layout), on top of
    /// any it overlaps there.
    ///
    /// # Errors
    ///
    /// [`Error::UnknownClip`], [`Error::UnknownTrack`], or the moved clip's [`Clip::check`] error.
    pub fn move_clip(&mut self, id: ClipId, track: TrackId, position: u64) -> Result<(), Error> {
        let clip = self.state.clip(id).ok_or(Error::UnknownClip(id))?;
        let clip = Clip {
            position,
            ..clip.clone()
        };
        let layer = self.top_layer(track, Some(id))?;
        self.apply(Edit::Batch(vec![
            Edit::RemoveClip(id),
            Edit::InsertClip {
                track,
                layer,
                id,
                clip,
            },
        ]))
    }

    /// Changes a clip's properties: trim, gain, fades. A change of position also brings it to the
    /// top, as a move does.
    ///
    /// # Errors
    ///
    /// [`Error::UnknownClip`], or the new clip's [`Clip::check`] error.
    pub fn set_clip(&mut self, id: ClipId, clip: Clip) -> Result<(), Error> {
        let (track, _) = self.state.find_clip(id).ok_or(Error::UnknownClip(id))?;
        let moved = self
            .state
            .clip(id)
            .is_some_and(|old| old.position != clip.position);
        if !moved {
            return self.apply(Edit::ReplaceClip { id, clip });
        }
        let layer = self.top_layer(track, Some(id))?;
        self.apply(Edit::Batch(vec![
            Edit::RemoveClip(id),
            Edit::InsertClip {
                track,
                layer,
                id,
                clip,
            },
        ]))
    }

    /// The parts of the clips on `tracks` inside `range`, for pasting later. Not an edit.
    ///
    /// # Errors
    ///
    /// [`Error::UnknownTrack`].
    pub fn copy_range(&self, tracks: &[TrackId], range: Range) -> Result<Excerpt, Error> {
        range::excerpt(&self.state, tracks, range)
    }

    /// Removes everything inside `range` on `tracks`, splitting clips at its edges and leaving a
    /// gap, as one step; no step if nothing lies there.
    ///
    /// # Errors
    ///
    /// [`Error::UnknownTrack`].
    pub fn delete_range(&mut self, tracks: &[TrackId], range: Range) -> Result<(), Error> {
        let mut next = self.next_id;
        let edits = range::delete_edits(&self.state, tracks, range, || {
            let id = ClipId(next);
            next += 1;
            id
        })?;
        self.apply_new_ids(edits, next)
    }

    /// Copies `range` on `tracks`, then deletes it as [`Session::delete_range`] does.
    ///
    /// # Errors
    ///
    /// [`Error::UnknownTrack`].
    pub fn cut_range(&mut self, tracks: &[TrackId], range: Range) -> Result<Excerpt, Error> {
        let excerpt = self.copy_range(tracks, range)?;
        self.delete_range(tracks, range)?;
        Ok(excerpt)
    }

    /// Pastes `excerpt` with its start at `position`: its lanes go in order onto `track` and the
    /// tracks below it, each clip on top with a new id, as one step; no step if it holds no
    /// clips. Returns the new clips' ids.
    ///
    /// # Errors
    ///
    /// [`Error::UnknownTrack`], [`Error::NotEnoughTracks`], [`Error::LayoutMismatch`], or
    /// [`Error::ClipOutsideTimeline`]; nothing is pasted then.
    pub fn paste(
        &mut self,
        excerpt: &Excerpt,
        track: TrackId,
        position: u64,
    ) -> Result<Vec<ClipId>, Error> {
        let mut next = self.next_id;
        let (edits, ids) = range::paste_edits(&self.state, excerpt, track, position, || {
            let id = ClipId(next);
            next += 1;
            id
        })?;
        self.apply_new_ids(edits, next)?;
        Ok(ids)
    }

    /// Applies `edits` as one step, if there are any, and on success takes the ids below
    /// `next_id` they used.
    fn apply_new_ids(&mut self, edits: Vec<Edit>, next_id: u64) -> Result<(), Error> {
        if edits.is_empty() {
            return Ok(());
        }
        self.apply(Edit::Batch(edits))?;
        self.next_id = next_id;
        Ok(())
    }

    /// Saves the current state as a named version. Not an edit: it changes no state.
    ///
    /// # Errors
    ///
    /// [`Error::VersionExists`].
    pub fn save_version(&mut self, name: &str) -> Result<(), Error> {
        if self.versions.contains_key(name) {
            return Err(Error::VersionExists(name.to_owned()));
        }
        self.versions.insert(name.to_owned(), self.state.clone());
        self.bump(true);
        Ok(())
    }

    /// Deletes a named version.
    ///
    /// # Errors
    ///
    /// [`Error::UnknownVersion`].
    pub fn remove_version(&mut self, name: &str) -> Result<(), Error> {
        self.versions
            .remove(name)
            .ok_or_else(|| Error::UnknownVersion(name.to_owned()))?;
        self.bump(true);
        Ok(())
    }

    /// The named versions, by name.
    pub fn versions(&self) -> &BTreeMap<String, State> {
        &self.versions
    }

    /// Makes a named version the current state, as one undoable step.
    ///
    /// # Errors
    ///
    /// [`Error::UnknownVersion`].
    pub fn restore_version(&mut self, name: &str) -> Result<(), Error> {
        let state = self
            .versions
            .get(name)
            .cloned()
            .ok_or_else(|| Error::UnknownVersion(name.to_owned()))?;
        self.apply(Edit::Restore(state))
    }

    /// The names of every audio file the current state or a named version uses; cleanup must
    /// keep these (`docs/architecture/project.md` "Editing model").
    pub fn referenced_files(&self) -> BTreeSet<&str> {
        std::iter::once(&self.state)
            .chain(self.versions.values())
            .flat_map(|state| &state.tracks)
            .flat_map(|track| &track.clips)
            .flat_map(|(_, clip)| clip.source.files())
            .map(|file| file.name.as_str())
            .collect()
    }

    /// The layer a clip arriving on `track` goes to: the top, once `leaving` has left it.
    fn top_layer(&self, track: TrackId, leaving: Option<ClipId>) -> Result<usize, Error> {
        let target = self.state.track(track).ok_or(Error::UnknownTrack(track))?;
        let leaves = leaving.is_some_and(|id| target.clip(id).is_some());
        Ok(target.clips.len() - usize::from(leaves))
    }

    fn take_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }
}

/// Applies the latest edit of `from`, pushing its inverse onto `to`.
fn step(state: &mut State, from: &mut Vec<Edit>, to: &mut Vec<Edit>) -> bool {
    let Some(edit) = from.pop() else {
        return false;
    };
    match edit.clone().apply(state) {
        Ok(inverse) => {
            to.push(inverse);
            true
        }
        // Cannot happen: a history edit always fits the state it reverses. Keep it rather than
        // lose it.
        Err(_) => {
            from.push(edit);
            false
        }
    }
}
