//! Spectrogram data: [`Spectrogram`] frames in dB, computed on demand, and the coarse overview
//! that [`OverviewWriter`] caches and [`OverviewReader`] reads back (`docs/architecture/analysis.md`
//! "Spectrograms", "Analysis cache files").
//!
//! Frame `t` covers samples `t·HOP .. t·HOP + FRAME_SIZE`, zero-padded past the end of the file,
//! for `t` below `ceil(samples / HOP)`. Overview column `c` averages the power of the frames that
//! start in samples `c·OVERVIEW_COLUMN .. (c + 1)·OVERVIEW_COLUMN`.
//!
//! The overview file is the header (magic `DBSG`, version, [`FRAME_SIZE`], [`HOP`],
//! [`OVERVIEW_COLUMN`], [`BINS`], sample count) followed by each column's [`BINS`] bytes.

use std::io::{Read, Seek, SeekFrom, Write};

use dsp::Stft;

use crate::error::Error;
use crate::header::{read_start, read_u32, read_u64, write_u32, write_u64};

/// Samples per spectrogram frame.
pub const FRAME_SIZE: usize = 2048;

/// Samples between the starts of consecutive frames.
pub const HOP: usize = 512;

/// Magnitude bins per frame: `FRAME_SIZE / 2 + 1`, from 0 Hz to half the sample rate.
pub const BINS: usize = FRAME_SIZE / 2 + 1;

/// Samples per overview column.
pub const OVERVIEW_COLUMN: u64 = 16_384;

/// The lowest level a frame reports, in dB; anything quieter, silence included, reads this.
pub const FLOOR_DB: f32 = -120.0;

/// Frames per full overview column.
const FRAMES_PER_COLUMN: u64 = OVERVIEW_COLUMN / HOP as u64;
/// The file's magic.
const MAGIC: [u8; 4] = *b"DBSG";
/// The format version this build reads and writes.
const VERSION: u32 = 1;
/// Magic, version, frame size, hop, column, bins, sample count.
const HEADER_LEN: u64 = 4 + 4 + 4 + 4 + 4 + 4 + 8;

/// The level, in dB, an overview byte stands for: `-byte / 2`. Byte 255 means −127.5 dB or less.
pub fn overview_db(byte: u8) -> f32 {
    -f32::from(byte) / 2.0
}

/// The overview byte for a level in dB, in 0.5 dB steps, clamped to 0 (0 dB or more) and 255.
fn quantise(db: f64) -> u8 {
    let steps = (-2.0 * db).round();
    if steps.is_nan() || steps >= 255.0 {
        255
    } else if steps <= 0.0 {
        0
    } else {
        steps as u8
    }
}

/// The number of frames for `samples` samples.
fn frames(samples: u64) -> u64 {
    samples.div_ceil(HOP as u64)
}

/// The number of overview columns for `samples` samples.
fn columns(samples: u64) -> u64 {
    samples.div_ceil(OVERVIEW_COLUMN)
}

/// Computes spectrogram frames in dB, for views drawn on demand.
pub struct Spectrogram {
    stft: Stft,
    frame: Box<[f32]>,
    magnitudes: Box<[f32]>,
}

impl Spectrogram {
    /// A frame calculator. Allocates its buffers; computing frames then allocates nothing.
    ///
    /// # Errors
    ///
    /// [`Error::Dsp`], which cannot happen for the fixed [`FRAME_SIZE`].
    pub fn new() -> Result<Self, Error> {
        Ok(Spectrogram {
            stft: Stft::new(FRAME_SIZE)?,
            frame: vec![0.0; FRAME_SIZE].into_boxed_slice(),
            magnitudes: vec![0.0; BINS].into_boxed_slice(),
        })
    }

