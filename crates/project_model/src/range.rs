//! Range edits: cutting, copying and pasting a span of frames across tracks
//! (`docs/architecture/project.md` "Range edits").
//!
//! This module builds the edits; [`Session`](crate::Session) applies them as one step and supplies
//! new clip ids.

use std::collections::BTreeSet;

use audio_buffers::Layout;

use crate::edit::Edit;
use crate::error::Error;
use crate::state::{Clip, State};
use crate::types::{ClipId, TrackId};

/// A half-open span of timeline frames, `start..end`, never empty.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Range {
    start: u64,
    end: u64,
}

impl Range {
    /// The frames from `start` up to, not including, `end`.
    ///
    /// # Errors
    ///
    /// [`Error::EmptyRange`] if `start` is not below `end`.
    pub fn new(start: u64, end: u64) -> Result<Range, Error> {
        if start >= end {
            return Err(Error::EmptyRange);
        }
        Ok(Range { start, end })
    }

    /// The first frame in the range.
    pub fn start(&self) -> u64 {
        self.start
    }

    /// The frame just after the range.
    pub fn end(&self) -> u64 {
        self.end
    }

    /// How many frames the range spans.
    pub fn length(&self) -> u64 {
        self.end - self.start
    }
}

/// What a copy or cut captured: one lane per chosen track, in display order.
#[derive(Debug, Clone, PartialEq)]
pub struct Excerpt {
    length: u64,
    lanes: Vec<Lane>,
}

impl Excerpt {
    /// The length of the range it was taken from, in frames.
    pub fn length(&self) -> u64 {
        self.length
    }

    /// Its lanes, in the display order of the tracks they came from.
    pub fn lanes(&self) -> &[Lane] {
        &self.lanes
    }
}

/// One track's part of an [`Excerpt`].
#[derive(Debug, Clone, PartialEq)]
pub struct Lane {
    layout: Layout,
    clips: Vec<Clip>,
}

impl Lane {
    /// The layout of the track it came from.
    pub fn layout(&self) -> Layout {
        self.layout
    }

    /// The clip parts inside the range, bottom layer first, positioned relative to the range
    /// start.
    pub fn clips(&self) -> &[Clip] {
        &self.clips
    }
}

/// The part of `clip` inside the timeline frames `from..to`, or `None` if they do not overlap.
/// A fade survives only at an edge that was the clip's own, shortened to fit.
fn part(clip: &Clip, from: u64, to: u64) -> Option<Clip> {
    let start = from.max(clip.position);
    let end = to.min(clip.end());
    if start >= end {
        return None;
    }
    let length = end - start;
    let fade_in = if start == clip.position {
        clip.fade_in.min(length)
    } else {
        0
    };
    let fade_out = if end == clip.end() {
        clip.fade_out.min(length - fade_in)
    } else {
        0
    };
    Some(Clip {
        source: clip.source.clone(),
        offset: clip.offset + (start - clip.position),
        length,
        position: start,
        gain_db: clip.gain_db,
        fade_in,
        fade_out,
    })
}

/// The indices of `tracks` in display order, each once.
fn chosen(state: &State, tracks: &[TrackId]) -> Result<BTreeSet<usize>, Error> {
    tracks.iter().map(|&id| state.track_index(id)).collect()
}

/// Copies the parts of the clips on `tracks` inside `range`.
pub(crate) fn excerpt(state: &State, tracks: &[TrackId], range: Range) -> Result<Excerpt, Error> {
    let lanes = chosen(state, tracks)?
        .into_iter()
        .map(|index| {
            let track = &state.tracks[index];
            let clips = track
                .clips
                .iter()
                .filter_map(|(_, clip)| part(clip, range.start, range.end))
                .map(|clip| Clip {
                    position: clip.position - range.start,
                    ..clip
                })
                .collect();
            Lane {
                layout: track.layout,
                clips,
            }
        })
        .collect();
    Ok(Excerpt {
        length: range.length(),
        lanes,
    })
}

