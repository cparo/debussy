//! Compressed-audio codecs (`docs/architecture/platform.md` "Compressed audio codecs").

use std::path::Path;

use crate::error::Error;

/// A compressed format the platform may handle (ADR-0024). MP3 is decode-only here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CodecFormat {
    /// MPEG-1/2 Layer III; decode only.
    Mp3,
    /// AAC in an MPEG-4 container.
    Aac,
    /// FLAC.
    Flac,
}

/// The integer depth of a FLAC file. Dither is applied before encoding, in `dsp` (ADR-0025).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlacDepth {
    /// 16-bit.
    Int16,
    /// 24-bit.
    Int24,
}

/// How to encode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncodeSettings {
    /// AAC at a bit rate in kilobits per second.
    Aac {
        /// Bit rate in kbit/s.
        bitrate_kbps: u32,
    },
    /// Lossless FLAC at a bit depth.
    Flac {
        /// Bit depth.
        depth: FlacDepth,
    },
}

impl EncodeSettings {
    /// The format these settings produce.
    pub fn format(&self) -> CodecFormat {
        match self {
            EncodeSettings::Aac { .. } => CodecFormat::Aac,
            EncodeSettings::Flac { .. } => CodecFormat::Flac,
        }
    }
}

/// What a decoded source holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceInfo {
    /// The format.
    pub format: CodecFormat,
    /// Frames per second.
    pub sample_rate: u32,
    /// Channels per frame.
    pub channels: u16,
    /// Total frames, if the file says.
    pub frames: Option<u64>,
}

/// A streaming decoder, used off the real-time thread (import).
pub trait Decoder: Send {
    /// What the source holds.
    fn info(&self) -> SourceInfo;
    /// Fills `interleaved` with whole frames and returns the number of frames; 0 at the end.
    ///
    /// # Errors
    ///
    /// [`Error::Undecodable`], [`Error::Io`] or [`Error::Platform`].
    fn read(&mut self, interleaved: &mut [f32]) -> Result<usize, Error>;
}

/// A streaming encoder, used off the real-time thread (export).
pub trait Encoder: Send {
    /// Appends whole interleaved frames.
    ///
    /// # Errors
    ///
    /// [`Error::Io`] or [`Error::Platform`].
    fn write(&mut self, interleaved: &[f32]) -> Result<(), Error>;
    /// Flushes and closes the file.
    ///
    /// # Errors
    ///
    /// [`Error::Io`] or [`Error::Platform`].
    fn finish(self: Box<Self>) -> Result<(), Error>;
}

/// The platform's codecs.
pub trait Codecs: Send + Sync {
    /// Whether this platform can decode `format`.
    fn can_decode(&self, format: CodecFormat) -> bool;
    /// Whether this platform can encode `format`. MP3 is never encoded here (ADR-0024); MP3
    /// export runs a host-installed `lame` instead (ADR-0129).
    fn can_encode(&self, format: CodecFormat) -> bool;
    /// Opens `path` for decoding, detecting its format.
    ///
    /// # Errors
    ///
    /// [`Error::Io`], [`Error::Undecodable`], or [`Error::UnsupportedFormat`].
    fn decode(&self, path: &Path) -> Result<Box<dyn Decoder>, Error>;
    /// Creates `path` and returns an encoder for `channels` channels at `sample_rate`.
    ///
    /// # Errors
    ///
    /// [`Error::Io`], [`Error::UnsupportedFormat`], or [`Error::Platform`].
    fn encode(
        &self,
        path: &Path,
        settings: EncodeSettings,
        sample_rate: u32,
        channels: u16,
    ) -> Result<Box<dyn Encoder>, Error>;
}
