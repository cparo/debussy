//! The waveform summary: a pyramid of levels of [`Bucket`]s, built by [`WaveformWriter`] into a
//! cache file and read back by [`WaveformReader`] (`docs/architecture/analysis.md` "Waveform
//! summaries", "Analysis cache files").
//!
//! The file is the header (magic `DBWF`, version, [`BASE_BUCKET`], [`LEVEL_FACTOR`], sample count)
//! followed by every level, finest first, each bucket as minimum, maximum and RMS in `f32`. Every
//! level's length follows from the sample count, so the writer can put each bucket in its place
//! as soon as it is complete, and the reader can seek straight to any bucket.

use std::io::{Read, Seek, SeekFrom, Write};

use crate::error::Error;
use crate::header::{read_start, read_u32, read_u64, write_u32, write_u64};

/// Samples per bucket in level 0.
pub const BASE_BUCKET: u64 = 64;

/// How many times more samples each level's buckets cover than the level below.
pub const LEVEL_FACTOR: u64 = 4;

/// The file's magic.
const MAGIC: [u8; 4] = *b"DBWF";
/// The format version this build reads and writes.
const VERSION: u32 = 1;
/// Magic, version, base, factor, sample count.
const HEADER_LEN: u64 = 4 + 4 + 4 + 4 + 8;
/// Minimum, maximum and RMS as `f32`.
const BUCKET_LEN: u64 = 12;
/// Buckets each level's writer holds before writing them out.
const WRITE_BATCH: usize = 1024;
/// Buckets read at a time when merging buckets into a column.
const READ_BATCH: usize = 8;

/// The minimum, maximum and RMS of a run of samples.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bucket {
    /// The lowest sample.
    pub min: f32,
    /// The highest sample.
    pub max: f32,
    /// The root mean square of the samples.
    pub rms: f32,
}

impl Bucket {
    /// The bucket of `samples`, or `None` if there are none. For drawing views zoomed in past
    /// [`BASE_BUCKET`] samples per column, from the audio itself.
    pub fn of(samples: &[f32]) -> Option<Bucket> {
        let mut acc = Accumulator::EMPTY;
        for &s in samples {
            acc.add(s);
        }
        acc.bucket()
    }

    /// Reads a bucket from its 12 bytes.
    fn from_bytes(bytes: &[u8; BUCKET_LEN as usize]) -> Bucket {
        let f = |i: usize| f32::from_le_bytes([bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]);
        Bucket {
            min: f(0),
            max: f(4),
            rms: f(8),
        }
    }

    /// The bucket's 12 bytes.
    fn to_bytes(self) -> [u8; BUCKET_LEN as usize] {
        let mut bytes = [0; BUCKET_LEN as usize];
        bytes[0..4].copy_from_slice(&self.min.to_le_bytes());
        bytes[4..8].copy_from_slice(&self.max.to_le_bytes());
        bytes[8..12].copy_from_slice(&self.rms.to_le_bytes());
        bytes
    }
}

/// A bucket being built: running minimum, maximum and sum of squares.
#[derive(Debug, Clone, Copy)]
struct Accumulator {
    min: f32,
    max: f32,
    sum_squares: f64,
    count: u64,
}

impl Accumulator {
    /// No samples yet.
    const EMPTY: Accumulator = Accumulator {
        min: f32::INFINITY,
        max: f32::NEG_INFINITY,
        sum_squares: 0.0,
        count: 0,
    };

    /// Adds one sample.
    fn add(&mut self, sample: f32) {
        self.min = self.min.min(sample);
        self.max = self.max.max(sample);
        self.sum_squares += f64::from(sample) * f64::from(sample);
        self.count += 1;
    }

    /// Adds every sample of `other`.
    fn merge(&mut self, other: &Accumulator) {
        self.min = self.min.min(other.min);
        self.max = self.max.max(other.max);
        self.sum_squares += other.sum_squares;
        self.count += other.count;
    }

