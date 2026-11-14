//! MP3 export through the host's real `lame`, decoded back through AudioToolbox (ADR-0129).
//! Skipped, with a note, when no LAME is installed.

use std::error::Error;
use std::f64::consts::TAU;
use std::path::Path;
use std::sync::Arc;

use audio_buffers::Layout;
use audio_engine::{ChannelClip, ClipAudio, RenderFeed, TrackFeed, TrackFeeds, TrackRenderer};
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

/// Exports one second of a 440 Hz sine at half scale, in stereo at `rate`, to MP3 at `path`.
fn export_sine(lame: &Lame, rate: u32, path: &Path) -> Result<(), Box<dyn Error>> {
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
        ExportFormat::Mp3 {
            bitrate: Mp3Bitrate::Kbps320,
        },
        ExportChannels::Stereo,
        rate,
        path,
        &MacCodecs::new(),
        Some(lame),
    )?;
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
        export_sine(&lame, rate, &path)?;
        let mut decoder = codecs.decode(&path)?;
        let info = decoder.info();
        assert_eq!(info.format, CodecFormat::Mp3);
        // MP3 tops out at 48 kHz: 96 kHz is resampled to it.
        assert_eq!((info.sample_rate, info.channels), (48_000, 2));
        let mut samples = Vec::new();
        let mut block = vec![0.0f32; 2 * 4096];
        loop {
            let read = decoder.read(&mut block)?;
            if read == 0 {
                break;
            }
            samples.extend_from_slice(block.get(..2 * read).ok_or("overread")?);
        }
        let frames = samples.len() / 2;
        // One second, give or take the encoder's padding of a few MP3 frames.
        assert!(
            (47_000..=53_000).contains(&frames),
            "{rate} Hz: {frames} frames"
        );
        let rms = (samples.iter().map(|s| f64::from(*s).powi(2)).sum::<f64>()
            / samples.len() as f64)
            .sqrt();
        let want = 0.5 / 2.0f64.sqrt();
        assert!((rms - want).abs() < 0.05 * want, "{rate} Hz: RMS {rms}");
    }
    std::fs::remove_dir_all(&dir)?;
    Ok(())
}