    /// Writes into `out` ([`BINS`] values) the level in dB of each bin of the frame that starts
    /// with `samples` (up to [`FRAME_SIZE`] of them; a frame that runs past the end of the file is
    /// given what remains and is zero-padded). A sine of amplitude 1 centred on a bin reads 0 dB
    /// there; levels are floored at [`FLOOR_DB`].
    ///
    /// # Errors
    ///
    /// [`Error::BufferLength`] for more than [`FRAME_SIZE`] samples or an `out` that is not
    /// [`BINS`] long.
    pub fn frame_db(&mut self, samples: &[f32], out: &mut [f32]) -> Result<(), Error> {
        if samples.len() > FRAME_SIZE {
            return Err(Error::BufferLength {
                expected: FRAME_SIZE,
                found: samples.len(),
            });
        }
        if out.len() != BINS {
            return Err(Error::BufferLength {
                expected: BINS,
                found: out.len(),
            });
        }
        self.frame[..samples.len()].copy_from_slice(samples);
        self.frame[samples.len()..].fill(0.0);
        self.stft.magnitudes(&self.frame, out)?;
        for level in out.iter_mut() {
            *level = (20.0 * level.log10()).max(FLOOR_DB);
        }
        Ok(())
    }

    /// The magnitudes of the frame held in `self.frame`, into `self.magnitudes`.
    fn magnitudes(&mut self) -> Result<&[f32], Error> {
        self.stft.magnitudes(&self.frame, &mut self.magnitudes)?;
        Ok(&self.magnitudes)
    }
}

/// Builds a spectrogram overview from one pass over the samples, writing its cache file as it
/// goes. Memory use is fixed, not proportional to the file's length.
pub struct OverviewWriter<W: Write> {
    inner: W,
    samples: u64,
    pushed: u64,
    spectrogram: Spectrogram,
    /// Samples held in `spectrogram.frame`, which always starts at the next frame's start.
    filled: usize,
    frames_done: u64,
    /// The summed power of each bin over the current column's frames.
    power: Box<[f64]>,
    frames_in_column: u64,
    column: Box<[u8]>,
}

impl<W: Write> OverviewWriter<W> {
    /// Starts an overview of `samples` samples, writing its header to `inner`.
    ///
    /// # Errors
    ///
    /// [`Error::Io`].
    pub fn new(mut inner: W, samples: u64) -> Result<Self, Error> {
        inner.write_all(&MAGIC)?;
        write_u32(&mut inner, VERSION)?;
        write_u32(&mut inner, FRAME_SIZE as u32)?;
        write_u32(&mut inner, HOP as u32)?;
        write_u32(&mut inner, OVERVIEW_COLUMN as u32)?;
        write_u32(&mut inner, BINS as u32)?;
        write_u64(&mut inner, samples)?;
        Ok(OverviewWriter {
            inner,
            samples,
            pushed: 0,
            spectrogram: Spectrogram::new()?,
            filled: 0,
            frames_done: 0,
            power: vec![0.0; BINS].into_boxed_slice(),
            frames_in_column: 0,
            column: vec![0; BINS].into_boxed_slice(),
        })
    }

    /// Adds the next `samples`.
    ///
    /// # Errors
    ///
    /// [`Error::SampleCount`] if this goes past the declared count; [`Error::Io`].
    pub fn push(&mut self, mut samples: &[f32]) -> Result<(), Error> {
        let total = self.pushed + samples.len() as u64;
        if total > self.samples {
            return Err(Error::SampleCount {
                expected: self.samples,
                found: total,
            });
        }
        while !samples.is_empty() {
            let take = samples.len().min(FRAME_SIZE - self.filled);
            self.spectrogram.frame[self.filled..self.filled + take]
                .copy_from_slice(&samples[..take]);
            self.filled += take;
            samples = &samples[take..];
            if self.filled == FRAME_SIZE {
                self.frame()?;
            }
        }
        self.pushed = total;
        Ok(())
    }

    /// Computes the frames that run past the end of the file, writes the last column and returns
    /// the writer.
    ///
    /// # Errors
    ///
    /// [`Error::SampleCount`] unless exactly the declared count was pushed; [`Error::Io`].
    pub fn finish(mut self) -> Result<W, Error> {
        if self.pushed != self.samples {
            return Err(Error::SampleCount {
                expected: self.samples,
                found: self.pushed,
            });
        }
        while self.frames_done < frames(self.samples) {
            self.spectrogram.frame[self.filled..].fill(0.0);
            self.filled = FRAME_SIZE;
            self.frame()?;
        }
        self.inner.flush()?;
        Ok(self.inner)
    }

