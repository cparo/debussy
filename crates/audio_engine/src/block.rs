//! The per-block step that playback and offline render share, so both produce the same samples.

use dsp::FOLD_DOWN_GAIN;
use signal_graph::{NodeId, Runner};

use crate::capture::{ArmedTrack, Input, replaces_clips};
use crate::error::Error;
use crate::feed::TrackFeeds;

/// The live side of a block: armed tracks and the device input. Offline render has none.
pub(crate) struct Live<'a> {
    pub(crate) armed: &'a mut [ArmedTrack],
    pub(crate) input: &'a Input,
    /// Whether recording: armed tracks without monitor are silent.
    pub(crate) recording: bool,
    /// Whether this block's input goes to the armed tracks' takes.
    pub(crate) capturing: bool,
}

/// Runs one block of `left.len()` frames: fills each track's source from its feeds (or with
/// silence unless `playing`), puts armed tracks' input in their sources and takes (see
/// [`Live`]), runs the plan with the sources at timeline frame `position` (where curves are
/// read), and writes the output sink to `left` and `right`, folded to mono on both if `mono`.
/// Real-time safe if the feeds and sinks are.
///
/// # Errors
///
/// [`Error::BufferLength`] or [`Error::Graph`]; the output is then silence and nothing is
/// captured.
#[allow(clippy::too_many_arguments)] // Each is a separate input of the step; a struct would only rename them.
pub(crate) fn run(
    runner: &mut Runner,
    output: NodeId,
    tracks: &mut [TrackFeeds],
    live: Option<&mut Live<'_>>,
    position: i64,
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
    runner.set_transport(position, playing);
    let (armed, recording): (&[ArmedTrack], bool) = match &live {
        Some(live) => (live.armed, live.recording),
        None => (&[], false),
    };
    for track in tracks.iter_mut() {
        if replaces_clips(armed, track.source, recording) {
            continue;
        }
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
    if let Some(live) = live {
        feed_armed(runner, live, frames);
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

/// Fills armed tracks' sources with their input if monitored, or silence if recording, and
/// hands the input to their takes if capturing.
fn feed_armed(runner: &mut Runner, live: &mut Live<'_>, frames: usize) {
    for track in live.armed.iter_mut() {
        if live.capturing {
            for (sink, &channel) in track.sinks.iter_mut().zip(&track.inputs) {
                sink.write(live.input.channel(channel, frames));
            }
        }
        if !(track.monitor || live.recording) {
            continue;
        }
        let Some(block) = runner.source_mut(track.source) else {
            continue;
        };
        for channel in 0..block.channels() {
            let Some(samples) = block.channel_mut(channel) else {
                continue;
            };
            match track.inputs.get(channel) {
                Some(&input) if track.monitor => {
                    copy_or_silence(samples, live.input.channel(input, frames));
                }
                _ => samples.fill(0.0),
            }
        }
    }
}

/// Copies `from` into `to`, padding with silence.
fn copy_or_silence(to: &mut [f32], from: &[f32]) {
    let n = to.len().min(from.len());
    to[..n].copy_from_slice(&from[..n]);
    to[n..].fill(0.0);
}
