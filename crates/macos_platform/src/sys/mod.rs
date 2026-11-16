//! The crate's only `unsafe` code: safe wrappers over the AudioToolbox calls the codecs need
//! (ADR-0051), over the Finder's open-documents Apple Event (ADR-0132), and over CoreMIDI
//! (ADR-0146). Everything outside
//! this module denies `unsafe_code`.
//!
//! - `ext_audio_file`: [`ExtAudioFile`], an owned `ExtAudioFileRef` disposed on drop, with typed
//!   property access, reads and writes of interleaved `f32`.
//! - `core_midi`: the CoreMIDI client, endpoints, ports and the host clock (ADR-0146).
//! - `open_documents`: the Apple Event handler for documents opened from the Finder.

#![allow(unsafe_code)]

pub(crate) mod core_midi;
mod ext_audio_file;
mod open_documents;

pub(crate) use ext_audio_file::{ExtAudioFile, Status, float_format, format_info};
pub(crate) use open_documents::install as install_open_documents;
