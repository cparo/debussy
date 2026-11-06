//! The crate's error type.

use std::fmt;
use std::io;

use crate::format::SampleFormat;

/// A four-byte RIFF chunk identifier, shown with non-printable bytes escaped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChunkId(pub [u8; 4]);

impl fmt::Display for ChunkId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0.escape_ascii())
    }
}

/// Why a WAV file could not be read or written.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The underlying reader or writer failed.
    #[error(transparent)]
    Io(#[from] io::Error),
    /// The file does not start with `RIFF` or `RF64`.
    #[error("not a RIFF or RF64 file")]
    NotRiff,
    /// The RIFF form type is not `WAVE`.
    #[error("not a WAVE file")]
    NotWave,
    /// The file ends inside its header.
    #[error("the file ends inside its header")]
    Truncated,
    /// An RF64 file whose first chunk is not `ds64`.
    #[error("RF64 file has no ds64 chunk")]
    MissingDs64,
    /// The `data` chunk comes before any `fmt ` chunk.
    #[error("no fmt chunk before the data chunk")]
    MissingFmt,
    /// The file has no `data` chunk.
    #[error("no data chunk")]
    MissingData,
    /// A chunk the reader must read is larger than its limit.
    #[error("{id} chunk of {size} bytes is over the {max}-byte limit")]
    ChunkTooLarge {
        /// The chunk.
        id: ChunkId,
        /// Its declared size.
        size: u64,
        /// The limit.
        max: u64,
    },
    /// A chunk is too small to hold its fields.
    #[error("{id} chunk of {size} bytes is too small")]
    ChunkTooSmall {
        /// The chunk.
        id: ChunkId,
        /// Its declared size.
        size: u64,
    },
    /// A chunk other than `data` runs past the end of the file.
    #[error("{id} chunk runs past the end of the file")]
    ChunkPastEnd {
        /// The chunk.
        id: ChunkId,
    },
    /// More chunks than the limit come before `data`.
    #[error("more than {0} chunks before the data chunk")]
    TooManyChunks(usize),
    /// A sample encoding the reader does not support.
    #[error("unsupported sample format (format tag {tag:#06x}, {bits} bits)")]
    UnsupportedFormat {
        /// The format tag (the sub-format's, for `WAVE_FORMAT_EXTENSIBLE`).
        tag: u16,
        /// Bits per sample.
        bits: u16,
    },
    /// A channel count other than mono or stereo (ADR-0029).
    #[error(
        "{0} channels: only mono and stereo files are supported; split it into mono or stereo files first"
    )]
    UnsupportedChannels(u16),
    /// A sample rate outside 1 Hz to [`crate::MAX_SAMPLE_RATE`].
    #[error("sample rate of {0} Hz is out of range")]
    SampleRateOutOfRange(u32),
    /// `block_align` does not match the channels and sample size.
    #[error("block align is {found}, expected {expected}")]
    BlockAlignMismatch {
        /// Channels × bytes per sample.
        expected: u16,
        /// The header's value.
        found: u16,
    },
    /// A size or offset overflows.
    #[error("a size in the file overflows")]
    SizeOverflow,
    /// The number of channel buffers passed does not match the file.
    #[error("{found} channel buffers given for a {expected}-channel file")]
    ChannelCountMismatch {
        /// The file's channel count.
        expected: usize,
        /// The number of buffers given.
        found: usize,
    },
    /// Channel buffers passed for writing have different lengths.
    #[error("channel buffers have different lengths")]
    ChannelLengthMismatch,
    /// A seek past the last frame.
    #[error("frame {frame} is past the end ({frames} frames)")]
    FrameOutOfRange {
        /// The requested frame.
        frame: u64,
        /// The file's frame count.
        frames: u64,
    },
    /// A sample format the writer does not produce.
    #[error("cannot write {0:?}: the writer produces 16- and 24-bit integer and 32-bit float")]
    UnsupportedWriteFormat(SampleFormat),
}
