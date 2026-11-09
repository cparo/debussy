//! Arming, monitoring and capture, with the real-time side under the guard, and a take's
//! placement against the playback it was recorded over.

mod common;

use std::error::Error;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use audio_engine::{
    ArmedTrack, Engine, Error as EngineError, Processor, TakeInfo, TrackFeed, TrackFeeds,
};
use common::{RATE, StreamControl, StreamFeed, TestGraph, TestStreams, graph, memory_sink, noise};
use realtime_guard::{GuardAllocator, assert_realtime};

#[global_allocator]
static ALLOCATOR: GuardAllocator = GuardAllocator;

const BLOCK: usize = 128;
/// The simulated device's input plus output latency.
const ROUND_TRIP: usize = 128;

/// The device input for frame `frame` (counted from the stream's start) of channel `channel`.
fn input(frame: usize, channel: usize) -> f32 {
    (frame * 2 + channel) as f32
}

/// A simulated two-input device: runs `blocks` blocks from device frame `at`, each under the
/// guard, with [`input`] as the input. Returns the output.
fn run(
    processor: &mut Processor,
    at: &mut usize,
    blocks: usize,
) -> Result<(Vec<f32>, Vec<f32>), Box<dyn Error>> {
    let (mut left, mut right) = (vec![0.0; blocks * BLOCK], vec![0.0; blocks * BLOCK]);
    for (l, r) in left.chunks_mut(BLOCK).zip(right.chunks_mut(BLOCK)) {
        let interleaved: Vec<f32> = (0..BLOCK * 2).map(|i| input(*at + i / 2, i % 2)).collect();
        assert_realtime(|| processor.process(&interleaved, 2, l, r))?;
        *at += BLOCK;
    }
    Ok((left, right))
}

/// Device channel `channel` of [`input`] for device frames `from..from + frames`.
fn input_channel(channel: usize, from: usize, frames: usize) -> Vec<f32> {
    (from..from + frames).map(|f| input(f, channel)).collect()
}

/// The stereo track's two timelines, and feeds of them for `g`.
fn stereo_feeds(
    g: &TestGraph,
    control: &Arc<StreamControl>,
) -> (Vec<f32>, Vec<f32>, Vec<TrackFeeds>) {
    let (left, right) = (noise(40_000, 1), noise(40_000, 2));
    let feeds: Vec<Box<dyn TrackFeed>> = vec![
        Box::new(StreamFeed::new(left.clone(), control)),
        Box::new(StreamFeed::new(right.clone(), control)),
    ];
    let tracks = vec![TrackFeeds {
        source: g.stereo,
        feeds,
    }];
    (left, right, tracks)
}

#[test]
fn test_capture_starts_once_primed_and_ends_on_stop() -> Result<(), Box<dyn Error>> {
    let g = graph(0)?;
    let control = Arc::new(StreamControl::default());
    let (_, _, tracks) = stereo_feeds(&g, &control);
    let mut streams = TestStreams(Arc::clone(&control));
    let (mut engine, mut processor) = Engine::new(g.runner, g.output, RATE)?;
    engine.set_tracks(tracks)?;
    let (sink, take) = memory_sink(100_000);
    engine.arm(vec![ArmedTrack {
        source: g.mono,
        inputs: vec![1],
        monitor: false,
        sinks: vec![sink],
    }])?;
    engine.locate(1000, &mut streams)?;
    engine.record(&mut streams)?;
    assert!(engine.is_recording());
    let mut at = 0;
    // Not primed: nothing captured.
    run(&mut processor, &mut at, 3)?;
    assert!(take.samples().is_empty());
    assert_eq!(engine.take(), None);

    control.ready.store(true, Ordering::SeqCst);
    let started = at;
    run(&mut processor, &mut at, 4)?;
    assert_eq!(take.samples(), input_channel(1, started, 4 * BLOCK));
    assert_eq!(
        engine.take(),
        Some(TakeInfo {
            start_frame: 1000,
            latency: 0
        })
    );

    engine.stop()?;
    assert!(!engine.is_recording());
    run(&mut processor, &mut at, 2)?;
    assert!(take.ended());
    assert_eq!(take.samples().len(), 4 * BLOCK);
    // The takes are spent.
    assert!(matches!(
        engine.record(&mut streams),
        Err(EngineError::NotArmed)
    ));
    Ok(())
}

