//! Offline render: the same per-block step as playback, without the real-time thread
//! (`docs/architecture/engine.md` "Offline render").

use signal_graph::{NodeId, Runner};

use crate::block;
use crate::error::Error;
use crate::feed::TrackFeeds;

/// Where rendered output goes: each stretch of left and right samples.
type Output<'a> = dyn FnMut(&[f32], &[f32]) -> Result<(), Error> + 'a;

/// Renders `frames` frames through `runner`'s plan from `tracks` (normally
/// [`RenderFeed`](crate::RenderFeed)s positioned at `start`, the range's first timeline frame),
/// in blocks of up to `block_frames`, passing each stretch of left and right output to `out`.
/// The plan's first `latency` frames are dropped, so the first frame passed is the range's
/// first; mono listening does not apply. Curves are read from `start` on, as in playback.
///
/// # Errors
///
/// [`Error::Graph`] if `block_frames` is 0 or larger than the graph allows; whatever `out`
/// returns.
pub fn render(
    runner: &mut Runner,
    output: NodeId,
    tracks: &mut [TrackFeeds],
    start: u64,
    frames: u64,
    block_frames: usize,
    out: &mut Output<'_>,
) -> Result<(), Error> {
    let mut left = vec![0.0; block_frames];
    let mut right = vec![0.0; block_frames];
    let mut skip: Option<u64> = None;
    let mut emitted = 0;
    // The timeline frame of the next block's sources.
    let mut position = i64::try_from(start).unwrap_or(i64::MAX);
    while emitted < frames {
        // Before the first block the latency is unknown; render one full block to learn it.
        let wanted = match skip {
            Some(skip) => (skip + frames - emitted) as usize,
            None => block_frames,
        };
        let n = wanted.min(block_frames).max(1);
        block::run(
            runner,
            output,
            tracks,
            None,
            position,
            true,
            false,
            &mut left[..n],
            &mut right[..n],
        )?;
        position = position.saturating_add(n as i64);
        let skip = skip.get_or_insert(runner.latency() as u64);
        let dropped = (*skip).min(n as u64) as usize;
        *skip -= dropped as u64;
        let take = ((n - dropped) as u64).min(frames - emitted) as usize;
        out(
            &left[dropped..dropped + take],
            &right[dropped..dropped + take],
        )?;
        emitted += take as u64;
    }
    Ok(())
}
