//! The per-block step that playback and offline render share, so both produce the same samples.

use dsp::FOLD_DOWN_GAIN;
use signal_graph::{NodeId, Runner};

use crate::error::Error;
use crate::feed::TrackFeeds;

/// Runs one block of `left.len()` frames: fills each track's source from its feeds (or with
/// silence unless `playing`), runs the plan, and writes the output sink to `left` and `right`,
/// folded to mono on both if `mono`. Real-time safe if the feeds are.
///
/// # Errors
///
/// [`Error::BufferLength`] or [`Error::Graph`]; the output is then silence.
pub(crate) fn run(
    runner: &mut Runner,
    output: NodeId,
    tracks: &mut [TrackFeeds],
    playing: bool,
    mono: bool,
    left: &mut [f32],
    right: &mut [f32],
) -> Result<(), Error> {
    let frames = left.len();
    if right.len() != frames {
        left.fill(0.0);
        right.fill(0.0);
        return Err(Error::BufferLength {
            left: frames,
            right: right.len(),
        });
    }
    if let Err(error) = runner.begin_block(frames) {
        left.fill(0.0);
        right.fill(0.0);
        return Err(error.into());
    }
    for track in tracks.iter_mut() {
        let Some(block) = runner.source_mut(track.source) else {
            continue;
        };
        for channel in 0..block.channels() {
            let Some(samples) = block.channel_mut(channel) else {
                continue;
            };
            match track.feeds.get_mut(channel) {
                Some(feed) if playing => {
                    feed.read(samples);
                }
                _ => samples.fill(0.0),
            }
        }
    }
    runner.process();
    match runner.sink(output) {
        Some(block) => {
            let first = block.channel(0).unwrap_or(&[]);
            let second = block.channel(1).unwrap_or(first);
            copy_or_silence(left, first);
            copy_or_silence(right, second);
        }
        None => {
            left.fill(0.0);
            right.fill(0.0);
        }
    }
    if mono {
        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
            let m = (*l + *r) * FOLD_DOWN_GAIN;
            *l = m;
            *r = m;
        }
    }
    Ok(())
}

/// Copies `from` into `to`, padding with silence.
fn copy_or_silence(to: &mut [f32], from: &[f32]) {
    let n = to.len().min(from.len());
    to[..n].copy_from_slice(&from[..n]);
    to[n..].fill(0.0);
}