    /// Adds a stored bucket that covered `count` samples.
    fn merge_bucket(&mut self, bucket: Bucket, count: u64) {
        let rms = f64::from(bucket.rms);
        self.merge(&Accumulator {
            min: bucket.min,
            max: bucket.max,
            sum_squares: rms * rms * count as f64,
            count,
        });
    }

    /// The finished bucket, if any samples were added.
    fn bucket(&self) -> Option<Bucket> {
        (self.count > 0).then(|| Bucket {
            min: self.min,
            max: self.max,
            rms: (self.sum_squares / self.count as f64).sqrt() as f32,
        })
    }
}

/// The samples per bucket of `level`.
fn bucket_size(level: usize) -> u64 {
    // Levels stop once one bucket covers the whole file, so this cannot overflow for a level
    // that exists; saturate rather than panic for one that does not.
    LEVEL_FACTOR
        .checked_pow(level as u32)
        .and_then(|f| f.checked_mul(BASE_BUCKET))
        .unwrap_or(u64::MAX)
}

/// The number of buckets in each level of a summary of `samples` samples: none for no samples,
/// otherwise up to and including the first level with one bucket.
fn level_buckets(samples: u64) -> Vec<u64> {
    let mut levels = Vec::new();
    if samples == 0 {
        return levels;
    }
    loop {
        let buckets = samples.div_ceil(bucket_size(levels.len()));
        levels.push(buckets);
        if buckets <= 1 {
            return levels;
        }
    }
}

/// The byte offset of each level's first bucket.
fn level_offsets(buckets: &[u64]) -> Vec<u64> {
    let mut offset = HEADER_LEN;
    buckets
        .iter()
        .map(|&n| {
            let start = offset;
            offset += n * BUCKET_LEN;
            start
        })
        .collect()
}

/// One level of a summary being written.
struct LevelWriter {
    /// The bucket being filled.
    current: Accumulator,
    /// Complete buckets' bytes not yet written.
    pending: Vec<u8>,
    /// Where `pending`'s first byte goes in the file.
    offset: u64,
}

/// Builds a waveform summary from one pass over the samples, writing its cache file as it goes.
/// Memory use is fixed by the number of levels, not the file's length.
pub struct WaveformWriter<W: Write + Seek> {
    inner: W,
    samples: u64,
    pushed: u64,
    levels: Vec<LevelWriter>,
}

impl<W: Write + Seek> WaveformWriter<W> {
    /// Starts a summary of `samples` samples, writing its header to `inner` at its start.
    ///
    /// # Errors
    ///
    /// [`Error::Io`].
    pub fn new(mut inner: W, samples: u64) -> Result<Self, Error> {
        let buckets = level_buckets(samples);
        let levels = level_offsets(&buckets)
            .into_iter()
            .map(|offset| LevelWriter {
                current: Accumulator::EMPTY,
                pending: Vec::with_capacity(WRITE_BATCH * BUCKET_LEN as usize),
                offset,
            })
            .collect();
        inner.seek(SeekFrom::Start(0))?;
        inner.write_all(&MAGIC)?;
        write_u32(&mut inner, VERSION)?;
        write_u32(&mut inner, BASE_BUCKET as u32)?;
        write_u32(&mut inner, LEVEL_FACTOR as u32)?;
        write_u64(&mut inner, samples)?;
        Ok(WaveformWriter {
            inner,
            samples,
            pushed: 0,
            levels,
        })
    }

    /// Adds the next `samples`.
    ///
    /// # Errors
    ///
    /// [`Error::SampleCount`] if this goes past the declared count; [`Error::Io`].
    pub fn push(&mut self, samples: &[f32]) -> Result<(), Error> {
        let total = self.pushed + samples.len() as u64;
        if total > self.samples {
            return Err(Error::SampleCount {
                expected: self.samples,
                found: total,
            });
        }
        for &sample in samples {
            self.levels[0].current.add(sample);
            if self.levels[0].current.count == BASE_BUCKET {
                self.complete(0)?;
            }
        }
        self.pushed = total;
        Ok(())
    }

