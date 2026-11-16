//! The per-block step that playback and offline render share, so both produce the same samples.

use dsp::FOLD_DOWN_GAIN;
use signal_graph::{NodeId, Runner};

use crate::capture::{ArmedMidi, ArmedTrack, HeldNotes, Input, replaces_clips};
use crate::error::Error;
use crate::feed::TrackFeeds;
use crate::midi::MidiTrack;

/// The live side of a block: armed tracks, the device input and live MIDI input. Offline render
/// has none.
pub(crate) struct Live<'a> {
    pub(crate) armed: &'a mut [ArmedTrack],
    pub(crate) input: &'a Input,
    /// Armed MIDI tracks.
    pub(crate) midi: &'a mut [ArmedMidi],
    /// Live notes of tracks no longer armed, still to be released, by event source.
    pub(crate) releasing: &'a mut Vec<(NodeId, HeldNotes)>,
    /// The host time of the block's callback, in nanoseconds, and the sample rate, for
    /// stamping live MIDI input.
    pub(crate) host_nanos: u64,
    pub(crate) sample_rate: u32,
    /// Whether recording (counting in, too): armed tracks without monitor are silent, and armed
    /// MIDI tracks' clips are muted.
    pub(crate) recording: bool,
    /// Whether this block's input goes to the armed tracks' takes.
    pub(crate) capturing: bool,
    /// The take frame of the block's first frame, while capturing.
    pub(crate) capture_frame: u64,
    /// Live MIDI events the event sources refused in this block; set by [`run`].
    pub(crate) overflows: u64,
}

/// Runs one block of `left.len()` frames: fills each track's source from its feeds (or with
/// silence unless `playing`), each MIDI track's event source with its events (see
/// [`MidiTrack::fill`]; an armed MIDI track's clips are muted while recording), puts armed
/// tracks' input and live MIDI input in their sources and takes (see [`Live`]), runs the plan with the sources at timeline frame `position` (where curves are
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
    midi: &mut [MidiTrack],
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
    let midi_position = u64::try_from(position).unwrap_or(0);
    for track in midi.iter_mut() {
        let muted = live
            .as_deref()
            .is_some_and(|l| l.recording && l.midi.iter().any(|m| m.source() == track.source()));
        if let Some(events) = runner.event_source_mut(track.source()) {
            // At most the graph's largest block, so it fits.
            track.fill(events, midi_position, frames as u32, playing && !muted);
        }
    }
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
        feed_midi(runner, live, frames);
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

/// Sends the note-offs still owed for tracks no longer armed, then drains each armed MIDI
/// track's live input into its event source if monitored and its take if capturing.
fn feed_midi(runner: &mut Runner, live: &mut Live<'_>, frames: usize) {
    let mut refused = 0;
    live.releasing.retain_mut(|(source, held)| {
        // A source gone from the plan has nothing left sounding.
        let Some(events) = runner.event_source_mut(*source) else {
            return false;
        };
        refused += held.release(events);
        !held.is_empty()
    });
    let capture = live.capturing.then_some(live.capture_frame);
    for track in live.midi.iter_mut() {
        let events = runner.event_source_mut(track.source());
        // At most the graph's largest block, so it fits.
        refused += track.feed(
            events,
            live.host_nanos,
            frames as u32,
            live.sample_rate,
            capture,
        );
    }
    live.overflows += refused;
}

/// Copies `from` into `to`, padding with silence.
fn copy_or_silence(to: &mut [f32], from: &[f32]) {
    let n = to.len().min(from.len());
    to[..n].copy_from_slice(&from[..n]);
    to[n..].fill(0.0);
}
