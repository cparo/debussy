//! A three-hour file summarised in memory that does not grow with its length: once made, the
//! writers allocate nothing, which the real-time guard checks. The summary's coarse levels are
//! checked against brute force.

use std::error::Error;
use std::io::{Cursor, Seek, SeekFrom, Write};

use realtime_guard::{GuardAllocator, assert_realtime};

use audio_analysis::{
    BASE_BUCKET, Bucket, LEVEL_FACTOR, OverviewWriter, WaveformReader, WaveformWriter,
};

#[global_allocator]
static ALLOCATOR: GuardAllocator = GuardAllocator;

/// A file that keeps only the bytes written at or after `keep_from`, preallocated so writing to it
/// allocates nothing.
struct Tail {
    position: u64,
    keep_from: u64,
    kept: Vec<u8>,
}

impl Write for Tail {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        for (i, &byte) in buf.iter().enumerate() {
            let at = self.position + i as u64;
            if at >= self.keep_from {
                let index = (at - self.keep_from) as usize;
                if let Some(slot) = self.kept.get_mut(index) {
                    *slot = byte;
                }
            }
        }
        self.position += buf.len() as u64;
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Seek for Tail {
    fn seek(&mut self, to: SeekFrom) -> std::io::Result<u64> {
        if let SeekFrom::Start(position) = to {
            self.position = position;
        }
        Ok(self.position)
    }
}

/// A cheap, reproducible, non-repeating test signal.
struct Signal {
    state: u32,
}

impl Signal {
    fn next(&mut self) -> f32 {
        self.state ^= self.state << 13;
        self.state ^= self.state >> 17;
        self.state ^= self.state << 5;
        self.state as f32 / u32::MAX as f32 * 2.0 - 1.0
    }
}

/// Brute-force minimum, maximum and sum of squares of one bucket.
#[derive(Clone, Copy)]
struct Brute {
    min: f32,
    max: f32,
    sum_squares: f64,
    count: u64,
}

const EMPTY: Brute = Brute {
    min: f32::INFINITY,
    max: f32::NEG_INFINITY,
    sum_squares: 0.0,
    count: 0,
};

#[test]
fn test_waveform_writer_summarises_three_hours_in_bounded_memory() -> Result<(), Box<dyn Error>> {
    const SAMPLES: u64 = 3 * 60 * 60 * 48_000;
    const CHUNK: usize = 4096;
    /// The first level checked by brute force, with 65 536-sample buckets.
    const CHECKED: usize = 5;

    let sizes: Vec<u64> = (0..)
        .map(|l| BASE_BUCKET * LEVEL_FACTOR.pow(l))
        .take_while(|&size| size < SAMPLES * LEVEL_FACTOR)
        .collect();
    let buckets: Vec<u64> = sizes.iter().map(|&s| SAMPLES.div_ceil(s)).collect();
    let keep_from = 24 + buckets[..CHECKED].iter().sum::<u64>() * 12;
    let kept_len = buckets[CHECKED..].iter().sum::<u64>() * 12;
    let tail = Tail {
        position: 0,
        keep_from,
        kept: vec![0; kept_len as usize],
    };
    let mut brute: Vec<Vec<Brute>> = buckets[CHECKED..]
        .iter()
        .map(|&n| vec![EMPTY; n as usize])
        .collect();
    let mut chunk = vec![0.0; CHUNK];
    let mut signal = Signal { state: 0x1234_5678 };

    let mut writer = WaveformWriter::new(tail, SAMPLES)?;
    assert_realtime(|| -> Result<(), audio_analysis::Error> {
        let mut position = 0u64;
        while position < SAMPLES {
            let n = CHUNK.min((SAMPLES - position) as usize);
            for (i, sample) in chunk[..n].iter_mut().enumerate() {
                *sample = signal.next();
                for (level, buckets) in brute.iter_mut().enumerate() {
                    let at = (position + i as u64) / sizes[CHECKED + level];
                    let b = &mut buckets[at as usize];
                    b.min = b.min.min(*sample);
                    b.max = b.max.max(*sample);
                    b.sum_squares += f64::from(*sample) * f64::from(*sample);
                    b.count += 1;
                }
            }
            writer.push(&chunk[..n])?;
            position += n as u64;
        }
        Ok(())
    })?;
    // Finishing frees the writer's buffers, so it runs outside the guard.
    let tail = writer.finish()?;

    // Rebuild a readable file: a header and zeroed finer levels, then the kept tail.
    let mut file = Vec::with_capacity((keep_from + kept_len) as usize);
    file.extend_from_slice(b"DBWF");
    file.extend_from_slice(&1u32.to_le_bytes());
    file.extend_from_slice(&64u32.to_le_bytes());
    file.extend_from_slice(&4u32.to_le_bytes());
    file.extend_from_slice(&SAMPLES.to_le_bytes());
    file.resize(keep_from as usize, 0);
    file.extend_from_slice(&tail.kept);
    let mut reader = WaveformReader::new(Cursor::new(file))?;
    assert_eq!(reader.levels(), sizes.len());
    for (offset, want) in brute.iter().enumerate() {
        let level = CHECKED + offset;
        let mut got = vec![Bucket::of(&[0.0]).ok_or("bucket")?; want.len()];
        assert_eq!(reader.read_buckets(level, 0, &mut got)?, want.len());
        for (g, w) in got.iter().zip(want) {
            assert_eq!(g.min, w.min);
            assert_eq!(g.max, w.max);
            let rms = (w.sum_squares / w.count as f64).sqrt() as f32;
            assert!((g.rms - rms).abs() <= 1e-5 * rms, "rms {} != {rms}", g.rms);
        }
    }
    Ok(())
}

#[test]
fn test_overview_writer_allocates_nothing_once_made() -> Result<(), Box<dyn Error>> {
    let chunk = vec![0.25f32; 4096];
    let mut writer = OverviewWriter::new(std::io::sink(), 256 * 4096)?;
    assert_realtime(|| -> Result<(), audio_analysis::Error> {
        for _ in 0..256 {
            writer.push(&chunk)?;
        }
        Ok(())
    })?;
    writer.finish()?;
    Ok(())
}