#[test]
fn test_pause_ends_the_take_and_rearming_ends_the_old_one() -> Result<(), Box<dyn Error>> {
    let g = graph(0)?;
    let control = Arc::new(StreamControl::default());
    control.ready.store(true, Ordering::SeqCst);
    let mut streams = TestStreams(Arc::clone(&control));
    let (mut engine, mut processor) = Engine::new(g.runner, g.output, RATE)?;
    let arm = |engine: &mut Engine| -> Result<_, Box<dyn Error>> {
        let (sink, take) = memory_sink(10_000);
        engine.arm(vec![ArmedTrack {
            source: g.mono,
            inputs: vec![0],
            monitor: false,
            sinks: vec![sink],
        }])?;
        Ok(take)
    };
    let first = arm(&mut engine)?;
    engine.record(&mut streams)?;
    let mut at = 0;
    run(&mut processor, &mut at, 2)?;
    engine.pause()?;
    run(&mut processor, &mut at, 1)?;
    assert!(first.ended());
    assert_eq!(first.samples().len(), 2 * BLOCK);

    // Armed but never recorded: replacing it ends it, empty.
    let unused = arm(&mut engine)?;
    let _ = arm(&mut engine)?;
    run(&mut processor, &mut at, 1)?;
    assert!(unused.ended());
    assert!(unused.samples().is_empty());
    engine.collect();
    Ok(())
}

#[test]
fn test_monitored_input_replaces_clips_whenever_armed() -> Result<(), Box<dyn Error>> {
    let g = graph(0)?;
    let control = Arc::new(StreamControl::default());
    control.ready.store(true, Ordering::SeqCst);
    let (_, _, tracks) = stereo_feeds(&g, &control);
    let mut streams = TestStreams(Arc::clone(&control));
    let (mut engine, mut processor) = Engine::new(g.runner, g.output, RATE)?;
    engine.set_tracks(tracks)?;
    engine.arm(vec![ArmedTrack {
        source: g.stereo,
        inputs: vec![1, 0],
        monitor: true,
        sinks: Vec::new(),
    }])?;
    let mut at = 0;
    // Stopped: the input is heard, crossed as armed.
    let from = at;
    let (left, right) = run(&mut processor, &mut at, 2)?;
    assert_eq!(left, input_channel(1, from, 2 * BLOCK));
    assert_eq!(right, input_channel(0, from, 2 * BLOCK));
    // Playing: still the input, not the clips.
    engine.play(&mut streams)?;
    let from = at;
    let (left, _) = run(&mut processor, &mut at, 2)?;
    assert_eq!(left, input_channel(1, from, 2 * BLOCK));
    Ok(())
}

#[test]
fn test_unmonitored_armed_track_is_silent_only_while_recording() -> Result<(), Box<dyn Error>> {
    let g = graph(0)?;
    let control = Arc::new(StreamControl::default());
    control.ready.store(true, Ordering::SeqCst);
    let (timeline, _, tracks) = stereo_feeds(&g, &control);
    let mut streams = TestStreams(Arc::clone(&control));
    let (mut engine, mut processor) = Engine::new(g.runner, g.output, RATE)?;
    engine.set_tracks(tracks)?;
    let (sink, _take) = memory_sink(10_000);
    engine.arm(vec![ArmedTrack {
        source: g.stereo,
        inputs: vec![0, 1],
        monitor: false,
        sinks: vec![sink],
    }])?;
    let mut at = 0;
    engine.play(&mut streams)?;
    control.ready.store(true, Ordering::SeqCst);
    let (left, _) = run(&mut processor, &mut at, 2)?;
    assert_eq!(left, timeline[..2 * BLOCK]);
    engine.stop()?;
    engine.record(&mut streams)?;
    control.ready.store(true, Ordering::SeqCst);
    let (left, right) = run(&mut processor, &mut at, 2)?;
    assert!(left.iter().chain(&right).all(|&s| s == 0.0));
    Ok(())
}

