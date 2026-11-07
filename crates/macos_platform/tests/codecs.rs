//! AudioToolbox codecs: FLAC and AAC round trips, MP3 decoding, and untrusted files refused
//! without panicking, against the contract `platform_interfaces`' test doubles follow.

use std::error::Error;
use std::f32::consts::TAU;
use std::path::PathBuf;

use macos_platform::MacCodecs;
use platform_interfaces::{CodecFormat, Codecs, EncodeSettings, Error as PlatformError, FlacDepth};

fn dir(name: &str) -> Result<PathBuf, Box<dyn Error>> {
    let dir = std::env::temp_dir().join(format!("macos-platform-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// One second of interleaved stereo: 1 kHz left, 440 Hz right, at half scale.
fn tone(rate: u32) -> Vec<f32> {
    (0..rate)
        .flat_map(|n| {
            let t = n as f32 / rate as f32;
            [
                0.5 * (TAU * 1000.0 * t).sin(),
                0.5 * (TAU * 440.0 * t).sin(),
            ]
        })
        .collect()
}

/// Encodes `samples` and decodes them back, in odd-sized chunks both ways.
fn round_trip(
    settings: EncodeSettings,
    name: &str,
    samples: &[f32],
    rate: u32,
) -> Result<(platform_interfaces::SourceInfo, Vec<f32>), Box<dyn Error>> {
    let path = dir("round-trip")?.join(name);
    let codecs = MacCodecs::new();
    let mut encoder = codecs.encode(&path, settings, rate, 2)?;
    for chunk in samples.chunks(2 * 1001) {
        encoder.write(chunk)?;
    }
    encoder.finish()?;
    let mut decoder = codecs.decode(&path)?;
    let info = decoder.info();
    let mut out = Vec::new();
    let mut buffer = vec![0.0; 2 * 777];
    loop {
        let n = decoder.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        out.extend_from_slice(&buffer[..2 * n]);
    }
    Ok((info, out))
}

#[test]
fn test_flac_round_trips_within_quantisation() -> Result<(), Box<dyn Error>> {
    let rate = 48_000;
    let samples = tone(rate);
    for (depth, bits, name) in [
        (FlacDepth::Int16, 16, "t16.flac"),
        (FlacDepth::Int24, 24, "t24.flac"),
    ] {
        let (info, out) = round_trip(EncodeSettings::Flac { depth }, name, &samples, rate)?;
        assert_eq!(info.format, CodecFormat::Flac);
        assert_eq!((info.sample_rate, info.channels), (rate, 2));
        assert_eq!(info.frames, Some(u64::from(rate)));
        assert_eq!(out.len(), samples.len());
        let step = 2.0f32.powi(1 - bits);
        let worst = out
            .iter()
            .zip(&samples)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0, f32::max);
        assert!(worst <= step, "{bits}-bit error {worst}");
    }
    Ok(())
}

#[test]
fn test_aac_round_trip_keeps_the_signal() -> Result<(), Box<dyn Error>> {
    let rate = 48_000;
    let samples = tone(rate);
    let (info, out) = round_trip(
        EncodeSettings::Aac { bitrate_kbps: 256 },
        "t.m4a",
        &samples,
        rate,
    )?;
    assert_eq!(info.format, CodecFormat::Aac);
    assert_eq!((info.sample_rate, info.channels), (rate, 2));
    // Lossy, so compare the middle half by correlation at the best small lag.
    let frames = samples.len().min(out.len()) / 2;
    assert!(
        frames > rate as usize * 9 / 10,
        "only {frames} frames came back"
    );
    let window = frames / 4..frames * 3 / 4;
    let best = (0..=4096usize)
        .map(|lag| {
            let (mut dot, mut a2, mut b2) = (0.0f64, 0.0f64, 0.0f64);
            for f in window.clone() {
                let (Some(&a), Some(&b)) = (samples.get(2 * f), out.get(2 * (f + lag))) else {
                    continue;
                };
                dot += f64::from(a) * f64::from(b);
                a2 += f64::from(a) * f64::from(a);
                b2 += f64::from(b) * f64::from(b);
            }
            dot / (a2 * b2).sqrt().max(1e-12)
        })
        .fold(0.0, f64::max);
    assert!(best > 0.99, "correlation {best}");
    Ok(())
}

/// `frames` frames of MPEG-1 Layer III, 128 kbit/s, 44.1 kHz mono, holding digital silence: a
/// valid header and all-zero side information and main data. Made here, so no third-party file.
fn silent_mp3(frames: usize) -> Vec<u8> {
    let frame_bytes = 144 * 128_000 / 44_100;
    let mut bytes = Vec::with_capacity(frames * frame_bytes);
    for _ in 0..frames {
        let start = bytes.len();
        bytes.extend_from_slice(&[0xFF, 0xFB, 0x90, 0xC0]);
        bytes.resize(start + frame_bytes, 0);
    }
    bytes
}

#[test]
fn test_mp3_decodes() -> Result<(), Box<dyn Error>> {
    let path = dir("mp3")?.join("silence.mp3");
    std::fs::write(&path, silent_mp3(200))?;
    let codecs = MacCodecs::new();
    assert!(codecs.can_decode(CodecFormat::Mp3));
    assert!(!codecs.can_encode(CodecFormat::Mp3));
    let mut decoder = codecs.decode(&path)?;
    let info = decoder.info();
    assert_eq!(info.format, CodecFormat::Mp3);
    assert_eq!((info.sample_rate, info.channels), (44_100, 1));
    let mut total = 0;
    let mut buffer = vec![1.0; 4096];
    loop {
        let n = decoder.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        assert!(buffer[..n].iter().all(|&s| s.abs() < 1e-6));
        total += n;
    }
    // 1152 frames per MP3 frame, less the decoder's delay.
    assert!(
        (200 * 1152 - 3000..=200 * 1152).contains(&total),
        "{total} frames"
    );
    Ok(())
}

#[test]
fn test_decode_refuses_untrusted_files_without_panicking() -> Result<(), Box<dyn Error>> {
    let dir = dir("untrusted")?;
    let codecs = MacCodecs::new();
    // The contract: a missing file is an I/O error.
    assert!(matches!(
        codecs.decode(&dir.join("missing.flac")),
        Err(PlatformError::Io(e)) if e.kind() == std::io::ErrorKind::NotFound
    ));
    // Garbage under every extension.
    let mut noise = 0x1234_5678u32;
    let junk: Vec<u8> = (0..50_000)
        .map(|_| {
            noise ^= noise << 13;
            noise ^= noise >> 17;
            noise ^= noise << 5;
            noise as u8
        })
        .collect();
    for name in ["junk.mp3", "junk.m4a", "junk.flac", "junk.bin"] {
        let path = dir.join(name);
        std::fs::write(&path, &junk)?;
        assert!(codecs.decode(&path).is_err(), "{name} was accepted");
    }
    // A truncated FLAC: header intact, audio cut off.
    let rate = 48_000;
    let (_, _) = round_trip(
        EncodeSettings::Flac {
            depth: FlacDepth::Int16,
        },
        "whole.flac",
        &tone(rate),
        rate,
    )?;
    let whole = std::fs::read(
        std::env::temp_dir()
            .join(format!("macos-platform-{}-round-trip", std::process::id()))
            .join("whole.flac"),
    )?;
    let cut = dir.join("cut.flac");
    std::fs::write(&cut, &whole[..whole.len() / 3])?;
    if let Ok(mut decoder) = codecs.decode(&cut) {
        let mut buffer = vec![0.0; 8192];
        // Reads end early or fail; neither panics.
        for _ in 0..1000 {
            match decoder.read(&mut buffer) {
                Ok(0) | Err(_) => break,
                Ok(_) => {}
            }
        }
    }
    // WAV is read in-house (ADR-0025), not through AudioToolbox.
    let wav = dir.join("tone.wav");
    let spec = wav_files::Spec {
        channels: wav_files::Channels::Mono,
        sample_rate: 48_000,
        format: wav_files::SampleFormat::Float32,
    };
    let mut writer = wav_files::WavWriter::new(std::fs::File::create(&wav)?, spec)?;
    writer.write(&[&[0.0; 1000]])?;
    writer.finish()?;
    assert!(matches!(
        codecs.decode(&wav),
        Err(PlatformError::Undecodable(_))
    ));
    Ok(())
}

#[test]
fn test_encode_refuses_bad_channel_counts() {
    let codecs = MacCodecs::new();
    let path = std::env::temp_dir().join("macos-platform-never.flac");
    for channels in [0, 3] {
        assert!(
            codecs
                .encode(
                    &path,
                    EncodeSettings::Flac {
                        depth: FlacDepth::Int16
                    },
                    48_000,
                    channels
                )
                .is_err()
        );
    }
}
