//! Waveform summaries against brute-force minimum, maximum and RMS, column merging, and cache
//! files that must be rebuilt.

use std::error::Error;
use std::io::Cursor;

use audio_analysis::{
    BASE_BUCKET, Bucket, Error as AnalysisError, LEVEL_FACTOR, WaveformReader, WaveformWriter,
};

/// A reproducible test signal in roughly [-1, 1].
fn signal(n: usize, seed: u64) -> Vec<f32> {
    let mut noise = dsp::Noise::new(seed);
    (0..n).map(|_| 2.0 * noise.uniform()).collect()
}

/// Builds the summary of `samples`, pushed in chunks of `chunk`.
fn summarise(samples: &[f32], chunk: usize) -> Result<Vec<u8>, Box<dyn Error>> {
    let mut writer = WaveformWriter::new(Cursor::new(Vec::new()), samples.len() as u64)?;
    for part in samples.chunks(chunk) {
        writer.push(part)?;
    }
    Ok(writer.finish()?.into_inner())
}

fn assert_close(got: Bucket, want: Bucket) {
    assert_eq!(got.min, want.min);
    assert_eq!(got.max, want.max);
    assert!(
        (got.rms - want.rms).abs() <= 1e-5 * want.rms.max(1.0),
        "rms {} != {}",
        got.rms,
        want.rms
    );
}

#[test]
fn test_waveform_writer_matches_brute_force_at_every_level() -> Result<(), Box<dyn Error>> {
    // Not a multiple of any bucket size, so every level ends with a partial bucket.
    let samples = signal(64 * 4 * 4 * 4 * 5 + 37, 7);
    for chunk in [1, 100, 4096, samples.len()] {
        let mut reader = WaveformReader::new(Cursor::new(summarise(&samples, chunk)?))?;
        assert_eq!(reader.samples(), samples.len() as u64);
        assert_eq!(reader.levels(), 6);
        for level in 0..reader.levels() {
            let size = (BASE_BUCKET * LEVEL_FACTOR.pow(level as u32)) as usize;
            assert_eq!(reader.bucket_size(level), size as u64);
            let want: Vec<Bucket> = samples.chunks(size).filter_map(Bucket::of).collect();
            assert_eq!(reader.buckets(level), want.len() as u64);
            let mut got = vec![Bucket::of(&[0.0]).ok_or("bucket")?; want.len() + 3];
            assert_eq!(reader.read_buckets(level, 0, &mut got)?, want.len());
            for (g, w) in got.iter().zip(&want) {
                assert_close(*g, *w);
            }
        }
        assert_eq!(reader.buckets(reader.levels() - 1), 1);
    }
    Ok(())
}

#[test]
fn test_waveform_reader_read_buckets_from_middle_and_past_end() -> Result<(), Box<dyn Error>> {
    let samples = signal(1000, 3);
    let mut reader = WaveformReader::new(Cursor::new(summarise(&samples, 333)?))?;
    let mut out = [Bucket::of(&[0.0]).ok_or("bucket")?; 4];
    assert_eq!(reader.read_buckets(0, 14, &mut out)?, 2);
    assert_close(
        out[0],
        Bucket::of(&samples[14 * 64..15 * 64]).ok_or("bucket")?,
    );
    assert_close(out[1], Bucket::of(&samples[15 * 64..]).ok_or("bucket")?);
    assert_eq!(reader.read_buckets(0, 16, &mut out)?, 0);
    assert!(matches!(
        reader.read_buckets(3, 0, &mut out),
        Err(AnalysisError::LevelOutOfRange {
            level: 3,
            levels: 3
        })
    ));
    Ok(())
}

#[test]
fn test_waveform_reader_columns_merge_bucket_aligned_ranges() -> Result<(), Box<dyn Error>> {
    let samples = signal(10_000, 11);
    let mut reader = WaveformReader::new(Cursor::new(summarise(&samples, 512)?))?;
    // 300 samples per column reads level 1 (256-sample buckets); edges round out to them.
    let mut out = [Bucket::of(&[0.0]).ok_or("bucket")?; 40];
    let count = reader.columns(1000, 300, &mut out)?;
    assert_eq!(count, 30);
    for (index, column) in out[..count].iter().enumerate() {
        let low = 1000 + index * 300;
        let high = (low + 300).min(samples.len());
        let rounded = &samples[low / 256 * 256..(high.div_ceil(256) * 256).min(samples.len())];
        assert_close(*column, Bucket::of(rounded).ok_or("bucket")?);
    }
    Ok(())
}

#[test]
fn test_waveform_reader_columns_rejects_zoom_past_base() -> Result<(), Box<dyn Error>> {
    let mut reader = WaveformReader::new(Cursor::new(summarise(&signal(500, 1), 500)?))?;
    let mut out = [Bucket::of(&[0.0]).ok_or("bucket")?; 2];
    assert!(matches!(
        reader.columns(0, 63, &mut out),
        Err(AnalysisError::TooZoomedIn(63))
    ));
    Ok(())
}

#[test]
fn test_waveform_writer_empty_file() -> Result<(), Box<dyn Error>> {
    let mut reader = WaveformReader::new(Cursor::new(summarise(&[], 1)?))?;
    assert_eq!(reader.levels(), 0);
    let mut out = [Bucket::of(&[0.0]).ok_or("bucket")?; 2];
    assert_eq!(reader.columns(0, 64, &mut out)?, 0);
    Ok(())
}

#[test]
fn test_waveform_writer_rejects_wrong_sample_count() -> Result<(), Box<dyn Error>> {
    let mut writer = WaveformWriter::new(Cursor::new(Vec::new()), 10)?;
    assert!(matches!(
        writer.push(&[0.0; 11]),
        Err(AnalysisError::SampleCount {
            expected: 10,
            found: 11
        })
    ));
    writer.push(&[0.0; 9])?;
    assert!(matches!(
        writer.finish(),
        Err(AnalysisError::SampleCount {
            expected: 10,
            found: 9
        })
    ));
    Ok(())
}

#[test]
fn test_waveform_reader_rejects_files_to_rebuild() -> Result<(), Box<dyn Error>> {
    let good = summarise(&signal(1000, 5), 1000)?;
    let open = |bytes: Vec<u8>| WaveformReader::new(Cursor::new(bytes)).map(|_| ());

    assert!(matches!(
        open(Vec::new()),
        Err(AnalysisError::WrongMagic(_))
    ));
    let mut bad = good.clone();
    bad[0] = b'X';
    assert!(matches!(open(bad), Err(AnalysisError::WrongMagic(_))));
    let mut bad = good.clone();
    bad[4] = 2;
    assert!(matches!(
        open(bad),
        Err(AnalysisError::UnsupportedVersion(2))
    ));
    let mut bad = good.clone();
    bad[8] = 128;
    assert!(matches!(open(bad), Err(AnalysisError::ParameterMismatch)));
    let mut bad = good.clone();
    bad.pop();
    assert!(matches!(open(bad), Err(AnalysisError::WrongLength { .. })));
    let mut bad = good;
    bad[17] += 1;
    assert!(matches!(open(bad), Err(AnalysisError::WrongLength { .. })));
    Ok(())
}
