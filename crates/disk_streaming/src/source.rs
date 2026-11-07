//! What the reader thread reads from: a seekable source of mono frames.
//!
//! Files are opened and read only by the reader thread, never the real-time side, so this module
//! allows the file I/O that `clippy.toml` bans.
#![allow(clippy::disallowed_types)]

use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use wav_files::{Channels, WavReader};

use crate::error::Error;

/// A seekable source of mono `f32` frames. Implemented for WAV files; tests supply their own.
pub trait FrameSource: Send {
    /// Frames per second.
    fn sample_rate(&self) -> u32;
    /// The number of frames.
    fn frames(&self) -> u64;
    /// Moves to `frame`.
    ///
    /// # Errors
    ///
    /// If the frame is past the end or the underlying read fails.
    fn seek(&mut self, frame: u64) -> Result<(), Error>;
    /// Fills `out` from the current position, returning the frames read (0 at the end).
    ///
    /// # Errors
    ///
    /// If the underlying read fails.
    fn read(&mut self, out: &mut [f32]) -> Result<usize, Error>;
}

impl<R: std::io::Read + std::io::Seek + Send> FrameSource for WavReader<R> {
    fn sample_rate(&self) -> u32 {
        self.spec().sample_rate
    }

    fn frames(&self) -> u64 {
        WavReader::frames(self)
    }

    fn seek(&mut self, frame: u64) -> Result<(), Error> {
        Ok(WavReader::seek(self, frame)?)
    }

    fn read(&mut self, out: &mut [f32]) -> Result<usize, Error> {
        Ok(WavReader::read(self, &mut [out])?)
    }
}

/// Opens a mono WAV file of stored audio as a [`FrameSource`].
///
/// # Errors
///
/// [`Error::Io`] or [`Error::Wav`] if it cannot be opened or parsed; [`Error::NotMono`] for a
/// stereo file.
pub fn open_wav(path: &Path) -> Result<Box<dyn FrameSource>, Error> {
    let reader = WavReader::new(BufReader::with_capacity(256 * 1024, File::open(path)?))?;
    if reader.spec().channels != Channels::Mono {
        return Err(Error::NotMono);
    }
    Ok(Box::new(reader))
}
