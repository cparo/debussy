//! Tracks as the engine plays them: each channel's clips as [`ChannelClip`]s in a
//! [`TrackRenderer`], with the stored files they read numbered in a [`FileTable`]
//! (`docs/architecture/engine.md` "Clip playback", ADR-0047, ADR-0054).

use std::collections::BTreeMap;

use audio_engine::{ChannelClip, TrackRenderer};
use dsp::db_to_gain;
use project_model::{Source, State, Track, TrackId};
use signal_graph::NodeId as GraphNodeId;

use crate::error::Error;

/// The stored files clips read, numbered as [`ChannelClip::file`] refers to them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileTable {
    names: Vec<String>,
    index: BTreeMap<String, usize>,
}

impl FileTable {
    /// The number for `name`, adding it if new.
    fn number(&mut self, name: &str) -> usize {
        if let Some(&n) = self.index.get(name) {
            return n;
        }
        let n = self.names.len();
        self.names.push(name.to_owned());
        self.index.insert(name.to_owned(), n);
        n
    }

    /// The file names, in number order; names are within the bundle's `audio/`.
    pub fn names(&self) -> &[String] {
        &self.names
    }
}

/// One track's renderers, one per channel (left first), and the graph source they feed.
#[derive(Debug)]
pub struct TrackRenderers {
    /// The track.
    pub track: TrackId,
    /// Its source in the graph.
    pub source: GraphNodeId,
    /// One renderer per channel of the track's layout.
    pub channels: Vec<TrackRenderer>,
}

/// Each channel's clips of `track`, bottom layer first.
fn channel_clips(track: &Track, files: &mut FileTable) -> Vec<Vec<ChannelClip>> {
    let channels = match track.layout {
        audio_buffers::Layout::Mono => 1,
        audio_buffers::Layout::Stereo => 2,
    };
    (0..channels)
        .map(|channel| {
            track
                .clips
                .iter()
                .map(|(_, clip)| {
                    let file = match (&clip.source, channel) {
                        (Source::Mono(file), _) => file,
                        (Source::Stereo { left, .. }, 0) => left,
                        (Source::Stereo { right, .. }, _) => right,
                    };
                    ChannelClip {
                        file: files.number(&file.name),
                        offset: clip.offset,
                        length: clip.length,
                        position: clip.position,
                        gain: db_to_gain(clip.gain_db),
                        fade_in: clip.fade_in,
                        fade_out: clip.fade_out,
                    }
                })
                .collect()
        })
        .collect()
}

/// Renderers for every track of `state` that has a source in `sources`, at `sample_rate`, and
/// the files they read. Feed them to disk streams for playback, or to `audio_engine`'s
/// `RenderFeed`s for export.
///
/// # Errors
///
/// [`Error::Engine`] for a clip the renderer refuses.
pub fn track_renderers(
    state: &State,
    sources: &BTreeMap<TrackId, GraphNodeId>,
    sample_rate: u32,
) -> Result<(Vec<TrackRenderers>, FileTable), Error> {
    let mut files = FileTable::default();
    let mut tracks = Vec::with_capacity(state.tracks.len());
    for track in &state.tracks {
        let Some(&source) = sources.get(&track.id) else {
            continue;
        };
        let channels = channel_clips(track, &mut files)
            .into_iter()
            .map(|clips| TrackRenderer::new(clips, sample_rate))
            .collect::<Result<Vec<_>, _>>()?;
        tracks.push(TrackRenderers {
            track: track.id,
            source,
            channels,
        });
    }
    Ok((tracks, files))
}
