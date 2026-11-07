//! [`AppServices`]: `debussy_ui`'s I/O, from the bundle, disk streaming and the waveform caches
//! (ADR-0056).

use std::collections::BTreeMap;
use std::fs::File;
use std::io::BufReader;

use audio_analysis::{Bucket, WaveformReader};
use audio_engine::{Streams, TrackFeed, TrackFeeds};
use debussy_ui::Services;
use disk_streaming::{FrameSource, Streamer, StreamerConfig, TimelineSource, WavClipAudio};
use engine_bridge::track_renderers;
use project_model::{Session, State, TrackId};
use project_storage::Bundle;
use signal_graph::NodeId;

use crate::project::waveform_cache;

/// Streams before any project audio is loaded: nothing to move.
struct NoStreams;

impl Streams for NoStreams {
    fn seek(&mut self, _frame: u64) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(())
    }
}

/// The application's services.
pub struct AppServices {
    bundle: Bundle,
    streamer: Option<Streamer>,
    no_streams: NoStreams,
    /// Open waveform summaries, by stored file name; `None` where there is none.
    waveforms: BTreeMap<String, Option<WaveformReader<BufReader<File>>>>,
}

impl AppServices {
    /// Services over the open `bundle`.
    pub fn new(bundle: Bundle) -> Self {
        AppServices {
            bundle,
            streamer: None,
            no_streams: NoStreams,
            waveforms: BTreeMap::new(),
        }
    }
}

impl Services for AppServices {
    fn streams(&mut self) -> &mut dyn Streams {
        match &mut self.streamer {
            Some(streamer) => streamer,
            None => &mut self.no_streams,
        }
    }

    fn track_feeds(
        &mut self,
        state: &State,
        sources: &BTreeMap<TrackId, NodeId>,
    ) -> Result<Vec<TrackFeeds>, String> {
        let rate = self.bundle.sample_rate();
        let (tracks, files) = track_renderers(state, sources, rate).map_err(|e| e.to_string())?;
        let paths: Vec<_> = files
            .names()
            .iter()
            .map(|n| self.bundle.audio_dir().join(n))
            .collect();
        // One stream per track channel, in order; remember which track each belongs to.
        let mut layout = Vec::with_capacity(tracks.len());
        let mut streams: Vec<Box<dyn FrameSource>> = Vec::new();
        for track in tracks {
            layout.push((track.source, track.channels.len()));
            for renderer in track.channels {
                streams.push(Box::new(TimelineSource::new(
                    renderer,
                    WavClipAudio::new(paths.clone()),
                    rate,
                )));
            }
        }
        // The old streamer stops here; the engine is stopped, so its feeds are not being read.
        self.streamer = None;
        let (streamer, playbacks) =
            Streamer::start(streams, StreamerConfig::new(rate)).map_err(|e| e.to_string())?;
        self.streamer = Some(streamer);
        let mut playbacks = playbacks.into_iter();
        Ok(layout
            .into_iter()
            .map(|(source, channels)| TrackFeeds {
                source,
                feeds: playbacks
                    .by_ref()
                    .take(channels)
                    .map(|p| Box::new(p) as Box<dyn TrackFeed>)
                    .collect(),
            })
            .collect())
    }

    fn waveform(&mut self, file: &str, start: u64, spc: u64, out: &mut [Bucket]) -> usize {
        let bundle = &self.bundle;
        let reader = self.waveforms.entry(file.to_owned()).or_insert_with(|| {
            File::open(waveform_cache(bundle, file))
                .ok()
                .and_then(|f| WaveformReader::new(BufReader::new(f)).ok())
        });
        match reader {
            Some(reader) => reader.columns(start, spc, out).unwrap_or(0),
            None => 0,
        }
    }

    fn save(&mut self, session: &Session) -> Result<(), String> {
        self.bundle.save(session).map_err(|e| e.to_string())
    }
}
