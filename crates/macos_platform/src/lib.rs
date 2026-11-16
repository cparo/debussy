//! `platform_interfaces` for macOS (`docs/architecture/platform.md`): audio devices through
//! `cpal` ([`MacDevices`]), MP3, AAC and FLAC through AudioToolbox ([`MacCodecs`]), and where
//! the app keeps its files ([`folders`]), documents opened from the Finder ([`documents`]), and
//! MIDI devices through CoreMIDI ([`MacMidi`]). Nothing outside this crate sees a `cpal` or Apple
//! type.
//!
//! `unsafe` code is confined to `sys` (ADR-0051); the rest of the crate denies it.
//!
//! # Modules
//!
//! - `devices`: [`MacDevices`], enumeration and duplex streams over `cpal` (ADR-0052).
//! - `ring`: the lock-free sample ring between `cpal`'s input and output callbacks.
//! - `codecs`: [`MacCodecs`], decoding and encoding through AudioToolbox, with the checks on
//!   untrusted files.
//! - `midi_devices`: [`MacMidi`], CoreMIDI endpoints, hot-plug, input validation and output
//!   (ADR-0146).
//! - [`documents`]: paths of projects the Finder asks the app to open (ADR-0132).
//! - [`folders`]: the projects folder and the settings file's path.
//! - `sys`: the `unsafe` AudioToolbox, Apple Event and CoreMIDI wrappers.

#![deny(unsafe_code)]

mod codecs;
mod devices;
pub mod documents;
pub mod folders;
mod midi_devices;
mod ring;
mod sys;

pub use codecs::{MAX_CHANNELS, MAX_SAMPLE_RATE, MAX_SECONDS, MIN_SAMPLE_RATE, MacCodecs};
pub use devices::MacDevices;
pub use midi_devices::MacMidi;
