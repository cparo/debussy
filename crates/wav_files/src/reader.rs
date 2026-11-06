//! Streaming reads of a WAV file's samples, as `f32` per channel.

use std::io::{Read, Seek, SeekFrom};

use crate::error::Error;
use crate::format::Spec;
use crate::header::{self, Header};

/// The size of the reader's and writer's fixed scratch buffer (ADR-0029).
pub(crate) const SCRATCH_BYTES: usize = 64 * 1024;

/// Reads the samples of a WAV or RF64 file, any frame range at a time, without loading the file.
#[derive(Debug)]
pub struct WavReader<R> {
    inner: R,
    header: Header,
    /// The next frame [`WavReader::read`] returns.
    position: u64,
    /// Raw bytes for whole frames; never sized from the header.
    scratch: Box<[u8]>,
}

impl<R: Read + Seek> WavReader<R> {
    /// Parses the header of the file starting at `inner`'s current position and positions the
    /// reader at frame 0.
    ///
    /// # Errors
    ///
    /// Any header error in [`Error`], or [`Error::Io`] if `inner` fails.
    pub fn new(mut inner: R) -> Result<Self, Error> {
        let header = header::parse(&mut inner)?;
        inner.seek(SeekFrom::Start(header.data_start))?;
        Ok(Self {
            inner,
            header,
            position: 0,
            scratch: vec![0; SCRATCH_BYTES].into_boxed_slice(),
        })
    }

    /// The file's channels, sample rate and encoding.
    pub fn spec(&self) -> Spec {
        self.header.spec
    }

    /// The number of whole frames in the file.
    pub fn frames(&self) -> u64 {
        self.header.frames
    }

    /// Whether the file claimed more audio than it holds, as a crashed or streamed recording
    /// may. [`WavReader::frames`] counts only the frames present.
    pub fn truncated(&self) -> bool {
        self.header.truncated
    }

    /// The next frame [`WavReader::read`] will return.
    pub fn position(&self) -> u64 {
        self.position
    }

    /// Moves to `frame`, which may equal [`WavReader::frames`] (the end).
    ///
    /// # Errors
    ///
    /// [`Error::FrameOutOfRange`] past the end, or [`Error::Io`] if the seek fails.
    pub fn seek(&mut self, frame: u64) -> Result<(), Error> {
        if frame > self.header.frames {
            return Err(Error::FrameOutOfRange {
                frame,
                frames: self.header.frames,
            });
        }
        let offset = frame
            .checked_mul(self.header.spec.block_align() as u64)
            .and_then(|o| o.checked_add(self.header.data_start))
            .ok_or(Error::SizeOverflow)?;
        self.inner.seek(SeekFrom::Start(offset))?;
        self.position = frame;
        Ok(())
    }

    /// Fills `channels` (one slice per channel, left first) from the current position and
    /// returns the number of frames read: the shortest slice's length, or fewer at the end of the
    /// file, where it returns 0.
    ///
    /// # Errors
    ///
    /// [`Error::ChannelCountMismatch`] if `channels` has the wrong number of slices, or
    /// [`Error::Io`] if reading fails (including a file that has shrunk since it was opened).
    pub fn read(&mut self, channels: &mut [&mut [f32]]) -> Result<usize, Error> {
        let spec = self.header.spec;
        let count = spec.channels.count();
        if channels.len() != count {
            return Err(Error::ChannelCountMismatch {
                expected: count,
                found: channels.len(),
            });
        }
        let wanted = channels.iter().map(|c| c.len()).min().unwrap_or(0);
        let remaining = self.header.frames - self.position;
        let frames = usize::try_from(remaining).map_or(wanted, |r| r.min(wanted));

        let sample_bytes = spec.format.bytes();
        let block_align = spec.block_align();
        let per_chunk = self.scratch.len() / block_align;
        let mut done = 0;
        while done < frames {
            let n = (frames - done).min(per_chunk);
            let bytes = &mut self.scratch[..n * block_align];
            self.inner.read_exact(bytes)?;
            for (f, frame) in bytes.chunks_exact(block_align).enumerate() {
                for (channel, sample) in channels.iter_mut().zip(frame.chunks_exact(sample_bytes)) {
                    channel[done + f] = spec.format.decode(sample);
                }
            }
            done += n;
            self.position += n as u64;
        }
        Ok(frames)
    }

    /// The underlying reader.
    pub fn into_inner(self) -> R {
        self.inner
    }
}
