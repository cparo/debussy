//! Stored WAV files streamed through `TimelineSource`s into the engine, against offline render of
//! the same clips (ADR-0046).
// Tests wait for the reader thread with `sleep` and write fixture files; neither runs on the
// real-time side, which is what `clippy.toml` guards.
#![allow(clippy::disallowed_methods, clippy::disallowed_types)]

use std::error::Error;
use std::path::PathBuf;
use std::thread::sleep;
use std::time::Duration;

use audio_buffers::Layout;
use audio_engine::{
    ChannelClip, Engine, RenderFeed, TrackFeed, TrackFeeds, TrackRenderer, TransportState, render,
};
use disk_streaming::{Streamer, StreamerConfig, TimelineSource, WavClipAudio};
use signal_graph::{Config, Graph, NodeId, Runner};
use wav_files::{Channels, SampleFormat, Spec, WavWriter};

const RATE: u32 = 48_000;
const BLOCK: usize = 128;
const FRAMES: usize = 40_000;

/// Writes mono float WAV files of reproducible noise, returning their paths.
fn write_files(dir: &std::path::Path, count: u64) -> Result<Vec<PathBuf>, Box<dyn Error>> {
    std::fs::create_dir_all(dir)?;
    (0..count)
        .map(|f| {
            let path = dir.join(format!("{f}.wav"));
            let spec = Spec {
                channels: Channels::Mono,
                sample_rate: RATE,
                format: SampleFormat::Float32,
            };
            let mut writer =
                WavWriter::new(std::io::BufWriter::new(std::fs::File::create(&path)?), spec)?;
            let mut state = 0x9E37_79B9_u32.wrapping_mul(f as u32 + 1);
            let samples: Vec<f32> = (0..50_000)
                .map(|_| {
                    state ^= state << 13;
                    state ^= state >> 17;
                    state ^= state << 5;
                    state as f32 / u32::MAX as f32 * 2.0 - 1.0
                })
                .collect();
            writer.write(&[&samples])?;
            writer.finish()?;
            Ok(path)
        })
        .collect()
}

fn clip(file: usize, offset: u64, length: u64, position: u64) -> ChannelClip {
    ChannelClip {
        file,
        offset,
        length,
        position,
        gain: 0.8,
        fade_in: 1000,
        fade_out: 500,
    }
}

/// A stereo track of two overlapping clips per channel.
fn channels() -> [Vec<ChannelClip>; 2] {
    [
        vec![clip(0, 0, 30_000, 2000), clip(1, 100, 9000, 10_000)],
        vec![clip(2, 0, 30_000, 2000), clip(3, 100, 9000, 10_000)],
    ]
}

fn graph() -> Result<(Graph, Runner, NodeId, NodeId), Box<dyn Error>> {
    let (mut graph, runner) = Graph::new(Config::new(RATE, 512))?;
    let track = graph.add_source(Layout::Stereo);
    let output = graph.add_sink(Layout::Stereo);
    graph.connect(track, 0, output, 0)?;
    graph.commit()?;
    Ok((graph, runner, track, output))
}

