//! Edits: reversible commands on the project state (`docs/architecture/project.md` "Undo
//! history").
//!
//! Applying an edit either changes the state and returns the edit that reverses it exactly, or
//! refuses with an error and leaves the state unchanged.

use std::collections::BTreeSet;

use crate::error::Error;
use crate::state::{Clip, State, Track};
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

/// Checks a whole state: unique track and clip ids, valid clips.
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
    Ok(())
}
