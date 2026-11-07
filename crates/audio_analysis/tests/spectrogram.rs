//! Spectrogram frames and the cached overview against known tones, silence, and files that must
//! be rebuilt.

use std::error::Error;
use std::f64::consts::PI;
use std::io::Cursor;

use audio_analysis::{
    BINS, Error as AnalysisError, FLOOR_DB, FRAME_SIZE, HOP, OVERVIEW_COLUMN, OverviewReader,
    OverviewWriter, Spectrogram, overview_db,
};

/// `n` samples of a sine of `amplitude` centred on frequency bin `bin`.
fn tone(n: usize, bin: usize, amplitude: f64) -> Vec<f32> {
    (0..n)
        .map(|i| (amplitude * (2.0 * PI * (bin * i) as f64 / FRAME_SIZE as f64).sin()) as f32)
        .collect()
}

/// The index of the loudest value.
fn peak(levels: &[f32]) -> usize {
    levels
        .iter()
        .enumerate()
        .fold((0, f32::NEG_INFINITY), |best, (i, &l)| {
            if l > best.1 { (i, l) } else { best }
        })
        .0
}

/// Builds the overview of `samples`, pushed in chunks of `chunk`.
fn overview(samples: &[f32], chunk: usize) -> Result<Vec<u8>, Box<dyn Error>> {
    let mut writer = OverviewWriter::new(Vec::new(), samples.len() as u64)?;
    for part in samples.chunks(chunk) {
        writer.push(part)?;
    }
    Ok(writer.finish()?)
}

#[test]
fn test_spectrogram_frame_db_reads_tone_level_at_its_bin() -> Result<(), Box<dyn Error>> {
    let mut spectrogram = Spectrogram::new()?;
    let mut out = vec![0.0; BINS];
    for (bin, amplitude, db) in [(100, 1.0, 0.0), (37, 0.5, -6.02), (900, 0.01, -40.0)] {
        spectrogram.frame_db(&tone(FRAME_SIZE, bin, amplitude), &mut out)?;
        assert_eq!(peak(&out), bin);
        assert!((out[bin] - db).abs() < 0.05, "bin {bin}: {} dB", out[bin]);
        // Hann leakage stays within the neighbouring bins.
        assert!(out[bin + 5] < db - 60.0);
    }
    Ok(())
}

#[test]
fn test_spectrogram_frame_db_floors_silence_and_zero_pads() -> Result<(), Box<dyn Error>> {
    let mut spectrogram = Spectrogram::new()?;
    let mut out = vec![0.0; BINS];
    spectrogram.frame_db(&[], &mut out)?;
    assert!(out.iter().all(|&l| l == FLOOR_DB));
    // Half a frame of tone, zero-padded, still peaks at its bin, 6 dB down.
    spectrogram.frame_db(&tone(FRAME_SIZE / 2, 200, 1.0), &mut out)?;
    assert_eq!(peak(&out), 200);
    assert!((out[200] + 6.02).abs() < 0.5, "{} dB", out[200]);
    Ok(())
}

#[test]
fn test_spectrogram_frame_db_rejects_wrong_buffers() -> Result<(), Box<dyn Error>> {
    let mut spectrogram = Spectrogram::new()?;
    let mut out = vec![0.0; BINS];
    assert!(matches!(
        spectrogram.frame_db(&[0.0; FRAME_SIZE + 1], &mut out),
        Err(AnalysisError::BufferLength { .. })
    ));
    assert!(matches!(
        spectrogram.frame_db(&[0.0; FRAME_SIZE], &mut out[1..]),
        Err(AnalysisError::BufferLength { .. })
    ));
    Ok(())
}

#[test]
fn test_overview_columns_follow_tones() -> Result<(), Box<dyn Error>> {
    // Column 0 a full-scale tone at bin 64, column 1 a half-scale tone at bin 300, then a short
    // silent tail that makes a partial third column.
    let column = OVERVIEW_COLUMN as usize;
    let mut samples = tone(column, 64, 1.0);
    samples.extend(tone(column, 300, 0.5));
    samples.extend(vec![0.0; 1000]);
    for chunk in [1, 777, samples.len()] {
        let mut reader = OverviewReader::new(Cursor::new(overview(&samples, chunk)?))?;
        assert_eq!(reader.samples(), samples.len() as u64);
        assert_eq!(reader.columns(), 3);
        let mut out = vec![0; 4 * BINS];
        assert_eq!(reader.read_columns(0, &mut out)?, 3);
        let levels: Vec<Vec<f32>> = out
            .chunks(BINS)
            .take(3)
            .map(|c| c.iter().map(|&b| overview_db(b)).collect())
            .collect();
        // Frames straddling the boundary mix the two tones; the column means stay dominated
        // by each column's own tone.
        assert_eq!(peak(&levels[0]), 64);
        assert!(levels[0][64] > -1.0, "{} dB", levels[0][64]);
        assert_eq!(peak(&levels[1]), 300);
        assert!((levels[1][300] + 6.0).abs() < 1.0, "{} dB", levels[1][300]);
        // The tail's frames start after the tone ends and see only silence.
        assert!(levels[2].iter().all(|&l| l == -127.5));
    }
    Ok(())
}

#[test]
fn test_overview_frame_count_matches_hop() -> Result<(), Box<dyn Error>> {
    // One sample past a column boundary adds one column of one frame.
    let samples = vec![0.0; OVERVIEW_COLUMN as usize + 1];
    let reader = OverviewReader::new(Cursor::new(overview(&samples, HOP)?))?;
    assert_eq!(reader.columns(), 2);
    let empty = OverviewReader::new(Cursor::new(overview(&[], 1)?))?;
    assert_eq!(empty.columns(), 0);
    Ok(())
}

#[test]
fn test_overview_reader_rejects_files_to_rebuild() -> Result<(), Box<dyn Error>> {
    let good = overview(&[0.0; 20_000], 20_000)?;
    let open = |bytes: Vec<u8>| OverviewReader::new(Cursor::new(bytes)).map(|_| ());
    assert!(matches!(
        open(b"DBW".to_vec()),
        Err(AnalysisError::WrongMagic(_))
    ));
    let mut bad = good.clone();
    bad[3] = b'X';
    assert!(matches!(open(bad), Err(AnalysisError::WrongMagic(_))));
    let mut bad = good.clone();
    bad[4] = 9;
    assert!(matches!(
        open(bad),
        Err(AnalysisError::UnsupportedVersion(9))
    ));
    let mut bad = good.clone();
    bad[13] = 0;
    assert!(matches!(open(bad), Err(AnalysisError::ParameterMismatch)));
    let mut bad = good;
    bad.push(0);
    assert!(matches!(open(bad), Err(AnalysisError::WrongLength { .. })));
    Ok(())
}

#[test]
fn test_overview_read_columns_rejects_partial_columns() -> Result<(), Box<dyn Error>> {
    let mut reader = OverviewReader::new(Cursor::new(overview(&[0.0; 100], 100)?))?;
    let mut out = vec![0; BINS + 1];
    assert!(matches!(
        reader.read_columns(0, &mut out),
        Err(AnalysisError::BufferLength { .. })
    ));
    Ok(())
}
