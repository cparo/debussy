//! Compressed exports through the real encoders, decoded back through AudioToolbox: AAC at
//! every bit rate the Export dialog offers (ADR-0130), and MP3 through the host's `lame`
//! (ADR-0129), skipped with a note when no LAME is installed.

use std::error::Error;
use std::f64::consts::TAU;
use std::path::Path;
use std::sync::Arc;

use audio_buffers::Layout;
use audio_engine::{ChannelClip, ClipAudio, RenderFeed, TrackFeed, TrackFeeds, TrackRenderer};
use debussy_ui::AAC_BITRATES;
use import_export::{ExportChannels, ExportFormat, Lame, Mp3Bitrate, OfflineRender, export};
use macos_platform::{MacCodecs, folders};
use platform_interfaces::{CodecFormat, Codecs};
use signal_graph::{Config, Graph};

/// Stored audio for the render, in memory.
struct Memory(Arc<Vec<f32>>);

impl ClipAudio for Memory {
    fn read(
        &mut self,
        _file: usize,
        frame: u64,
        out: &mut [f32],
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let start = frame as usize;
        out.copy_from_slice(self.0.get(start..start + out.len()).ok_or("past end")?);
        Ok(())
    }
}

/// Exports one second of a 440 Hz sine at half scale at `rate`, as `format` with `channels`,
/// to `path`.
fn export_sine(
    format: ExportFormat,
    channels: ExportChannels,
    lame: Option<&Lame>,
    rate: u32,
    path: &Path,
) -> Result<(), Box<dyn Error>> {
    let frames = rate as usize;
    let sine: Vec<f32> = (0..frames)
        .map(|n| (0.5 * (TAU * 440.0 * n as f64 / f64::from(rate)).sin()) as f32)
        .collect();
    let audio = Arc::new(sine);
    let (mut graph, mut runner) = Graph::new(Config::new(rate, 512))?;
    let source = graph.add_source(Layout::Stereo);
    let output = graph.add_sink(Layout::Stereo);
    graph.connect(source, 0, output, 0)?;
    graph.commit()?;
    let mut feeds: Vec<Box<dyn TrackFeed>> = Vec::new();
    for _ in 0..2 {
        let clip = ChannelClip {
            file: 0,
            offset: 0,
            length: frames as u64,
            position: 0,
            gain: 1.0,
            fade_in: 0,
            fade_out: 0,
        };
        let renderer = TrackRenderer::new(vec![clip], rate)?;
        feeds.push(Box::new(RenderFeed::new(
            renderer,
            Memory(Arc::clone(&audio)),
            0,
        )));
    }
    let mut tracks = vec![TrackFeeds { source, feeds }];
    export(
        OfflineRender {
            runner: &mut runner,
            output,
            tracks: &mut tracks,
            start: 0,
            frames: frames as u64,
            block_frames: 256,
        },
        format,
        channels,
        rate,
        path,
        &MacCodecs::new(),
        lame,
    )?;
    Ok(())
}

/// Decodes `path` and checks it holds one second of the half-scale sine as `format`, at `rate`
/// with `channels`.
fn check_sine(
    codecs: &MacCodecs,
    path: &Path,
    format: CodecFormat,
    rate: u32,
    channels: u16,
) -> Result<(), Box<dyn Error>> {
    let mut decoder = codecs.decode(path)?;
    let info = decoder.info();
    assert_eq!(info.format, format);
    assert_eq!((info.sample_rate, info.channels), (rate, channels));
    let width = usize::from(channels);
    let mut samples = Vec::new();
    let mut block = vec![0.0f32; width * 4096];
    loop {
        let read = decoder.read(&mut block)?;
        if read == 0 {
            break;
        }
        samples.extend_from_slice(block.get(..width * read).ok_or("overread")?);
    }
    let frames = samples.len() / width;
    // One second, give or take the encoder's padding of a few codec frames.
    let second = rate as usize;
    let name = path.display();
    assert!(
        (second * 47 / 48..=second * 53 / 48).contains(&frames),
        "{name}: {frames} frames"
    );
    let rms =
        (samples.iter().map(|s| f64::from(*s).powi(2)).sum::<f64>() / samples.len() as f64).sqrt();
    // Both channels hold the sine, so the mono fold, (L + R) × 0.7071, is √2 louder.
    let peak = if channels == 1 {
        0.5 * 2.0f64.sqrt()
    } else {
        0.5
    };
    let want = peak / 2.0f64.sqrt();
    assert!((rms - want).abs() < 0.05 * want, "{name}: RMS {rms}");
    Ok(())
}

#[test]
fn test_aac_export_at_every_offered_bit_rate_decodes_to_the_sine() -> Result<(), Box<dyn Error>> {
    let dir = std::env::temp_dir().join(format!("aac-export-{}", std::process::id()));
    std::fs::create_dir_all(&dir)?;
    let codecs = MacCodecs::new();
    for kbps in AAC_BITRATES {
        for (channels, count) in [(ExportChannels::Stereo, 2), (ExportChannels::Mono, 1)] {
            let path = dir.join(format!("sine-{kbps}-{count}.m4a"));
            let format = ExportFormat::Aac { bitrate_kbps: kbps };
            export_sine(format, channels, None, 48_000, &path)?;
            check_sine(&codecs, &path, CodecFormat::Aac, 48_000, count)?;
        }
    }
    std::fs::remove_dir_all(&dir)?;
    Ok(())
}

#[test]
fn test_mp3_export_with_real_lame_decodes_to_the_sine() -> Result<(), Box<dyn Error>> {
    let Some(lame) = Lame::probe(&folders::lame_candidates()) else {
        eprintln!("skipped: no LAME installed (`brew install lame`)");
        return Ok(());
    };
    let dir = std::env::temp_dir().join(format!("mp3-export-{}", std::process::id()));
    std::fs::create_dir_all(&dir)?;
    let codecs = MacCodecs::new();
    for rate in [48_000, 96_000] {
        let path = dir.join(format!("sine-{rate}.mp3"));
        let format = ExportFormat::Mp3 {
            bitrate: Mp3Bitrate::Kbps320,
        };
        export_sine(format, ExportChannels::Stereo, Some(&lame), rate, &path)?;
        // MP3 tops out at 48 kHz: 96 kHz is resampled to it.
        check_sine(&codecs, &path, CodecFormat::Mp3, 48_000, 2)?;
    }
    std::fs::remove_dir_all(&dir)?;
    Ok(())
}
