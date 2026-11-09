//! [`Services`]: what the interface needs from I/O and the platform, which `debussy` supplies
//! (`docs/architecture/crates.md` "Dependency rules", ADR-0056).

use std::collections::BTreeMap;

use audio_analysis::Bucket;
use audio_buffers::Layout;
use audio_engine::{Streams, TrackFeeds};
use project_model::{Session, Source, State, TrackId};
use signal_graph::NodeId;

/// A file the user imported, stored and ready to become a track.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportedTrack {
    /// A name for the track: the file's name.
    pub name: String,
    /// Mono or stereo.
    pub layout: Layout,
    /// The stored audio.
    pub source: Source,
    /// Its length, in frames at the project rate.
    pub frames: u64,
}

/// An audio device the user can choose.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceChoice {
    /// The platform's id for it.
    pub id: String,
    /// Its name, for display.
    pub name: String,
}

impl std::fmt::Display for DeviceChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.name)
    }
}

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

    /// Summarises stored audio file `file` into `out` from its waveform summary: one bucket per
    /// column of `samples_per_column` frames (at least the summaries' smallest bucket), from
    /// frame `start`. Returns how many columns hold audio; 0 if the file has no summary yet.
    fn waveform(
        &mut self,
        file: &str,
        start: u64,
        samples_per_column: u64,
        out: &mut [Bucket],
    ) -> usize;

    /// Reads stored audio file `file`'s frames from frame `start` into `out`, for zooms finer than
    /// the waveform summaries. Returns how many were read: fewer at the end of the file, 0 if it
    /// cannot be read.
    fn samples(&mut self, file: &str, start: u64, out: &mut [f32]) -> usize;

    /// Writes into `out` one spectrum per column of `samples_per_column` frames of stored audio
    /// file `file`, from frame `start`: the frame centred on each column, silent where it runs
    /// past either end of the file, reduced with
    /// [`spectrum_bands`](crate::spectrum_bands). Returns how many columns hold audio; 0 if the
    /// file cannot be read.
    fn spectrogram(
        &mut self,
        file: &str,
        start: u64,
        samples_per_column: u64,
        out: &mut [crate::Spectrum],
    ) -> usize;

    /// Saves the project.
    ///
    /// # Errors
    ///
    /// A message for the status line.
    fn save(&mut self, session: &Session) -> Result<(), String>;

    /// Asks the user for files to import, then imports and summarises them. An empty list if
    /// the user cancelled.
    ///
    /// # Errors
    ///
    /// A message for the status line; files imported before the failure are still returned
    /// in the message's place only if none failed.
    fn import(&mut self) -> Result<Vec<ImportedTrack>, String>;

    /// Asks the user where to export, then renders `state` there, as the mono mix if `mono`.
    /// The file's name, or `None` if the user cancelled.
    ///
    /// # Errors
    ///
    /// A message for the status line.
    fn export(&mut self, state: &State, mono: bool) -> Result<Option<String>, String>;

    /// The audio devices present, and the one in use.
    fn devices(&mut self) -> (Vec<DeviceChoice>, Option<String>);

    /// An error that has stopped the audio device since the last call, such as it being
    /// unplugged.
    fn audio_error(&mut self) -> Option<String>;

    /// Switches to the device with `id`, remembering the choice. Called only while stopped.
    ///
    /// # Errors
    ///
    /// A message for the status line; the previous device stays in use.
    fn set_device(&mut self, id: &str) -> Result<(), String>;
}
