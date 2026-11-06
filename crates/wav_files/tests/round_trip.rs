//! Every written format, read back through streaming reads of frame ranges.

use std::error::Error;
use std::io::{Cursor, Seek, SeekFrom, Write};

use wav_files::{Channels, SampleFormat, Spec, WavReader, WavWriter};

/// More frames than one 64 KiB scratch buffer holds, and an odd count (so 24-bit mono pads).
const FRAMES: usize = 20_001;

/// Samples exactly representable in 16 bits, so every format round-trips them exactly.
fn signal(channel: usize) -> Vec<f32> {
    (0..FRAMES)
        .map(|i| {
            let v = ((i * 7 + channel * 1000) % 65_536) as i32 - 32_768;
            v as f32 / 32_768.0
        })
        .collect()
}

fn round_trip(spec: Spec) -> Result<(), Box<dyn Error>> {
    let channels: Vec<Vec<f32>> = (0..spec.channels.count()).map(signal).collect();
    let views: Vec<&[f32]> = channels.iter().map(Vec::as_slice).collect();
    let mut writer = WavWriter::new(Cursor::new(Vec::new()), spec)?;
    // Write in uneven pieces.
    let mut at = 0;
    for piece in [1, 999, 10_000, FRAMES - 11_000] {
        let slices: Vec<&[f32]> = views.iter().map(|c| &c[at..at + piece]).collect();
        writer.write(&slices)?;
        at += piece;
    }
    assert_eq!(writer.frames(), FRAMES as u64);
    let bytes = writer.finish()?.into_inner();
    assert_eq!(bytes.len() % 2, 0, "RIFF files have even length");

    let mut reader = WavReader::new(Cursor::new(bytes))?;
    assert_eq!(reader.spec(), spec);
    assert_eq!(reader.frames(), FRAMES as u64);
    assert!(!reader.truncated());

    // Read the whole file in ranges of 777 frames.
    let mut got: Vec<Vec<f32>> = vec![vec![0.0; FRAMES]; channels.len()];
    let mut at = 0;
    loop {
        let mut slices: Vec<&mut [f32]> = got
            .iter_mut()
            .map(|c| {
                let end = (at + 777).min(FRAMES);
                &mut c[at..end]
            })
            .collect();
        let n = reader.read(&mut slices)?;
        if n == 0 {
            break;
        }
        at += n;
    }
    assert_eq!(at, FRAMES);
    assert_eq!(got, channels, "{spec:?}");

    // Seek into the middle and read a range.
    reader.seek(12_345)?;
    let mut range: Vec<Vec<f32>> = vec![vec![0.0; 100]; channels.len()];
    let mut slices: Vec<&mut [f32]> = range.iter_mut().map(Vec::as_mut_slice).collect();
    assert_eq!(reader.read(&mut slices)?, 100);
    for (c, r) in channels.iter().zip(&range) {
        assert_eq!(&c[12_345..12_445], r.as_slice());
    }
    assert_eq!(reader.position(), 12_445);
    Ok(())
}

#[test]
fn test_round_trip_every_written_format() -> Result<(), Box<dyn Error>> {
    for channels in [Channels::Mono, Channels::Stereo] {
        for format in [
            SampleFormat::Int16,
            SampleFormat::Int24,
            SampleFormat::Float32,
        ] {
            round_trip(Spec {
                channels,
                sample_rate: 48_000,
                format,
            })?;
        }
    }
    Ok(())
}

#[test]
fn test_round_trip_project_storage_format() -> Result<(), Box<dyn Error>> {
    // docs/architecture/project.md "Stored audio": mono 32-bit float at the project rate.
    for sample_rate in [44_100, 48_000, 88_200, 96_000] {
        round_trip(Spec {
            channels: Channels::Mono,
            sample_rate,
            format: SampleFormat::Float32,
        })?;
    }
    Ok(())
}

#[test]
fn test_file_embedded_at_an_offset() -> Result<(), Box<dyn Error>> {
    let spec = Spec {
        channels: Channels::Mono,
        sample_rate: 44_100,
        format: SampleFormat::Int16,
    };
    let mut cursor = Cursor::new(Vec::new());
    cursor.write_all(&[0xAB; 100])?;
    let mut writer = WavWriter::new(cursor, spec)?;
    writer.write(&[&[0.5, -0.5, 0.25]])?;
    let mut cursor = writer.finish()?;
    cursor.seek(SeekFrom::Start(100))?;
    let mut reader = WavReader::new(cursor)?;
    let mut out = [0.0; 3];
    assert_eq!(reader.read(&mut [&mut out])?, 3);
    assert_eq!(out, [0.5, -0.5, 0.25]);
    Ok(())
}

#[test]
fn test_write_rejects_mismatched_channels() -> Result<(), Box<dyn Error>> {
    let spec = Spec {
        channels: Channels::Stereo,
        sample_rate: 48_000,
        format: SampleFormat::Float32,
    };
    let mut writer = WavWriter::new(Cursor::new(Vec::new()), spec)?;
    assert!(matches!(
        writer.write(&[&[0.0]]),
        Err(wav_files::Error::ChannelCountMismatch {
            expected: 2,
            found: 1
        })
    ));
    assert!(matches!(
        writer.write(&[&[0.0], &[0.0, 1.0]]),
        Err(wav_files::Error::ChannelLengthMismatch)
    ));
    Ok(())
}

#[test]
fn test_empty_file_round_trips() -> Result<(), Box<dyn Error>> {
    let spec = Spec {
        channels: Channels::Mono,
        sample_rate: 48_000,
        format: SampleFormat::Float32,
    };
    let bytes = WavWriter::new(Cursor::new(Vec::new()), spec)?
        .finish()?
        .into_inner();
    let mut reader = WavReader::new(Cursor::new(bytes))?;
    assert_eq!(reader.frames(), 0);
    let mut out = [0.0; 4];
    assert_eq!(reader.read(&mut [&mut out])?, 0);
    Ok(())
}
