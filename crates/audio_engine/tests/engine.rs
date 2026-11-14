//! The transport, the real-time side under the guard, and playback against offline render.

mod common;

use std::error::Error;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use audio_engine::{
    Engine, Error as EngineError, Processor, RenderFeed, TrackFeeds, TrackRenderer, TransportState,
    render,
};
use common::{
    MemoryAudio, RATE, StreamControl, StreamFeed, TestGraph, TestStreams, clip, graph, noise,
    rendered,
};
use realtime_guard::{GuardAllocator, assert_realtime};

#[global_allocator]
static ALLOCATOR: GuardAllocator = GuardAllocator;

const BLOCK: usize = 128;
const FRAMES: usize = 30_000;

/// The test project: a stereo track of two overlapping clips with fades, and a mono track.
struct Project {
    audio: MemoryAudio,
    left: Vec<audio_engine::ChannelClip>,
    right: Vec<audio_engine::ChannelClip>,
    mono: Vec<audio_engine::ChannelClip>,
}

fn project() -> Project {
    let audio = MemoryAudio(Arc::new((0..5).map(|f| noise(40_000, f + 10)).collect()));
    let mut top = clip(1, 1000, 12_000, 8000);
    top.fade_out = 2000;
    top.gain = 0.7;
    let mut top_right = top;
    top_right.file = 3;
    let mut voice = clip(4, 0, 20_000, 2500);
    voice.fade_in = 3000;
    Project {
        audio,
        left: vec![clip(0, 0, 25_000, 0), top],
        right: vec![clip(2, 0, 25_000, 0), top_right],
        mono: vec![voice],
    }
}

/// Simulated disk streams of the project's tracks for `graph`.
fn stream_feeds(
    p: &Project,
    g: &TestGraph,
    control: &Arc<StreamControl>,
) -> Result<Vec<TrackFeeds>, Box<dyn Error>> {
    let feed = |clips: &[audio_engine::ChannelClip]| -> Result<_, Box<dyn Error>> {
        let feed: Box<dyn audio_engine::TrackFeed> =
            Box::new(StreamFeed::new(rendered(clips, &p.audio, 40_000)?, control));
        Ok(feed)
    };
    Ok(vec![
        TrackFeeds {
            source: g.stereo,
            feeds: vec![feed(&p.left)?, feed(&p.right)?],
        },
        TrackFeeds {
            source: g.mono,
            feeds: vec![feed(&p.mono)?],
        },
    ])
}

/// Runs `blocks` blocks under the real-time guard, returning the output and the frames played.
fn run(processor: &mut Processor, blocks: usize) -> Result<(Vec<f32>, Vec<f32>), Box<dyn Error>> {
    let mut left = vec![0.0; blocks * BLOCK];
    let mut right = vec![0.0; blocks * BLOCK];
    assert_realtime(|| -> Result<(), EngineError> {
        for (l, r) in left.chunks_mut(BLOCK).zip(right.chunks_mut(BLOCK)) {
            processor.process(&[], 0, l, r)?;
        }
        Ok(())
    })?;
    Ok((left, right))
}

/// Plays the project from `from` until `frames` frames have played after priming, returning what
/// was output from the first playing block on.
fn play(latency: usize, from: u64, frames: usize) -> Result<(Vec<f32>, Vec<f32>), Box<dyn Error>> {
    let p = project();
    let g = graph(latency)?;
    let control = Arc::new(StreamControl::default());
    let feeds = stream_feeds(&p, &g, &control)?;
    let (mut engine, mut processor) = Engine::new(g.runner, g.output, RATE)?;
    engine.set_tracks(feeds)?;
    engine.locate(from, &mut TestStreams(Arc::clone(&control)))?;
    engine.play(&mut TestStreams(Arc::clone(&control)))?;
    // Not primed: silence, playhead held.
    let (left, _) = run(&mut processor, 3)?;
    assert!(left.iter().all(|&s| s == 0.0));
    assert_eq!(engine.real_time_state(), TransportState::Starting);
    assert_eq!(engine.playhead(), from);
    control.ready.store(true, Ordering::SeqCst);
    let (left, right) = run(&mut processor, frames.div_ceil(BLOCK))?;
    assert_eq!(engine.real_time_state(), TransportState::Playing);
    assert_eq!(
        engine.playhead(),
        from + (frames.div_ceil(BLOCK) * BLOCK) as u64
    );
    Ok((left, right))
}

