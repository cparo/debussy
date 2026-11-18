//! The project bundle (`docs/architecture/project.md` "Project bundle", "Saved format", "Saving",
//! "Loading"): reading and writing `project.json` and the named versions, migrating older
//! formats, saving crash-safely, and cleaning up unreferenced audio.
//!
//! [`Bundle::create`] makes a bundle; [`Bundle::open`] reads one into a
//! [`Session`](project_model::Session), checking it as untrusted input; [`Bundle::save`] writes
//! the session back; [`Bundle::clean_up`] deletes audio nothing refers to. [`write_archive`] and
//! [`unpack_archive`] turn a bundle into one zip file and back ("Project archives").
//!
//! # Modules
//!
//! - `format`: the saved types, which are the file format, and [`FORMAT_VERSION`].
//! - `validate`: conversion to and from `project_model`, with every check and limit.
//! - `migrate`: load limits on the text, the format version, and migrations.
//! - `bundle`: [`Bundle`]: the directory, crash-safe writes, version stamps, cleanup.
//! - `archive`: project archives, checked as untrusted input when unpacked.
//! - `recovery`: the recording marker, and recovering takes a crash cut off.
//! - `settings`: [`AppSettings`], the application's settings outside any project.
//! - `error`: [`Error`].

#![forbid(unsafe_code)]

mod archive;
mod bundle;
mod error;
mod format;
mod migrate;
mod recovery;
mod settings;
mod validate;

pub use archive::{
    ARCHIVE_SPACE_MARGIN, ARCHIVE_SUFFIX, MAX_ARCHIVE_ENTRIES, unpack_archive, write_archive,
};
pub use bundle::{Bundle, MAX_FILE_BYTES, MAX_VERSIONS};
pub use error::Error;
pub use format::FORMAT_VERSION;
pub use migrate::MAX_DEPTH;
pub use recovery::{
    MAX_RECORDING_BYTES, Recording, RecordingStart, RecoveredMidi, RecoveredPass, Recovery,
};
pub use settings::{
    AppSettings, ClickSound, ExportDepth, ExportFormatName, ExportPreferences, MAX_CLICK_LEVEL_DB,
    MAX_MIDI_LATENCY_MS, MAX_MIDI_OUTPUTS, MIN_CLICK_LEVEL_DB, MidiOutSettings, SETTINGS_VERSION,
};
pub use validate::{
    MAX_CLIPS, MAX_CONNECTIONS, MAX_FILE_NAME_BYTES, MAX_MIDI_EVENTS, MAX_MIXER_CHANNELS,
    MAX_NAME_BYTES, MAX_NODES, MAX_PARAMETERS, MAX_SEQUENCES, MAX_TRACKS, SAMPLE_RATES,
};
