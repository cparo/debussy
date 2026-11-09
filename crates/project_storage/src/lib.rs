//! The project bundle (`docs/architecture/project.md` "Project bundle", "Saved format", "Saving",
//! "Loading"): reading and writing `project.json` and the named versions, migrating older
//! formats, saving crash-safely, and cleaning up unreferenced audio.
//!
//! [`Bundle::create`] makes a bundle; [`Bundle::open`] reads one into a
//! [`Session`](project_model::Session), checking it as untrusted input; [`Bundle::save`] writes
//! the session back; [`Bundle::clean_up`] deletes audio nothing refers to.
//!
//! # Modules
//!
//! - `format`: the saved types, which are the file format, and [`FORMAT_VERSION`].
//! - `validate`: conversion to and from `project_model`, with every check and limit.
//! - `migrate`: load limits on the text, the format version, and migrations.
//! - `bundle`: [`Bundle`]: the directory, crash-safe writes, version stamps, cleanup.
//! - `settings`: [`AppSettings`], the application's settings outside any project.
//! - `error`: [`Error`].

#![forbid(unsafe_code)]

mod bundle;
mod error;
mod format;
mod migrate;
mod settings;
mod validate;

pub use bundle::{Bundle, MAX_FILE_BYTES, MAX_VERSIONS};
pub use error::Error;
pub use format::FORMAT_VERSION;
pub use migrate::MAX_DEPTH;
pub use settings::{
    AppSettings, ClickSound, MAX_CLICK_LEVEL_DB, MIN_CLICK_LEVEL_DB, SETTINGS_VERSION,
};
pub use validate::{
    MAX_CLIPS, MAX_CONNECTIONS, MAX_FILE_NAME_BYTES, MAX_MIXER_CHANNELS, MAX_NAME_BYTES, MAX_NODES,
    MAX_PARAMETERS, MAX_TRACKS, SAMPLE_RATES,
};