/// Renders the project offline from `from`, in blocks of `block`.
fn offline(
    latency: usize,
    from: u64,
    frames: usize,
    block: usize,
) -> Result<(Vec<f32>, Vec<f32>), Box<dyn Error>> {
    let p = project();
    let mut g = graph(latency)?;
    let feed = |clips: &[audio_engine::ChannelClip]| -> Result<_, Box<dyn Error>> {
        let renderer = TrackRenderer::new(clips.to_vec(), RATE)?;
        let feed: Box<dyn audio_engine::TrackFeed> =
            Box::new(RenderFeed::new(renderer, p.audio.clone(), from));
        Ok(feed)
    };
    let mut tracks = vec![
        TrackFeeds {
            source: g.stereo,
            feeds: vec![feed(&p.left)?, feed(&p.right)?],
        },
        TrackFeeds {
            source: g.mono,
            feeds: vec![feed(&p.mono)?],
        },
    ];
    let (mut left, mut right) = (Vec::new(), Vec::new());
    render(
        &mut g.runner,
        g.output,
        &mut tracks,
        0,
        frames as u64,
        block,
        &mut |l, r| {
            left.extend_from_slice(l);
            right.extend_from_slice(r);
            Ok(())
        },
    )?;
    Ok((left, right))
}

#[test]
fn test_playback_and_offline_render_produce_identical_samples() -> Result<(), Box<dyn Error>> {
    for (latency, from, block) in [(0, 0, 128), (0, 1234, 100), (300, 5000, 512), (77, 0, 1)] {
        let frames = if block == 1 { 2000 } else { FRAMES };
        let (rendered_left, rendered_right) = offline(latency, from, frames, block)?;
        assert_eq!(rendered_left.len(), frames);
        assert!(rendered_left.iter().any(|&s| s != 0.0));
        let (left, right) = play(latency, from, frames + latency)?;
        // Playback is heard `latency` frames late; render drops that lead.
        assert!(left[..latency].iter().all(|&s| s == 0.0));
        assert_eq!(
            &left[latency..latency + frames],
            &rendered_left[..],
            "{latency} {from}"
        );
        assert_eq!(&right[latency..latency + frames], &rendered_right[..]);
    }
    Ok(())
}

#[test]
fn test_pause_holds_and_stop_returns_to_start() -> Result<(), Box<dyn Error>> {
    let g = graph(0)?;
    let control = Arc::new(StreamControl::default());
    control.ready.store(true, Ordering::SeqCst);
    let mut streams = TestStreams(Arc::clone(&control));
    let (mut engine, mut processor) = Engine::new(g.runner, g.output, RATE)?;
    engine.locate(1000, &mut streams)?;
    engine.play(&mut streams)?;
    run(&mut processor, 4)?;
    assert_eq!(engine.playhead(), 1000 + 4 * BLOCK as u64);

    engine.pause()?;
    // A resume before the real-time side has paused would start from a stale playhead.
    assert!(matches!(engine.play(&mut streams), Err(EngineError::Busy)));
    run(&mut processor, 2)?;
    assert_eq!(engine.real_time_state(), TransportState::Paused);
    assert_eq!(engine.playhead(), 1000 + 4 * BLOCK as u64);

    engine.play(&mut streams)?;
    run(&mut processor, 1)?;
    assert_eq!(engine.playhead(), 1000 + 5 * BLOCK as u64);

    engine.stop()?;
    run(&mut processor, 1)?;
    assert_eq!(engine.real_time_state(), TransportState::Stopped);
    // Stop returns to where playback last started: the resume point.
    assert_eq!(engine.playhead(), 1000 + 4 * BLOCK as u64);
    Ok(())
}

#[test]
fn test_playhead_after_stop_is_the_start_point_before_the_real_time_side_runs()
-> Result<(), Box<dyn Error>> {
    let g = graph(0)?;
    let control = Arc::new(StreamControl::default());
    control.ready.store(true, Ordering::SeqCst);
    let mut streams = TestStreams(Arc::clone(&control));
    let (mut engine, mut processor) = Engine::new(g.runner, g.output, RATE)?;
    engine.locate(1000, &mut streams)?;
    engine.play(&mut streams)?;
    run(&mut processor, 4)?;
    engine.stop()?;
    // The view redraws before the real-time side has run again.
    assert_eq!(engine.playhead(), 1000);
    run(&mut processor, 1)?;
    assert_eq!(engine.playhead(), 1000);
    Ok(())
}

