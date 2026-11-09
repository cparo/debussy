//! The click at the output stage, with the real-time side under the guard: it follows the
//! playhead behind the plan's latency, sounds only while playing, and is the same for any block
//! size.

mod common;

use std::error::Error;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use audio_engine::{ArmedTrack, ClickTrack, Engine, Processor, TransportState};
use common::{RATE, StreamControl, TestStreams, graph, memory_sink};
use realtime_guard::{GuardAllocator, assert_realtime};

#[global_allocator]
static ALLOCATOR: GuardAllocator = GuardAllocator;

/// A short decaying click.
fn sound() -> Arc<[f32]> {
    (0..300).map(|i| 1.0 - i as f32 / 300.0).collect()
}

/// `blocks` blocks of `block` frames, each under the guard.
fn run(
    processor: &mut Processor,
    blocks: usize,
    block: usize,
) -> Result<(Vec<f32>, Vec<f32>), Box<dyn Error>> {
    let (mut left, mut right) = (vec![0.0; blocks * block], vec![0.0; blocks * block]);
    for (l, r) in left.chunks_mut(block).zip(right.chunks_mut(block)) {
        assert_realtime(|| processor.process(&[], 0, l, r))?;
    }
    Ok((left, right))
}

/// An engine over the test graph with plan latency `latency`, a click of 1000 frames per beat
/// in 4/4 switched on, playing from `from` once the first block has run.
fn playing(latency: usize, from: u64) -> Result<(Engine, Processor), Box<dyn Error>> {
    let g = graph(latency)?;
    let control = Arc::new(StreamControl::default());
    let (mut engine, processor) = Engine::new(g.runner, g.output, RATE)?;
    // The graph is kept by the runner's plan; the test graph handle may go.
    drop(g.graph);
    engine.set_click(ClickTrack::new(sound(), 1000.0, 4)?)?;
    engine.set_click_on(true);
    engine.locate(from, &mut TestStreams(Arc::clone(&control)))?;
    engine.play(&mut TestStreams(Arc::clone(&control)))?;
    // No track feeds: playback starts at once.
    control.ready.store(true, Ordering::SeqCst);
    Ok((engine, processor))
}

/// The click alone for timeline frames `start..start + frames`.
fn expected(start: i64, frames: usize) -> Result<Vec<f32>, Box<dyn Error>> {
    let click = ClickTrack::new(sound(), 1000.0, 4)?;
    let (mut left, mut right) = (vec![0.0; frames], vec![0.0; frames]);
    click.mix(start, 1.0, &mut left, &mut right);
    Ok(left)
}

#[test]
fn test_click_plays_behind_the_playhead_by_the_plan_latency() -> Result<(), Box<dyn Error>> {
    for latency in [0, 200] {
        let (_engine, mut processor) = playing(latency, 3_000)?;
        let (left, right) = run(&mut processor, 40, 128)?;
        assert_eq!(left, right);
        assert_eq!(left, expected(3_000 - latency as i64, left.len())?);
        assert!(left.iter().any(|&s| s != 0.0));
    }
    Ok(())
}

#[test]
fn test_click_is_the_same_for_any_block_size() -> Result<(), Box<dyn Error>> {
    let (_, mut processor) = playing(64, 500)?;
    let (whole, _) = run(&mut processor, 10, 512)?;
    for block in [32, 64, 160, 320] {
        let (_, mut processor) = playing(64, 500)?;
        let (left, _) = run(&mut processor, 5_120 / block, block)?;
        assert_eq!(left, whole, "block {block}");
    }
    Ok(())
}

#[test]
fn test_click_is_silent_when_off_stopped_or_paused() -> Result<(), Box<dyn Error>> {
    let (mut engine, mut processor) = playing(0, 0)?;
    engine.set_click_on(false);
    let (off, _) = run(&mut processor, 10, 128)?;
    assert!(off.iter().all(|&s| s == 0.0));
    engine.set_click_on(true);
    engine.pause()?;
    let (paused, _) = run(&mut processor, 10, 128)?;
    assert!(paused.iter().all(|&s| s == 0.0));
    engine.stop()?;
    let (stopped, _) = run(&mut processor, 10, 128)?;
    assert!(stopped.iter().all(|&s| s == 0.0));
    Ok(())
}

#[test]
fn test_click_level_scales_the_click() -> Result<(), Box<dyn Error>> {
    let (engine, mut processor) = playing(0, 0)?;
    engine.set_click_level(-6.0);
    let (left, _) = run(&mut processor, 10, 128)?;
    let gain = engine.click_gain();
    assert!((gain - 0.501).abs() < 0.001);
    let full = expected(0, left.len())?;
    for (got, want) in left.iter().zip(&full) {
        assert!((got - want * gain).abs() < 1e-6);
    }
    Ok(())
}

/// A count-in clicks on the grid extended before the playhead, even with the click off, from a
/// playhead mid-bar; recording then plays with the click off.
#[test]
fn test_count_in_clicks_on_the_grid_even_with_the_click_off() -> Result<(), Box<dyn Error>> {
    for latency in [0, 96] {
        let g = graph(latency)?;
        let control = Arc::new(StreamControl::default());
        let mut streams = TestStreams(Arc::clone(&control));
        let (mut engine, mut processor) = Engine::new(g.runner, g.output, RATE)?;
        engine.set_click(ClickTrack::new(sound(), 1000.0, 4)?)?;
        let (sink, _take) = memory_sink(100_000);
        engine.arm(vec![ArmedTrack {
            source: g.mono,
            inputs: vec![0],
            monitor: false,
            sinks: vec![sink],
        }])?;
        // Mid-bar: a bar is 4000 frames; count in one bar.
        let from = 5_500;
        engine.locate(from, &mut streams)?;
        engine.record(&mut streams, 4_000)?;
        control.ready.store(true, Ordering::SeqCst);
        let (left, right) = run(&mut processor, 50, 128)?;
        assert_eq!(left, right);
        assert_eq!(engine.real_time_state(), TransportState::Playing);
        let counted = expected(from as i64 - 4_000 - latency as i64, 4_000)?;
        assert_eq!(&left[..4_000], counted.as_slice());
        assert!(counted.iter().any(|&s| s != 0.0));
        // The click is off once playing.
        assert!(left[4_000..].iter().all(|&s| s == 0.0));
    }
    Ok(())
}
