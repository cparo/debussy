//! [`Services`]: what the interface needs from I/O and the platform, which `debussy` supplies
//! (`docs/architecture/crates.md` "Dependency rules", ADR-0056).

use std::collections::BTreeMap;

use audio_analysis::Bucket;
use audio_engine::{Streams, TrackFeeds};
use project_model::{Session, State, TrackId};
use signal_graph::NodeId;

/// I/O and platform services, implemented by the application with `disk_streaming`,
/// `project_storage` and the waveform cache, and by fakes in tests. Called on the UI thread.
pub trait Services {
    /// The handle that moves every track's disk stream when the transport starts or jumps.
    fn streams(&mut self) -> &mut dyn Streams;

    /// Disk-streamed feeds for `state`'s tracks, each filling its source in `sources`. Called
    /// while stopped, whenever the clips have changed since the last call.
    ///
    /// # Errors
    ///
    /// A message for the status line.
    fn track_feeds(
        &mut self,
        state: &State,
        sources: &BTreeMap<TrackId, NodeId>,
    ) -> Result<Vec<TrackFeeds>, String>;

    /// Summarises stored audio file `file` into `out`: one bucket per column of
    /// `samples_per_column` frames, from frame `start`. Returns how many columns hold audio;
    /// 0 if the file has no summary yet.
    fn waveform(
        &mut self,
        file: &str,
        start: u64,
        samples_per_column: u64,
        out: &mut [Bucket],
    ) -> usize;

    /// Saves the project.
    ///
    /// # Errors
    ///
    /// A message for the status line.
    fn save(&mut self, session: &Session) -> Result<(), String>;
}