#[test]
fn test_playhead_after_locate_while_stopped_is_the_new_frame_before_the_real_time_side_runs()
-> Result<(), Box<dyn Error>> {
    let g = graph(0)?;
    let control = Arc::new(StreamControl::default());
    control.ready.store(true, Ordering::SeqCst);
    let mut streams = TestStreams(Arc::clone(&control));
    let (mut engine, mut processor) = Engine::new(g.runner, g.output, RATE)?;
    run(&mut processor, 1)?;
    engine.locate(5000, &mut streams)?;
    assert_eq!(engine.playhead(), 5000);
    run(&mut processor, 1)?;
    assert_eq!(engine.playhead(), 5000);
    Ok(())
}

#[test]
fn test_locate_while_playing_jumps_and_becomes_the_return_point() -> Result<(), Box<dyn Error>> {
    let g = graph(0)?;
    let control = Arc::new(StreamControl::default());
    let feeds = stream_feeds(&project(), &g, &control)?;
    let mut streams = TestStreams(Arc::clone(&control));
    let (mut engine, mut processor) = Engine::new(g.runner, g.output, RATE)?;
    engine.set_tracks(feeds)?;
    engine.play(&mut streams)?;
    control.ready.store(true, Ordering::SeqCst);
    run(&mut processor, 2)?;
    engine.locate(9000, &mut streams)?;
    assert!(!control.ready.load(Ordering::SeqCst));
    run(&mut processor, 1)?;
    assert_eq!(engine.real_time_state(), TransportState::Starting);
    control.ready.store(true, Ordering::SeqCst);
    run(&mut processor, 1)?;
    assert_eq!(engine.playhead(), 9000 + BLOCK as u64);
    engine.stop()?;
    run(&mut processor, 1)?;
    assert_eq!(engine.playhead(), 9000);
    Ok(())
}

#[test]
fn test_clip_changes_are_refused_while_playing() -> Result<(), Box<dyn Error>> {
    let g = graph(0)?;
    let control = Arc::new(StreamControl::default());
    let mut streams = TestStreams(Arc::clone(&control));
    let (mut engine, mut processor) = Engine::new(g.runner, g.output, RATE)?;
    engine.play(&mut streams)?;
    assert!(matches!(
        engine.set_tracks(Vec::new()),
        Err(EngineError::Playing)
    ));
    engine.pause()?;
    run(&mut processor, 1)?;
    engine.set_tracks(Vec::new())?;
    engine.stop()?;
    engine.set_tracks(Vec::new())?;
    run(&mut processor, 1)?;
    engine.collect();
    Ok(())
}

#[test]
fn test_mono_switch_folds_both_outputs() -> Result<(), Box<dyn Error>> {
    let p = project();
    let g = graph(0)?;
    let control = Arc::new(StreamControl::default());
    control.ready.store(true, Ordering::SeqCst);
    let feeds = stream_feeds(&p, &g, &control)?;
    let (mut engine, mut processor) = Engine::new(g.runner, g.output, RATE)?;
    engine.set_tracks(feeds)?;
    engine.play(&mut TestStreams(Arc::clone(&control)))?;
    control.ready.store(true, Ordering::SeqCst);
    let (left, right) = run(&mut processor, 4)?;
    assert!(left.iter().any(|&s| s != 0.0));
    engine.stop()?;
    engine.set_mono(true);
    engine.play(&mut TestStreams(Arc::clone(&control)))?;
    control.ready.store(true, Ordering::SeqCst);
    let (mono_left, mono_right) = run(&mut processor, 4)?;
    assert_eq!(mono_left, mono_right);
    for i in 0..left.len() {
        let want = (left[i] + right[i]) * dsp::FOLD_DOWN_GAIN;
        assert_eq!(mono_left[i], want);
    }
    Ok(())
}

#[test]
fn test_process_rejects_mismatched_buffers_with_silence() -> Result<(), Box<dyn Error>> {
    let g = graph(0)?;
    let (_engine, mut processor) = Engine::new(g.runner, g.output, RATE)?;
    let mut left = [1.0; 64];
    let mut right = [1.0; 32];
    assert!(matches!(
        processor.process(&[], 0, &mut left, &mut right),
        Err(EngineError::BufferLength {
            left: 64,
            right: 32
        })
    ));
    assert!(left.iter().chain(&right).all(|&s| s == 0.0));
    let mut big = vec![0.0; 1024];
    let mut big_right = vec![0.0; 1024];
    assert!(matches!(
        processor.process(&[], 0, &mut big, &mut big_right),
        Err(EngineError::Graph(_))
    ));
    Ok(())
}
