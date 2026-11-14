//! [`Services`]: what the interface needs from I/O and the platform, which `debussy` supplies
//! (`docs/architecture/crates.md` "Dependency rules", ADR-0056).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use audio_analysis::Bucket;
use audio_buffers::Layout;
use audio_engine::{Processor, Streams, TakeSink, TrackFeeds};
use project_model::{Session, Source, State, TrackId};
use signal_graph::NodeId;

use crate::export_dialog::ExportSettings;

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

/// The new files of one armed track's take, and the sinks the engine records them through, one
/// per channel, left first.
pub struct TakeFiles {
    /// The files' names in the bundle's audio directory.
    pub files: Vec<String>,
    /// Where each channel's input goes.
    pub sinks: Vec<Box<dyn TakeSink>>,
}

/// What became of one take file, numbered across a recording pass in the order
/// [`Services::start_takes`] returned them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TakeStatus {
    /// The file is complete.
    Finished {
        /// The file's number in the pass.
        take: usize,
        /// Its length, in frames.
        frames: u64,
    },
    /// Writing the file failed.
    Failed {
        /// The file's number in the pass.
        take: usize,
        /// What went wrong.
        message: String,
    },
}

/// The click's per-machine preferences (`docs/architecture/recording.md` "Click track").
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClickPreferences {
    /// The sound.
    pub sound: engine_bridge::ClickSound,
    /// The level, in decibels: [`MIN_CLICK_LEVEL_DB`] to [`MAX_CLICK_LEVEL_DB`].
    pub level_db: f32,
}

impl Default for ClickPreferences {
    /// The Click sound at 0 dB.
    fn default() -> Self {
        ClickPreferences {
            sound: engine_bridge::ClickSound::default(),
            level_db: 0.0,
        }
    }
}

/// The quietest click level, in decibels.
pub const MIN_CLICK_LEVEL_DB: f32 = -24.0;
/// The loudest click level, in decibels.
pub const MAX_CLICK_LEVEL_DB: f32 = 6.0;

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

    /// The last export's settings, remembered between launches; the Export dialog opens on
    /// them (ADR-0130).
    fn export_settings(&mut self) -> ExportSettings;

    /// Whether MP3 export is offered: the host has LAME (ADR-0129).
    fn mp3_available(&mut self) -> bool;

    /// Asks the user where to export with `settings`, through a save dialog for that format
    /// only; `None` if the user cancelled.
    fn pick_export_path(&mut self, settings: &ExportSettings) -> Option<PathBuf>;

    /// Renders `state` to `path` with `settings`, and remembers them. Returns the file's name.
    ///
    /// # Errors
    ///
    /// A message for the status line.
    fn export(
        &mut self,
        state: &State,
        path: &Path,
        settings: &ExportSettings,
    ) -> Result<String, String>;

    /// The audio devices present, and the one in use.
    fn devices(&mut self) -> (Vec<DeviceChoice>, Option<String>);

    /// An error that has stopped the audio device since the last call, such as it being
    /// unplugged.
    fn audio_error(&mut self) -> Option<String>;

    /// Whether the audio device has stopped and taken the engine's real-time side with it, so
    /// the engine must be rebuilt and given to [`Services::adopt_engine`].
    fn audio_lost(&mut self) -> bool;

    /// Runs a rebuilt engine's real-time side, dropping the lost one, on the device with
    /// `device`'s id (remembering the choice), or with `None` on the remembered device if
    /// present, else the default output.
    ///
    /// # Errors
    ///
    /// A message for the status line; the processor is kept for a later device choice.
    fn adopt_engine(&mut self, processor: Processor, device: Option<&str>) -> Result<(), String>;

    /// Switches to the device with `id`, remembering the choice. Called only while stopped.
    ///
    /// # Errors
    ///
    /// A message for the status line; the previous device stays in use.
    fn set_device(&mut self, id: &str) -> Result<(), String>;

    /// Opens the audio device's inputs as well as its outputs (`true`), or outputs only. Called
    /// only while stopped, when the first track is armed or the last disarmed.
    ///
    /// # Errors
    ///
    /// A message for the status line.
    fn set_input(&mut self, input: bool) -> Result<(), String>;

    /// The input channels open on the device; 0 when none are.
    fn input_channels(&mut self) -> usize;

    /// The device's input plus output latency, in frames, for placing takes (ADR-0022).
    fn device_latency(&mut self) -> u64;

    /// Starts recording one take per armed track, each named after the track and holding
    /// `channels` channels: creates the files and starts writing what the sinks are given.
    ///
    /// # Errors
    ///
    /// A message for the status line; no files are left behind.
    fn start_takes(&mut self, tracks: &[(String, usize)]) -> Result<Vec<TakeFiles>, String>;

    /// What has become of the current pass's files since the last call.
    fn take_status(&mut self) -> Vec<TakeStatus>;

    /// How many times the current pass's writing fell behind and dropped audio.
    fn take_overruns(&mut self) -> u64;

    /// Ends the pass, keeping `files` as stored audio: summarises them for the lanes.
    ///
    /// # Errors
    ///
    /// A message for the status line; the files are kept regardless.
    fn keep_takes(&mut self, files: &[String]) -> Result<(), String>;

    /// Ends the pass, deleting `files`.
    fn discard_takes(&mut self, files: &[String]);

    /// The click preferences remembered on this machine.
    fn click_preferences(&mut self) -> ClickPreferences;

    /// Remembers the click preferences on this machine.
    ///
    /// # Errors
    ///
    /// A message for the status line.
    fn set_click_preferences(&mut self, preferences: ClickPreferences) -> Result<(), String>;
}
