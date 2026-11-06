//! Writing WAV files, switching to RF64 when the file passes 4 GB.
//!
//! The header is written first with a 28-byte `JUNK` chunk reserving room for a `ds64` chunk, and
//! [`WavWriter::finish`] fills in the sizes, turning the file into RF64 if they do not fit in 32
//! bits (EBU Tech 3306).

use std::io::{Seek, SeekFrom, Write};

use crate::error::Error;
use crate::format::{SampleFormat, Spec};
use crate::header::SIZE_IN_DS64;
use crate::reader::SCRATCH_BYTES;

/// Body size of the `JUNK`/`ds64` reserve: RIFF size, data size, sample count (u64 each) and an
/// empty table length (u32).
const DS64_BODY: u32 = 28;
/// Offset of the reserve chunk.
const RESERVE_AT: u64 = 12;

/// Writes a WAV file's samples, given as `f32` per channel. Integer formats are rounded and
/// clipped without dither; dither is applied before this, in `dsp`.
///
/// Call [`WavWriter::finish`]; a writer dropped without it leaves a file whose sizes are zero.
#[derive(Debug)]
pub struct WavWriter<W: Write + Seek> {
    inner: W,
    spec: Spec,
    /// Where the file starts in `inner`.
    base: u64,
    /// Offsets from `base`.
    fact_at: Option<u64>,
    data_size_at: u64,
    data_start: u64,
    data_bytes: u64,
    frames: u64,
    /// The RIFF size above which the file becomes RF64; lowered only by tests.
    rf64_threshold: u64,
    scratch: Box<[u8]>,
}

impl<W: Write + Seek> WavWriter<W> {
    /// Starts a file at `inner`'s current position.
    ///
    /// # Errors
    ///
    /// [`Error::UnsupportedWriteFormat`] for formats other than 16- and 24-bit integer and 32-bit
    /// float, [`Error::SampleRateOutOfRange`], or [`Error::Io`].
    pub fn new(mut inner: W, spec: Spec) -> Result<Self, Error> {
        if !matches!(
            spec.format,
            SampleFormat::Int16 | SampleFormat::Int24 | SampleFormat::Float32
        ) {
            return Err(Error::UnsupportedWriteFormat(spec.format));
        }
        Spec::check_rate(spec.sample_rate)?;
        let base = inner.stream_position()?;
        let float = spec.format == SampleFormat::Float32;
        // Mono or stereo × at most 4 bytes, so these fit.
        let block_align = spec.block_align() as u16;
        let channels = spec.channels.count() as u16;
        let bits = (spec.format.bytes() * 8) as u16;

        let mut h = Vec::with_capacity(96);
        h.extend_from_slice(b"RIFF\0\0\0\0WAVE");
        h.extend_from_slice(b"JUNK");
        h.extend_from_slice(&DS64_BODY.to_le_bytes());
        h.extend_from_slice(&[0; DS64_BODY as usize]);
        h.extend_from_slice(b"fmt ");
        h.extend_from_slice(&(if float { 18_u32 } else { 16 }).to_le_bytes());
        h.extend_from_slice(&spec.format.tag().to_le_bytes());
        h.extend_from_slice(&channels.to_le_bytes());
        h.extend_from_slice(&spec.sample_rate.to_le_bytes());
        h.extend_from_slice(&(spec.sample_rate * u32::from(block_align)).to_le_bytes());
        h.extend_from_slice(&block_align.to_le_bytes());
        h.extend_from_slice(&bits.to_le_bytes());
        let mut fact_at = None;
        if float {
            h.extend_from_slice(&0_u16.to_le_bytes());
            fact_at = Some(h.len() as u64 + 8);
            h.extend_from_slice(b"fact");
            h.extend_from_slice(&4_u32.to_le_bytes());
            h.extend_from_slice(&0_u32.to_le_bytes());
        }
        let data_size_at = h.len() as u64 + 4;
        h.extend_from_slice(b"data\0\0\0\0");
        inner.write_all(&h)?;
        Ok(Self {
            inner,
            spec,
            base,
            fact_at,
            data_size_at,
            data_start: h.len() as u64,
            data_bytes: 0,
            frames: 0,
            rf64_threshold: u64::from(u32::MAX),
            scratch: vec![0; SCRATCH_BYTES].into_boxed_slice(),
        })
    }

    /// The file's spec.
    pub fn spec(&self) -> Spec {
        self.spec
    }

    /// Frames written so far.
    pub fn frames(&self) -> u64 {
        self.frames
    }

