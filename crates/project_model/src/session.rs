//! An editing session: the current state, its undo history and the named versions
//! (`docs/architecture/project.md` "Undo history" and "Named versions").

use std::collections::{BTreeMap, BTreeSet};

use audio_buffers::Layout;

use crate::edit::Edit;
use crate::error::Error;
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
            .flat_map(|s| &s.tracks)
            .flat_map(|track| {
                std::iter::once(track.id.0).chain(track.clips.iter().map(|(id, _)| id.0))
            });
        let next_id = ids.max().map_or(0, |max| max.saturating_add(1));
        Session {
            state,
            undo: Vec::new(),
            redo: Vec::new(),
            versions,
            next_id,
        }
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
        Ok(())
    }

    /// Undoes the latest step. Returns `false` if there is nothing to undo.
    pub fn undo(&mut self) -> bool {
        step(&mut self.state, &mut self.undo, &mut self.redo)
    }

    /// Redoes the latest undone step. Returns `false` if there is nothing to redo.
    pub fn redo(&mut self) -> bool {
        step(&mut self.state, &mut self.redo, &mut self.undo)
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

    /// Adds an empty track of `layout` after the others.
    ///
    /// # Errors
    ///
    /// None in practice; see [`Session::apply`].
    pub fn add_track(&mut self, name: &str, layout: Layout) -> Result<TrackId, Error> {
        let id = self.new_track_id();
        let index = self.state.tracks.len();
        let track = Track::new(id, name, layout);
        self.apply(Edit::InsertTrack { index, track })?;
        Ok(id)
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
            .map(|_| ())
            .ok_or_else(|| Error::UnknownVersion(name.to_owned()))
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
