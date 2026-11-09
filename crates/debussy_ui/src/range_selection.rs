//! The range selected in the lanes (`docs/architecture/user-interface.md` "Range selection and
//! shortcuts"): two corners, each a track and a frame.

use project_model::{Range, State, TrackId};

/// A corner of the selection: a track and a timeline frame.
pub type Corner = (TrackId, u64);

/// A range selection from the corner it was started at to the one being moved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RangeSelection {
    /// The corner that stays put.
    pub anchor: Corner,
    /// The corner the pointer moves.
    pub head: Corner,
}

impl RangeSelection {
    /// The selected tracks, top first, and frames; `None` if a corner's track is gone or the
    /// frames are empty.
    pub fn resolve(&self, state: &State) -> Option<(Vec<TrackId>, Range)> {
        let index = |id: TrackId| state.tracks.iter().position(|t| t.id == id);
        let (a, h) = (index(self.anchor.0)?, index(self.head.0)?);
        let tracks = state.tracks[a.min(h)..=a.max(h)]
            .iter()
            .map(|t| t.id)
            .collect();
        let (from, to) = (self.anchor.1, self.head.1);
        let range = Range::new(from.min(to), from.max(to)).ok()?;
        Some((tracks, range))
    }

    /// The selection a shift-press at `corner` makes of this one: its nearer edge, in frames and
    /// in tracks, moves to the press and the far end stays.
    pub fn extended(&self, state: &State, corner: Corner) -> Option<RangeSelection> {
        let (tracks, range) = self.resolve(state)?;
        let index = |id: TrackId| state.tracks.iter().position(|t| t.id == id);
        let (top, bottom) = (index(*tracks.first()?)?, index(*tracks.last()?)?);
        let pressed = index(corner.0)?;
        let far_track = if pressed.abs_diff(top) < pressed.abs_diff(bottom) {
            bottom
        } else {
            top
        };
        let far_frame = if corner.1.abs_diff(range.start()) < corner.1.abs_diff(range.end()) {
            range.end()
        } else {
            range.start()
        };
        Some(RangeSelection {
            anchor: (state.tracks[far_track].id, far_frame),
            head: corner,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use audio_buffers::Layout;
    use project_model::Track;

    fn state(n: u64) -> State {
        State {
            tracks: (0..n)
                .map(|i| Track::new(TrackId(i), "t", Layout::Mono))
                .collect(),
            ..State::default()
        }
    }

    #[test]
    fn test_resolve_orders_tracks_and_frames() -> Result<(), project_model::Error> {
        let selection = RangeSelection {
            anchor: (TrackId(2), 900),
            head: (TrackId(0), 100),
        };
        let resolved = selection.resolve(&state(3));
        let want = vec![TrackId(0), TrackId(1), TrackId(2)];
        assert_eq!(resolved, Some((want, Range::new(100, 900)?)));
        Ok(())
    }

    #[test]
    fn test_resolve_empty_or_missing_track_is_none() {
        let empty = RangeSelection {
            anchor: (TrackId(0), 5),
            head: (TrackId(1), 5),
        };
        assert_eq!(empty.resolve(&state(2)), None);
        let gone = RangeSelection {
            anchor: (TrackId(0), 5),
            head: (TrackId(9), 50),
        };
        assert_eq!(gone.resolve(&state(2)), None);
    }

    #[test]
    fn test_extended_moves_the_nearer_edge() {
        let selection = RangeSelection {
            anchor: (TrackId(1), 100),
            head: (TrackId(2), 1_000),
        };
        let s = state(4);
        // Near the start, on the bottom track: the end and the top track stay.
        assert_eq!(
            selection.extended(&s, (TrackId(3), 200)),
            Some(RangeSelection {
                anchor: (TrackId(1), 1_000),
                head: (TrackId(3), 200),
            })
        );
        // Past the end, above the top: the start and the bottom track stay.
        assert_eq!(
            selection.extended(&s, (TrackId(0), 2_000)),
            Some(RangeSelection {
                anchor: (TrackId(2), 100),
                head: (TrackId(0), 2_000),
            })
        );
    }
}
