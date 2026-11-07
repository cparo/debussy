//! The crate's only `unsafe` code: safe wrappers over the AudioToolbox calls the codecs need
//! (ADR-0051). Everything outside this module denies `unsafe_code`.
//!
//! - `ext_audio_file`: [`ExtAudioFile`], an owned `ExtAudioFileRef` disposed on drop, with typed
//!   property access, reads and writes of interleaved `f32`.

#![allow(unsafe_code)]

mod ext_audio_file;

pub(crate) use ext_audio_file::{ExtAudioFile, Status, float_format, format_info};
