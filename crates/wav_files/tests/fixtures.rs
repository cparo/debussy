//! Files written by another tool (`afconvert`; see `tests/fixtures/README.md`).

use std::error::Error;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use wav_files::{Channels, SampleFormat, WavReader};

fn open(name: &str) -> Result<WavReader<BufReader<File>>, Box<dyn Error>> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    Ok(WavReader::new(BufReader::new(File::open(path)?))?)
}

/// Every channel of the fixture `name`.
fn decode(name: &str) -> Result<Vec<Vec<f32>>, Box<dyn Error>> {
    let mut reader = open(name)?;
    let frames = usize::try_from(reader.frames())?;
    let mut channels = vec![vec![0.0; frames]; reader.spec().channels.count()];
    let mut slices: Vec<&mut [f32]> = channels.iter_mut().map(Vec::as_mut_slice).collect();
    assert_eq!(reader.read(&mut slices)?, frames);
    assert!(!reader.truncated(), "{name}");
    Ok(channels)
}

#[test]
fn test_afconvert_fixtures_have_expected_specs() -> Result<(), Box<dyn Error>> {
    let cases = [
        ("mono_LEI16.wav", Channels::Mono, SampleFormat::Int16),
        ("mono_LEI24.wav", Channels::Mono, SampleFormat::Int24),
        ("mono_LEI32.wav", Channels::Mono, SampleFormat::Int32),
        ("mono_LEF32.wav", Channels::Mono, SampleFormat::Float32),
        ("mono_LEF64.wav", Channels::Mono, SampleFormat::Float64),
        ("mono_LEF32_junk.wav", Channels::Mono, SampleFormat::Float32),
        ("stereo_LEI24.wav", Channels::Stereo, SampleFormat::Int24),
        ("stereo_LEF32.wav", Channels::Stereo, SampleFormat::Float32),
    ];
    for (name, channels, format) in cases {
        let spec = open(name)?.spec();
        assert_eq!(
            (spec.channels, spec.format, spec.sample_rate),
            (channels, format, 8000),
            "{name}"
        );
    }
    Ok(())
}

#[test]
fn test_afconvert_fixtures_decode_to_the_same_samples() -> Result<(), Box<dyn Error>> {
    let reference = decode("mono_LEI16.wav")?;
    assert!(reference[0].len() > 6000, "about 0.8 s at 8 kHz");
    assert!(reference[0].iter().any(|&s| s.abs() > 0.1), "not silent");
    for name in [
        "mono_LEI24.wav",
        "mono_LEI32.wav",
        "mono_LEF32.wav",
        "mono_LEF64.wav",
        "mono_LEF32_junk.wav",
    ] {
        assert_eq!(decode(name)?, reference, "{name}");
    }
    Ok(())
}

#[test]
fn test_afconvert_stereo_fixtures_match_each_other() -> Result<(), Box<dyn Error>> {
    let int = decode("stereo_LEI24.wav")?;
    let float = decode("stereo_LEF32.wav")?;
    assert_eq!(int, float);
    assert_eq!(int[0].len(), decode("mono_LEI16.wav")?[0].len());
    Ok(())
}