/// The edits that remove everything inside `range` on `tracks`, splitting clips at its edges;
/// empty if nothing lies there. `new_id` names the right piece of a clip straddling the range.
pub(crate) fn delete_edits(
    state: &State,
    tracks: &[TrackId],
    range: Range,
    mut new_id: impl FnMut() -> ClipId,
) -> Result<Vec<Edit>, Error> {
    let mut edits = Vec::new();
    for index in chosen(state, tracks)? {
        let track = &state.tracks[index];
        // Top down, so the layers still to visit keep their indices as edits above them apply.
        for (layer, (id, clip)) in track.clips.iter().enumerate().rev() {
            if clip.end() <= range.start || clip.position >= range.end {
                continue;
            }
            let left = part(clip, 0, range.start);
            let right = part(clip, range.end, u64::MAX);
            match (left, right) {
                (None, None) => edits.push(Edit::RemoveClip(*id)),
                (Some(piece), None) | (None, Some(piece)) => edits.push(Edit::ReplaceClip {
                    id: *id,
                    clip: piece,
                }),
                (Some(left), Some(right)) => {
                    edits.push(Edit::ReplaceClip {
                        id: *id,
                        clip: left,
                    });
                    edits.push(Edit::InsertClip {
                        track: track.id,
                        layer: layer + 1,
                        id: new_id(),
                        clip: right,
                    });
                }
            }
        }
    }
    Ok(edits)
}

/// The edits that paste `excerpt` with its start at `position`, its lanes going onto `track` and
/// the tracks below it, and the new clips' ids in the order they are inserted.
pub(crate) fn paste_edits(
    state: &State,
    excerpt: &Excerpt,
    track: TrackId,
    position: u64,
    mut new_id: impl FnMut() -> ClipId,
) -> Result<(Vec<Edit>, Vec<ClipId>), Error> {
    let first = state.track_index(track)?;
    let targets = &state.tracks[first..];
    if excerpt.lanes.len() > targets.len() {
        return Err(Error::NotEnoughTracks {
            needed: excerpt.lanes.len(),
            available: targets.len(),
        });
    }
    if excerpt
        .lanes
        .iter()
        .zip(targets)
        .any(|(lane, target)| lane.layout != target.layout)
    {
        return Err(Error::LayoutMismatch);
    }
    let mut edits = Vec::new();
    let mut ids = Vec::new();
    for (lane, target) in excerpt.lanes.iter().zip(targets) {
        // Each insert goes on top of the track and of those already queued.
        for (layer, clip) in (target.clips.len()..).zip(&lane.clips) {
            let at = position
                .checked_add(clip.position)
                .ok_or(Error::ClipOutsideTimeline)?;
            let id = new_id();
            edits.push(Edit::InsertClip {
                track: target.id,
                layer,
                id,
                clip: Clip {
                    position: at,
                    ..clip.clone()
                },
            });
            ids.push(id);
        }
    }
    Ok((edits, ids))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{AudioFile, Source};

    fn clip(position: u64, length: u64, fade_in: u64, fade_out: u64) -> Clip {
        let source = Source::Mono(AudioFile {
            name: "a.wav".into(),
            frames: 10_000,
        });
        Clip {
            fade_in,
            fade_out,
            ..Clip::new(source, 100, length, position)
        }
    }

    #[test]
    fn test_part_interior_moves_offset_and_drops_fades() {
        let whole = clip(1_000, 500, 50, 60);
        let piece = part(&whole, 1_100, 1_300);
        assert_eq!(
            piece,
            Some(Clip {
                offset: 200,
                length: 200,
                position: 1_100,
                fade_in: 0,
                fade_out: 0,
                ..whole
            })
        );
    }

    #[test]
    fn test_part_edge_piece_keeps_its_fade_shortened() {
        let whole = clip(1_000, 500, 50, 60);
        let left = part(&whole, 0, 1_030).map(|c| (c.length, c.fade_in, c.fade_out));
        assert_eq!(left, Some((30, 30, 0)));
        let right = part(&whole, 1_200, u64::MAX).map(|c| (c.offset, c.fade_in, c.fade_out));
        assert_eq!(right, Some((300, 0, 60)));
    }

    #[test]
    fn test_part_fades_still_fit_after_split() -> Result<(), Error> {
        let whole = clip(0, 100, 70, 30);
        for edge in 1..100 {
            for piece in [part(&whole, 0, edge), part(&whole, edge, u64::MAX)]
                .into_iter()
                .flatten()
            {
                piece.check(Layout::Mono)?;
                assert!(piece.fade_in + piece.fade_out <= piece.length);
            }
        }
        let piece = part(&whole, 0, 100).map(|c| (c.fade_in, c.fade_out));
        assert_eq!(piece, Some((70, 30)));
        Ok(())
    }

    #[test]
    fn test_part_no_overlap_is_none() {
        let whole = clip(1_000, 500, 0, 0);
        assert_eq!(part(&whole, 0, 1_000), None);
        assert_eq!(part(&whole, 1_500, 2_000), None);
    }
}