#[test]
fn test_arming_seeking_and_record_are_refused_while_playing() -> Result<(), Box<dyn Error>> {
    let g = graph(0)?;
    let control = Arc::new(StreamControl::default());
    let mut streams = TestStreams(Arc::clone(&control));
    let (mut engine, _processor) = Engine::new(g.runner, g.output, RATE)?;
    assert!(matches!(
        engine.record(&mut streams),
        Err(EngineError::NotArmed)
    ));
    let (sink, _take) = memory_sink(16);
    engine.arm(vec![ArmedTrack {
        source: g.mono,
        inputs: vec![0],
        monitor: false,
        sinks: vec![sink],
    }])?;
    engine.record(&mut streams)?;
    assert!(matches!(engine.arm(Vec::new()), Err(EngineError::Playing)));
    assert!(matches!(
        engine.locate(10, &mut streams),
        Err(EngineError::Recording)
    ));
    assert!(matches!(
        engine.record(&mut streams),
        Err(EngineError::Playing)
    ));
    engine.stop()?;
    engine.arm(Vec::new())?;
    Ok(())
}

/// Records over playback through a simulated device whose input hears its own output
/// [`ROUND_TRIP`] frames later, as through a loopback cable, with a plan latency of `latency`.
/// Placed as [`TakeInfo::placement`] says, the take must equal the playback material it heard.
#[test]
fn test_placed_take_lines_up_with_the_playback_it_recorded() -> Result<(), Box<dyn Error>> {
    for (latency, from) in [(0, 5000_usize), (77, 5000), (300, 100)] {
        let g = graph(latency)?;
        let control = Arc::new(StreamControl::default());
        let (timeline, _, tracks) = stereo_feeds(&g, &control);
        let mut streams = TestStreams(Arc::clone(&control));
        let (mut engine, mut processor) = Engine::new(g.runner, g.output, RATE)?;
        engine.set_tracks(tracks)?;
        let (sink, take) = memory_sink(100_000);
        engine.arm(vec![ArmedTrack {
            source: g.mono,
            inputs: vec![0],
            monitor: false,
            sinks: vec![sink],
        }])?;
        engine.locate(from as u64, &mut streams)?;
        engine.record(&mut streams)?;
        control.ready.store(true, Ordering::SeqCst);

        // Everything the device output, from its start; its input is that, ROUND_TRIP late.
        let mut heard = vec![0.0_f32; ROUND_TRIP];
        let mut right = vec![0.0; BLOCK];
        for _ in 0..80 {
            let at = heard.len() - ROUND_TRIP;
            let interleaved: Vec<f32> = heard[at..at + BLOCK]
                .iter()
                .flat_map(|&s| [s, 0.0])
                .collect();
            let mut left = vec![0.0; BLOCK];
            assert_realtime(|| processor.process(&interleaved, 2, &mut left, &mut right))?;
            heard.extend_from_slice(&left);
        }
        engine.stop()?;

        let info = engine.take().ok_or("capture never began")?;
        assert_eq!(
            info,
            TakeInfo {
                start_frame: from as u64,
                latency: latency as u64
            }
        );
        let (position, offset) = info.placement(ROUND_TRIP as u64);
        let take = take.samples();
        assert_eq!(take.len(), 80 * BLOCK);
        // Take frame `offset + k` plays at timeline frame `position + k`. Before the round trip
        // and the plan latency have passed, the device heard what preceded playback.
        let settled = (ROUND_TRIP + latency).saturating_sub(offset as usize);
        let placed = &take[offset as usize + settled..];
        let start = position as usize + settled;
        assert!(placed.iter().any(|&s| s != 0.0));
        assert_eq!(placed, &timeline[start..start + placed.len()], "{latency}");
    }
    Ok(())
}

#[test]
fn test_input_peaks_are_published_while_armed_and_cleared_on_read() -> Result<(), Box<dyn Error>> {
    let g = graph(0)?;
    let (mut engine, mut processor) = Engine::new(g.runner, g.output, RATE)?;
    let mut at = 0;
    // Not armed: not measured.
    run(&mut processor, &mut at, 1)?;
    assert_eq!(engine.take_input_peak(1), 0.0);
    engine.arm(vec![ArmedTrack {
        source: g.mono,
        inputs: vec![0],
        monitor: false,
        sinks: Vec::new(),
    }])?;
    run(&mut processor, &mut at, 2)?;
    // The largest input so far on each channel, across both blocks.
    assert_eq!(engine.take_input_peak(0), input(at - 1, 0));
    assert_eq!(engine.take_input_peak(1), input(at - 1, 1));
    assert_eq!(engine.take_input_peak(0), 0.0);
    assert_eq!(engine.take_input_peak(audio_engine::MAX_INPUTS), 0.0);
    Ok(())
}