    /// Appends one slice of samples per channel, left first, all the same length.
    ///
    /// # Errors
    ///
    /// [`Error::ChannelCountMismatch`], [`Error::ChannelLengthMismatch`], or [`Error::Io`].
    pub fn write(&mut self, channels: &[&[f32]]) -> Result<(), Error> {
        let count = self.spec.channels.count();
        if channels.len() != count {
            return Err(Error::ChannelCountMismatch {
                expected: count,
                found: channels.len(),
            });
        }
        let frames = channels.first().map_or(0, |c| c.len());
        if channels.iter().any(|c| c.len() != frames) {
            return Err(Error::ChannelLengthMismatch);
        }
        let format = self.spec.format;
        let sample_bytes = format.bytes();
        let block_align = self.spec.block_align();
        let per_chunk = self.scratch.len() / block_align;
        let mut done = 0;
        while done < frames {
            let n = (frames - done).min(per_chunk);
            let bytes = &mut self.scratch[..n * block_align];
            for (f, frame) in bytes.chunks_exact_mut(block_align).enumerate() {
                for (channel, out) in channels.iter().zip(frame.chunks_exact_mut(sample_bytes)) {
                    format.encode(channel[done + f], out);
                }
            }
            self.inner.write_all(bytes)?;
            done += n;
            self.data_bytes += (n * block_align) as u64;
            self.frames += n as u64;
        }
        Ok(())
    }

    /// Writes the sizes into the header, as RF64 if the file is over 4 GB, flushes, and returns
    /// the underlying writer positioned at the end of the file.
    ///
    /// # Errors
    ///
    /// [`Error::Io`] if writing, seeking or flushing fails.
    pub fn finish(mut self) -> Result<W, Error> {
        let pad = self.data_bytes & 1;
        if pad == 1 {
            self.inner.write_all(&[0])?;
        }
        let end = self.data_start + self.data_bytes + pad;
        let riff_size = end - 8;
        if riff_size > self.rf64_threshold {
            self.put(0, b"RF64")?;
            self.put(4, &SIZE_IN_DS64.to_le_bytes())?;
            let mut ds64 = Vec::with_capacity(36);
            ds64.extend_from_slice(b"ds64");
            ds64.extend_from_slice(&DS64_BODY.to_le_bytes());
            ds64.extend_from_slice(&riff_size.to_le_bytes());
            ds64.extend_from_slice(&self.data_bytes.to_le_bytes());
            ds64.extend_from_slice(&self.frames.to_le_bytes());
            ds64.extend_from_slice(&0_u32.to_le_bytes());
            self.put(RESERVE_AT, &ds64)?;
            self.put(self.data_size_at, &SIZE_IN_DS64.to_le_bytes())?;
            if let Some(at) = self.fact_at {
                let frames = u32::try_from(self.frames).unwrap_or(SIZE_IN_DS64);
                self.put(at, &frames.to_le_bytes())?;
            }
        } else {
            // All three are at most `riff_size`, which fits in 32 bits here.
            self.put(4, &(riff_size as u32).to_le_bytes())?;
            self.put(self.data_size_at, &(self.data_bytes as u32).to_le_bytes())?;
            if let Some(at) = self.fact_at {
                self.put(at, &(self.frames as u32).to_le_bytes())?;
            }
        }
        self.inner.seek(SeekFrom::Start(self.base + end))?;
        self.inner.flush()?;
        Ok(self.inner)
    }

    fn put(&mut self, at: u64, bytes: &[u8]) -> Result<(), Error> {
        self.inner.seek(SeekFrom::Start(self.base + at))?;
        self.inner.write_all(bytes)?;
        Ok(())
    }

    /// Lowers the RF64 threshold, so tests can cross it without gigabytes of data.
    #[cfg(test)]
    pub(crate) fn set_rf64_threshold(&mut self, bytes: u64) {
        self.rf64_threshold = bytes;
    }