#[test]
fn test_streamed_playback_matches_offline_render() -> Result<(), Box<dyn Error>> {
    let dir = std::env::temp_dir().join(format!("disk-streaming-engine-{}", std::process::id()));
    let paths = write_files(&dir, 4)?;
    let from = 1500;

    // Offline render.
    let (_render_graph, mut runner, track, output) = graph()?;
    let feeds = channels()
        .into_iter()
        .map(|clips| -> Result<Box<dyn TrackFeed>, Box<dyn Error>> {
            let renderer = TrackRenderer::new(clips, RATE)?;
            Ok(Box::new(RenderFeed::new(
                renderer,
                WavClipAudio::new(paths.clone()),
                from,
            )))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut tracks = vec![TrackFeeds {
        source: track,
        feeds,
    }];
    let mut rendered = Vec::new();
    render(
        &mut runner,
        output,
        &mut tracks,
        FRAMES as u64,
        256,
        &mut |l, r| {
            rendered.extend(l.iter().zip(r).map(|(&a, &b)| (a, b)));
            Ok(())
        },
    )?;

    // Playback through the disk streamer.
    let (_play_graph, runner, track, output) = graph()?;
    let sources = channels()
        .into_iter()
        .map(
            |clips| -> Result<Box<dyn disk_streaming::FrameSource>, Box<dyn Error>> {
                let renderer = TrackRenderer::new(clips, RATE)?;
                Ok(Box::new(TimelineSource::new(
                    renderer,
                    WavClipAudio::new(paths.clone()),
                    RATE,
                )))
            },
        )
        .collect::<Result<Vec<_>, _>>()?;
    let (mut streamer, playbacks) = Streamer::start(sources, StreamerConfig::new(RATE))?;
    let feeds = playbacks
        .into_iter()
        .map(|p| -> Box<dyn TrackFeed> { Box::new(p) })
        .collect();
    let (mut engine, mut processor) = Engine::new(runner, output, RATE)?;
    engine.set_tracks(vec![TrackFeeds {
        source: track,
        feeds,
    }])?;
    engine.locate(from, &mut streamer)?;
    engine.play(&mut streamer)?;

    let mut left = vec![0.0; BLOCK];
    let mut right = vec![0.0; BLOCK];
    let mut played = Vec::new();
    for _ in 0..500 {
        processor.process(&[], 0, &mut left, &mut right)?;
        if engine.real_time_state() == TransportState::Playing {
            played.extend(left.iter().zip(&right).map(|(&a, &b)| (a, b)));
            break;
        }
        sleep(Duration::from_millis(10));
    }
    assert!(!played.is_empty(), "the streams never primed");
    // Let the reader get the whole range ahead (2 s of read-ahead covers it), then play on.
    sleep(Duration::from_millis(500));
    while played.len() < FRAMES {
        processor.process(&[], 0, &mut left, &mut right)?;
        played.extend(left.iter().zip(&right).map(|(&a, &b)| (a, b)));
    }
    assert_eq!(streamer.underruns(0), Some(0));
    assert_eq!(streamer.underruns(1), Some(0));
    assert_eq!(&played[..FRAMES], &rendered[..]);
    assert!(rendered.iter().any(|&(l, r)| l != 0.0 && r != 0.0));
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

#[test]
fn test_play_after_a_jump_starts_once_the_old_read_ahead_is_full() -> Result<(), Box<dyn Error>> {
    let dir = std::env::temp_dir().join(format!("disk-streaming-jump-{}", std::process::id()));
    let paths = write_files(&dir, 4)?;
    let (_graph, runner, track, output) = graph()?;
    let sources = channels()
        .into_iter()
        .map(
            |clips| -> Result<Box<dyn disk_streaming::FrameSource>, Box<dyn Error>> {
                let renderer = TrackRenderer::new(clips, RATE)?;
                Ok(Box::new(TimelineSource::new(
                    renderer,
                    WavClipAudio::new(paths.clone()),
                    RATE,
                )))
            },
        )
        .collect::<Result<Vec<_>, _>>()?;
    // A read-ahead well short of the sources, so the reader fills every block it has.
    let config = StreamerConfig {
        read_ahead: Duration::from_millis(100),
        ..StreamerConfig::new(RATE)
    };
    let (mut streamer, playbacks) = Streamer::start(sources, config)?;
    let feeds = playbacks
        .into_iter()
        .map(|p| -> Box<dyn TrackFeed> { Box::new(p) })
        .collect();
    let (mut engine, mut processor) = Engine::new(runner, output, RATE)?;
    engine.set_tracks(vec![TrackFeeds {
        source: track,
        feeds,
    }])?;
    let mut left = vec![0.0; BLOCK];
    let mut right = vec![0.0; BLOCK];
    // While stopped the reader fills the read-ahead from frame 0.
    processor.process(&[], 0, &mut left, &mut right)?;
    sleep(Duration::from_millis(200));
    engine.locate(20_000, &mut streamer)?;
    engine.play(&mut streamer)?;
    let mut started = false;
    for _ in 0..200 {
        processor.process(&[], 0, &mut left, &mut right)?;
        if engine.real_time_state() == TransportState::Playing {
            started = true;
            break;
        }
        sleep(Duration::from_millis(5));
    }
    let _ = std::fs::remove_dir_all(&dir);
    assert!(started, "the streams never primed at the new position");
    assert!(engine.playhead() >= 20_000);
    Ok(())
}