    /// Adds the full frame in the buffer to the column, writes the column if it is complete, and
    /// moves the buffer on by one hop.
    fn frame(&mut self) -> Result<(), Error> {
        let magnitudes = self.spectrogram.magnitudes()?;
        for (power, &m) in self.power.iter_mut().zip(magnitudes) {
            *power += f64::from(m) * f64::from(m);
        }
        self.frames_done += 1;
        self.frames_in_column += 1;
        if self.frames_in_column == FRAMES_PER_COLUMN || self.frames_done == frames(self.samples) {
            let frames = self.frames_in_column as f64;
            for (byte, power) in self.column.iter_mut().zip(self.power.iter_mut()) {
                *byte = quantise(10.0 * (*power / frames).log10());
                *power = 0.0;
            }
            self.inner.write_all(&self.column)?;
            self.frames_in_column = 0;
        }
        self.spectrogram.frame.copy_within(HOP.., 0);
        self.filled -= HOP;
        Ok(())
    }
}

/// Reads columns of a spectrogram overview's cache file.
pub struct OverviewReader<R: Read + Seek> {
    inner: R,
    samples: u64,
}

impl<R: Read + Seek> OverviewReader<R> {
    /// Opens an overview, checking its header and length.
    ///
    /// # Errors
    ///
    /// [`Error::WrongMagic`], [`Error::UnsupportedVersion`], [`Error::ParameterMismatch`] or
    /// [`Error::WrongLength`] for a file to rebuild; [`Error::Io`].
    pub fn new(mut inner: R) -> Result<Self, Error> {
        let length = read_start(&mut inner, MAGIC, VERSION, "spectrogram", HEADER_LEN)?;
        let parameters = [
            read_u32(&mut inner)?,
            read_u32(&mut inner)?,
            read_u32(&mut inner)?,
            read_u32(&mut inner)?,
        ];
        let expected = [
            FRAME_SIZE as u32,
            HOP as u32,
            OVERVIEW_COLUMN as u32,
            BINS as u32,
        ];
        if parameters != expected {
            return Err(Error::ParameterMismatch);
        }
        let samples = read_u64(&mut inner)?;
        let expected = HEADER_LEN + columns(samples) * BINS as u64;
        if length != expected {
            return Err(Error::WrongLength {
                expected,
                found: length,
            });
        }
        Ok(OverviewReader { inner, samples })
    }

    /// The number of samples the overview covers.
    pub fn samples(&self) -> u64 {
        self.samples
    }

    /// The number of columns.
    pub fn columns(&self) -> u64 {
        columns(self.samples)
    }

    /// Reads columns from `first` into `out`, [`BINS`] bytes per column (see [`overview_db`]),
    /// returning how many columns there were (fewer than fit at the end of the file).
    ///
    /// # Errors
    ///
    /// [`Error::BufferLength`] unless `out` holds a whole number of columns; [`Error::Io`].
    pub fn read_columns(&mut self, first: u64, out: &mut [u8]) -> Result<usize, Error> {
        if !out.len().is_multiple_of(BINS) {
            return Err(Error::BufferLength {
                expected: BINS,
                found: out.len(),
            });
        }
        let count = self
            .columns()
            .saturating_sub(first)
            .min((out.len() / BINS) as u64) as usize;
        if count == 0 {
            return Ok(0);
        }
        self.inner
            .seek(SeekFrom::Start(HEADER_LEN + first * BINS as u64))?;
        self.inner.read_exact(&mut out[..count * BINS])?;
        Ok(count)
    }

    /// Returns the reader.
    pub fn into_inner(self) -> R {
        self.inner
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quantise_steps_and_clamps() {
        assert_eq!(quantise(3.0), 0);
        assert_eq!(quantise(0.0), 0);
        assert_eq!(quantise(-0.5), 1);
        assert_eq!(quantise(-6.0), 12);
        assert_eq!(quantise(-127.5), 255);
        assert_eq!(quantise(-200.0), 255);
        assert_eq!(quantise(f64::NEG_INFINITY), 255);
        assert_eq!(quantise(f64::NAN), 255);
    }

    #[test]
    fn test_overview_db_inverts_quantise() {
        for byte in 0..=255u8 {
            assert_eq!(quantise(f64::from(overview_db(byte))), byte);
        }
    }

    #[test]
    fn test_frames_and_columns_round_up() {
        assert_eq!(frames(0), 0);
        assert_eq!(frames(1), 1);
        assert_eq!(frames(513), 2);
        assert_eq!(columns(16_384), 1);
        assert_eq!(columns(16_385), 2);
    }
}