    /// Advances past `frames` frames without writing them, leaving a hole the sink fills with
    /// zeros, so tests can make files larger than 4 GB.
    #[cfg(test)]
    pub(crate) fn skip_frames(&mut self, frames: u64) -> Result<(), Error> {
        let bytes = frames * self.spec.block_align() as u64;
        self.inner.seek(SeekFrom::Current(
            i64::try_from(bytes).map_err(|_| Error::SizeOverflow)?,
        ))?;
        self.data_bytes += bytes;
        self.frames += frames;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::Channels;
    use crate::reader::WavReader;
    use std::collections::HashMap;
    use std::error::Error as StdError;
    use std::io::{self, Cursor, Read};

    const PAGE: u64 = 4096;

    /// An in-memory file that stores only the pages written to; holes read as zeros.
    #[derive(Default)]
    struct Sparse {
        pages: HashMap<u64, Box<[u8; PAGE as usize]>>,
        len: u64,
        pos: u64,
    }

    impl Write for Sparse {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            for (i, &b) in buf.iter().enumerate() {
                let at = self.pos + i as u64;
                let page = self
                    .pages
                    .entry(at / PAGE)
                    .or_insert_with(|| Box::new([0; PAGE as usize]));
                page[(at % PAGE) as usize] = b;
            }
            self.pos += buf.len() as u64;
            self.len = self.len.max(self.pos);
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl Read for Sparse {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            let n = buf
                .len()
                .min(usize::try_from(self.len.saturating_sub(self.pos)).unwrap_or(usize::MAX));
            for (i, b) in buf[..n].iter_mut().enumerate() {
                let at = self.pos + i as u64;
                *b = self
                    .pages
                    .get(&(at / PAGE))
                    .map_or(0, |p| p[(at % PAGE) as usize]);
            }
            self.pos += n as u64;
            Ok(n)
        }
    }

    impl Seek for Sparse {
        fn seek(&mut self, to: SeekFrom) -> io::Result<u64> {
            let target = match to {
                SeekFrom::Start(p) => Some(p),
                SeekFrom::End(d) => self.len.checked_add_signed(d),
                SeekFrom::Current(d) => self.pos.checked_add_signed(d),
            };
            self.pos = target.ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "seek"))?;
            Ok(self.pos)
        }
    }

    fn mono_f32() -> Spec {
        Spec {
            channels: Channels::Mono,
            sample_rate: 48_000,
            format: SampleFormat::Float32,
        }
    }

    #[test]
    fn test_finish_switches_to_rf64_past_threshold() -> Result<(), Box<dyn StdError>> {
        let samples: Vec<f32> = (0..1000).map(|i| i as f32 / 1000.0).collect();
        let mut writer = WavWriter::new(Cursor::new(Vec::new()), mono_f32())?;
        writer.set_rf64_threshold(1024);
        writer.write(&[&samples])?;
        let bytes = writer.finish()?.into_inner();
        assert_eq!(&bytes[0..4], b"RF64");
        assert_eq!(&bytes[12..16], b"ds64");

        let mut reader = WavReader::new(Cursor::new(bytes))?;
        assert_eq!(reader.frames(), 1000);
        assert!(!reader.truncated());
        let mut out = vec![0.0; 1000];
        assert_eq!(reader.read(&mut [&mut out])?, 1000);
        assert_eq!(out, samples);
        Ok(())
    }

    #[test]
    fn test_finish_below_threshold_stays_riff() -> Result<(), Box<dyn StdError>> {
        let mut writer = WavWriter::new(Cursor::new(Vec::new()), mono_f32())?;
        writer.set_rf64_threshold(1024);
        writer.write(&[&[0.5; 100]])?;
        let bytes = writer.finish()?.into_inner();
        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(&bytes[12..16], b"JUNK");
        Ok(())
    }

    /// A real file over 4 GB, held sparsely: the header crosses the real threshold.
    #[test]
    fn test_finish_over_4_gb_writes_rf64() -> Result<(), Box<dyn StdError>> {
        let spec = Spec {
            channels: Channels::Stereo,
            sample_rate: 96_000,
            format: SampleFormat::Int24,
        };
        let mut writer = WavWriter::new(Sparse::default(), spec)?;
        writer.write(&[&[0.5, -0.5], &[0.25, -0.25]])?;
        // 6 bytes per frame: just over 4 GiB of audio, and an odd frame count.
        let skipped = (1_u64 << 32) / 6 + 1;
        writer.skip_frames(skipped)?;
        writer.write(&[&[0.75], &[-0.75]])?;
        let mut file = writer.finish()?;
        assert!(file.len > u64::from(u32::MAX));
        file.seek(SeekFrom::Start(0))?;

        let mut reader = WavReader::new(file)?;
        let total = 2 + skipped + 1;
        assert_eq!(reader.frames(), total);
        assert_eq!(reader.spec(), spec);
        let (mut left, mut right) = ([0.0; 2], [0.0; 2]);
        assert_eq!(reader.read(&mut [&mut left, &mut right])?, 2);
        assert_eq!((left, right), ([0.5, -0.5], [0.25, -0.25]));
        reader.seek(total - 2)?;
        assert_eq!(reader.read(&mut [&mut left, &mut right])?, 2);
        assert_eq!((left, right), ([0.0, 0.75], [0.0, -0.75]));
        Ok(())
    }

    #[test]
    fn test_new_rejects_unwritable_formats() {
        for format in [SampleFormat::Int32, SampleFormat::Float64] {
            let spec = Spec {
                format,
                ..mono_f32()
            };
            assert!(matches!(
                WavWriter::new(Cursor::new(Vec::new()), spec),
                Err(Error::UnsupportedWriteFormat(_))
            ));
        }
        let spec = Spec {
            sample_rate: 0,
            ..mono_f32()
        };
        assert!(WavWriter::new(Cursor::new(Vec::new()), spec).is_err());
    }
}
