//! The clip renderer: placement, gain, fades, overlaps and crossfades, and chunking.

mod common;

use std::error::Error;
use std::f32::consts::FRAC_PI_2;
use std::sync::Arc;

use audio_engine::{ClipAudio, Error as EngineError, TrackRenderer};
use common::{MemoryAudio, RATE, clip, noise, rendered};

/// 10 ms at 48 kHz.
const CROSSFADE: usize = 480;

#[test]
fn test_render_places_clip_with_gain_and_fades() -> Result<(), Box<dyn Error>> {
    let file = noise(1000, 1);
    let audio = MemoryAudio(Arc::new(vec![file.clone()]));
    let mut c = clip(0, 100, 500, 50);
    c.gain = 0.5;
    c.fade_in = 100;
    c.fade_out = 50;
    let out = rendered(&[c], &audio, 700)?;
    assert!(out[..50].iter().all(|&s| s == 0.0));
    assert!(out[550..].iter().all(|&s| s == 0.0));
    for k in 0..500 {
        let fade = if k < 100 {
            k as f32 / 100.0
        } else if 499 - k < 50 {
            (499 - k) as f32 / 50.0
        } else {
            1.0
        };
        assert_eq!(out[50 + k], 1.0 * 0.5 * fade * file[100 + k]);
    }
    Ok(())
}

#[test]
fn test_render_top_clip_covers_and_crossfades_with_equal_power() -> Result<(), Box<dyn Error>> {
    let below = vec![1.0; 10_000];
    let above = vec![-1.0; 10_000];
    let audio = MemoryAudio(Arc::new(vec![below, above]));
    // The top clip sits inside the bottom one: crossfades at both of its edges.
    let out = rendered(
        &[clip(0, 0, 10_000, 0), clip(1, 0, 4000, 3000)],
        &audio,
        10_000,
    )?;
    assert!(out[..3000].iter().all(|&s| s == 1.0));
    for k in 0..CROSSFADE {
        let x = (k as f32 + 0.5) / CROSSFADE as f32;
        let want = -(x * FRAC_PI_2).sin() + (x * FRAC_PI_2).cos();
        assert!((out[3000 + k] - want).abs() < 1e-6);
        assert!((out[6999 - k] - want).abs() < 1e-6);
    }
    assert!(
        out[3000 + CROSSFADE..7000 - CROSSFADE]
            .iter()
            .all(|&s| s == -1.0)
    );
    assert!(out[7000..].iter().all(|&s| s == 1.0));
    Ok(())
}

#[test]
fn test_render_bare_edges_do_not_crossfade() -> Result<(), Box<dyn Error>> {
    let audio = MemoryAudio(Arc::new(vec![vec![1.0; 1000], vec![-1.0; 1000]]));
    // Adjacent clips, and the top one starting on a gap: no overlap, no crossfade.
    let out = rendered(&[clip(0, 0, 500, 0), clip(1, 0, 500, 500)], &audio, 1000)?;
    assert!(out[..500].iter().all(|&s| s == 1.0));
    assert!(out[500..].iter().all(|&s| s == -1.0));
    Ok(())
}

#[test]
fn test_render_in_chunks_matches_one_call() -> Result<(), Box<dyn Error>> {
    let audio = MemoryAudio(Arc::new(vec![noise(20_000, 2), noise(20_000, 3)]));
    let mut faded = clip(1, 500, 9000, 4000);
    faded.fade_in = 700;
    faded.fade_out = 1300;
    faded.gain = 1.5;
    let clips = [clip(0, 0, 15_000, 1000), faded, clip(0, 7, 300, 12_900)];
    let whole = rendered(&clips, &audio, 18_000)?;
    let mut renderer = TrackRenderer::new(clips.to_vec(), RATE)?;
    for chunk in [1, 127, 4096, 5000] {
        let mut out = vec![0.0; 18_000];
        let mut start = 0;
        for part in out.chunks_mut(chunk) {
            renderer.render(start, part, &mut audio.clone())?;
            start += part.len() as u64;
        }
        assert_eq!(out, whole, "chunks of {chunk}");
    }
    Ok(())
}

#[test]
fn test_render_reports_read_failure() -> Result<(), Box<dyn Error>> {
    struct Failing;
    impl ClipAudio for Failing {
        fn read(
            &mut self,
            _file: usize,
            _frame: u64,
            _out: &mut [f32],
        ) -> Result<(), Box<dyn Error + Send + Sync>> {
            Err("disk gone".into())
        }
    }
    let mut renderer = TrackRenderer::new(vec![clip(0, 0, 10, 0)], RATE)?;
    let mut out = [0.0; 10];
    assert!(matches!(
        renderer.render(0, &mut out, &mut Failing),
        Err(EngineError::ClipAudio(_))
    ));
    Ok(())
}