    /// Writes out the last, partial buckets and returns the writer.
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
        for level in 0..self.levels.len() {
            if self.levels[level].current.count > 0 {
                self.complete(level)?;
            }
        }
        for level in 0..self.levels.len() {
            self.flush(level)?;
        }
        self.inner.flush()?;
        Ok(self.inner)
    }

    /// Stores `level`'s current bucket and adds it to the level above, completing that one in
    /// turn if it is full.
    fn complete(&mut self, mut level: usize) -> Result<(), Error> {
        loop {
            let acc = std::mem::replace(&mut self.levels[level].current, Accumulator::EMPTY);
            if let Some(bucket) = acc.bucket() {
                self.levels[level]
                    .pending
                    .extend_from_slice(&bucket.to_bytes());
                if self.levels[level].pending.len() >= WRITE_BATCH * BUCKET_LEN as usize {
                    self.flush(level)?;
                }
            }
            let Some(above) = self.levels.get_mut(level + 1) else {
                return Ok(());
            };
            above.current.merge(&acc);
            if above.current.count < bucket_size(level + 1) {
                return Ok(());
            }
            level += 1;
        }
    }

    /// Writes `level`'s pending buckets in place.
    fn flush(&mut self, level: usize) -> Result<(), Error> {
        let writer = &mut self.levels[level];
        if writer.pending.is_empty() {
            return Ok(());
        }
        self.inner.seek(SeekFrom::Start(writer.offset))?;
        self.inner.write_all(&writer.pending)?;
        writer.offset += writer.pending.len() as u64;
        writer.pending.clear();
        Ok(())
    }
}

/// Reads ranges of a waveform summary's cache file.
pub struct WaveformReader<R: Read + Seek> {
    inner: R,
    samples: u64,
    buckets: Vec<u64>,
    offsets: Vec<u64>,
}

impl<R: Read + Seek> WaveformReader<R> {
    /// Opens a summary, checking its header and length.
    ///
    /// # Errors
    ///
    /// [`Error::WrongMagic`], [`Error::UnsupportedVersion`], [`Error::ParameterMismatch`] or
    /// [`Error::WrongLength`] for a file to rebuild; [`Error::Io`].
    pub fn new(mut inner: R) -> Result<Self, Error> {
        let length = read_start(&mut inner, MAGIC, VERSION, "waveform", HEADER_LEN)?;
        let base = read_u32(&mut inner)?;
        let factor = read_u32(&mut inner)?;
        if u64::from(base) != BASE_BUCKET || u64::from(factor) != LEVEL_FACTOR {
            return Err(Error::ParameterMismatch);
        }
        let samples = read_u64(&mut inner)?;
        let buckets = level_buckets(samples);
        let offsets = level_offsets(&buckets);
        let expected = offsets
            .last()
            .zip(buckets.last())
            .map_or(HEADER_LEN, |(o, n)| o + n * BUCKET_LEN);
        if length != expected {
            return Err(Error::WrongLength {
                expected,
                found: length,
            });
        }
        Ok(WaveformReader {
            inner,
            samples,
            buckets,
            offsets,
        })
    }

    /// The number of samples summarised.
    pub fn samples(&self) -> u64 {
        self.samples
    }

    /// The number of levels; none for an empty file.
    pub fn levels(&self) -> usize {
        self.buckets.len()
    }

    /// The samples per bucket of `level`: [`BASE_BUCKET`] times [`LEVEL_FACTOR`] to the `level`.
    /// The last bucket of a level may cover fewer.
    pub fn bucket_size(&self, level: usize) -> u64 {
        bucket_size(level)
    }

    /// The number of buckets in `level`, or 0 for a level the summary does not have.
    pub fn buckets(&self, level: usize) -> u64 {
        self.buckets.get(level).copied().unwrap_or(0)
    }

