//! The project state: tracks holding clips (`docs/architecture/project.md` "Editing model").

use audio_buffers::Layout;

use crate::error::Error;
use crate::routing::{Connection, ConnectionId, Endpoint, Node, NodeId};
use crate::types::{ClipId, Source, TrackId};

/// A stretch of a source placed on the timeline. All times are frames at the project rate.
#[derive(Debug, Clone, PartialEq)]
pub struct Clip {
    /// The audio it plays.
    pub source: Source,
    /// Where in the source it starts.
    pub offset: u64,
    /// How many frames it plays.
    pub length: u64,
    /// Where on the timeline it starts.
    pub position: u64,
    /// Its gain, in decibels.
    pub gain_db: f32,
    /// Its fade-in length.
    pub fade_in: u64,
    /// Its fade-out length.
    pub fade_out: u64,
}

impl Clip {
    /// A clip playing `length` frames of `source` from `offset`, at `position`, at 0 dB with no
    /// fades.
    pub fn new(source: Source, offset: u64, length: u64, position: u64) -> Self {
        Clip {
            source,
            offset,
            length,
            position,
            gain_db: 0.0,
            fade_in: 0,
            fade_out: 0,
        }
    }

    /// The timeline frame just after the clip.
    pub fn end(&self) -> u64 {
        self.position.saturating_add(self.length)
    }

    /// Checks the clip is playable on a track of `layout`.
    ///
    /// # Errors
    ///
    /// [`Error::LayoutMismatch`], [`Error::EmptyClip`], [`Error::ClipOutsideSource`],
    /// [`Error::ClipOutsideTimeline`], [`Error::FadesTooLong`] or [`Error::InvalidGain`].
    pub fn check(&self, layout: Layout) -> Result<(), Error> {
        if self.source.layout() != layout {
            return Err(Error::LayoutMismatch);
        }
        if self.length == 0 {
            return Err(Error::EmptyClip);
        }
        let frames = self.source.frames();
        match self.offset.checked_add(self.length) {
            Some(end) if end <= frames => {}
            end => {
                return Err(Error::ClipOutsideSource {
                    end: end.unwrap_or(u64::MAX),
                    frames,
                });
            }
        }
        if self.position.checked_add(self.length).is_none() {
            return Err(Error::ClipOutsideTimeline);
        }
        let fades = self.fade_in.saturating_add(self.fade_out);
        if fades > self.length {
            return Err(Error::FadesTooLong {
                fades,
                length: self.length,
            });
        }
        if !self.gain_db.is_finite() {
            return Err(Error::InvalidGain);
        }
        Ok(())
    }
}

/// A track: a name, a channel layout fixed at creation, and clips.
#[derive(Debug, Clone, PartialEq)]
pub struct Track {
    /// The track's id.
    pub id: TrackId,
    /// A name for display.
    pub name: String,
    /// Whether its clips are mono or stereo.
    pub layout: Layout,
    /// Its clips from the bottom layer up. Where clips overlap, the higher one plays.
    pub clips: Vec<(ClipId, Clip)>,
}

impl Track {
    /// An empty track.
    pub fn new(id: TrackId, name: &str, layout: Layout) -> Self {
        Track {
            id,
            name: name.to_owned(),
            layout,
            clips: Vec::new(),
        }
    }

    /// A clip on this track, by id.
    pub fn clip(&self, id: ClipId) -> Option<&Clip> {
        self.clips
            .iter()
            .find(|(c, _)| *c == id)
            .map(|(_, clip)| clip)
    }
}

/// The whole editable state of a project: its tracks, in display order, and its routing.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct State {
    /// The tracks, top of the arrangement first.
    pub tracks: Vec<Track>,
    /// The routing nodes besides tracks and the project output.
    pub nodes: Vec<Node>,
    /// The connections, in the order they were made.
    pub connections: Vec<Connection>,
}

impl State {
    /// A routing node, by id.
    pub fn node(&self, id: NodeId) -> Option<&Node> {
        self.nodes.iter().find(|node| node.id == id)
    }

    /// A connection, by id.
    pub fn connection(&self, id: ConnectionId) -> Option<&Connection> {
        self.connections.iter().find(|c| c.id == id)
    }

    /// Whether `endpoint` exists: the output always does.
    pub fn has_endpoint(&self, endpoint: Endpoint) -> bool {
        match endpoint {
            Endpoint::Track(id) => self.track(id).is_some(),
            Endpoint::Output => true,
            Endpoint::Node(id) => self.node(id).is_some(),
        }
    }

    /// The connections to or from `endpoint`.
    pub fn connections_of(&self, endpoint: Endpoint) -> impl Iterator<Item = &Connection> {
        self.connections.iter().filter(move |c| c.touches(endpoint))
    }

    /// A track, by id.
    pub fn track(&self, id: TrackId) -> Option<&Track> {
        self.tracks.iter().find(|track| track.id == id)
    }

    /// The track holding clip `id`, and the clip's layer in it.
    pub fn find_clip(&self, id: ClipId) -> Option<(TrackId, usize)> {
        self.tracks.iter().find_map(|track| {
            let layer = track.clips.iter().position(|(c, _)| *c == id)?;
            Some((track.id, layer))
        })
    }

    /// A clip, by id, wherever it is.
    pub fn clip(&self, id: ClipId) -> Option<&Clip> {
        self.tracks.iter().find_map(|track| track.clip(id))
    }

    pub(crate) fn track_index(&self, id: TrackId) -> Result<usize, Error> {
        self.tracks
            .iter()
            .position(|track| track.id == id)
            .ok_or(Error::UnknownTrack(id))
    }

    pub(crate) fn track_mut(&mut self, id: TrackId) -> Result<&mut Track, Error> {
        let index = self.track_index(id)?;
        Ok(&mut self.tracks[index])
    }
}