    /// Reads `level`'s buckets from `first` into `out`, returning how many there were (fewer than
    /// `out.len()` at the end of the level).
    ///
    /// # Errors
    ///
    /// [`Error::LevelOutOfRange`]; [`Error::Io`].
    pub fn read_buckets(
        &mut self,
        level: usize,
        first: u64,
        out: &mut [Bucket],
    ) -> Result<usize, Error> {
        let (Some(&buckets), Some(&offset)) = (self.buckets.get(level), self.offsets.get(level))
        else {
            return Err(Error::LevelOutOfRange {
                level,
                levels: self.levels(),
            });
        };
        let count = buckets.saturating_sub(first).min(out.len() as u64) as usize;
        if count == 0 {
            return Ok(0);
        }
        self.inner
            .seek(SeekFrom::Start(offset + first * BUCKET_LEN))?;
        let mut bytes = [0; BUCKET_LEN as usize];
        for bucket in &mut out[..count] {
            self.inner.read_exact(&mut bytes)?;
            *bucket = Bucket::from_bytes(&bytes);
        }
        Ok(count)
    }

    /// Summarises consecutive columns of `samples_per_column` samples each, the first starting at
    /// sample `start`, into `out`; returns how many columns hold samples (fewer than `out.len()`
    /// at the end of the file). Each column merges the buckets of the coarsest level whose bucket
    /// size is at most `samples_per_column` that overlap it, so its edges are rounded out to that
    /// level's bucket edges.
    ///
    /// # Errors
    ///
    /// [`Error::TooZoomedIn`] for columns narrower than [`BASE_BUCKET`]; [`Error::Io`].
    pub fn columns(
        &mut self,
        start: u64,
        samples_per_column: u64,
        out: &mut [Bucket],
    ) -> Result<usize, Error> {
        if samples_per_column < BASE_BUCKET {
            return Err(Error::TooZoomedIn(samples_per_column));
        }
        let Some(level) = (0..self.levels())
            .rev()
            .find(|&l| bucket_size(l) <= samples_per_column)
        else {
            return Ok(0);
        };
        let size = bucket_size(level);
        let mut batch = [Bucket {
            min: 0.0,
            max: 0.0,
            rms: 0.0,
        }; READ_BATCH];
        for (index, column) in out.iter_mut().enumerate() {
            let low = (index as u64)
                .saturating_mul(samples_per_column)
                .saturating_add(start);
            if low >= self.samples {
                return Ok(index);
            }
            let high = low.saturating_add(samples_per_column).min(self.samples);
            let mut acc = Accumulator::EMPTY;
            let mut next = low / size;
            let end = high.div_ceil(size);
            while next < end {
                let wanted = ((end - next) as usize).min(READ_BATCH);
                let read = self.read_buckets(level, next, &mut batch[..wanted])?;
                for (i, &bucket) in batch[..read].iter().enumerate() {
                    let first_sample = (next + i as u64) * size;
                    let count = size.min(self.samples - first_sample);
                    acc.merge_bucket(bucket, count);
                }
                next += read as u64;
                if read < wanted {
                    break;
                }
            }
            if let Some(bucket) = acc.bucket() {
                *column = bucket;
            }
        }
        Ok(out.len())
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
    fn test_level_buckets_counts_up_to_one_bucket() {
        assert!(level_buckets(0).is_empty());
        assert_eq!(level_buckets(1), vec![1]);
        assert_eq!(level_buckets(64), vec![1]);
        assert_eq!(level_buckets(65), vec![2, 1]);
        assert_eq!(level_buckets(256), vec![4, 1]);
        assert_eq!(level_buckets(1000), vec![16, 4, 1]);
    }

    #[test]
    fn test_bucket_of_empty_is_none() {
        assert_eq!(Bucket::of(&[]), None);
    }

    #[test]
    fn test_bucket_bytes_round_trip() {
        let bucket = Bucket {
            min: -0.5,
            max: 1.25,
            rms: 0.75,
        };
        assert_eq!(Bucket::from_bytes(&bucket.to_bytes()), bucket);
    }
}
