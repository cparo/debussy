//! MIDI tracks as the engine plays them: each track's clips flattened onto the timeline as an
//! `audio_engine` [`MidiTrack`] (`docs/architecture/engine.md` "MIDI clip playback", ADR-0145).

use std::collections::BTreeMap;

use audio_engine::{ControlPoint, MidiTrack, NoteSpan};
use project_model::{Clip, Source, State, Track, TrackId};
use signal_graph::NodeId as GraphNodeId;

/// A stretch of the timeline, `start..end`.
type Stretch = (u64, u64);

/// The stretches of `clips[index]` that no higher clip covers, in timeline order.
fn visible(clips: &[(project_model::ClipId, Clip)], index: usize) -> Vec<Stretch> {
    let (_, clip) = &clips[index];
    let mut stretches = vec![(clip.position, clip.end())];
    for (_, above) in &clips[index + 1..] {
        let (cover_start, cover_end) = (above.position, above.end());
        stretches = stretches
            .into_iter()
            .flat_map(|(start, end)| {
                let before = (start, end.min(cover_start));
                let after = (start.max(cover_end), end);
                [before, after].into_iter().filter(|(s, e)| s < e)
            })
            .collect();
    }
    stretches
}

/// The notes and control changes `track`'s clips play, in timeline frames. Where clips overlap,
/// the higher plays; a note plays if it starts inside a stretch of its clip that shows, and is cut
/// off at that stretch's end (ADR-0144).
fn flatten(track: &Track) -> (Vec<NoteSpan>, Vec<ControlPoint>) {
    let mut notes = Vec::new();
    let mut controls = Vec::new();
    for (index, (_, clip)) in track.clips.iter().enumerate() {
        let Source::Midi(sequence) = &clip.source else {
            // The model keeps audio clips off MIDI tracks.
            continue;
        };
        // Timeline frame of sequence frame `t`, for `t` at or after the clip's offset.
        let timeline = |t: u64| clip.position.saturating_add(t - clip.offset);
        for (start, end) in visible(&track.clips, index) {
            for note in sequence.notes() {
                if note.start < clip.offset {
                    continue;
                }
                let at = timeline(note.start);
                if at < start || at >= end {
                    continue;
                }
                notes.push(NoteSpan {
                    start: at,
                    end: at.saturating_add(note.length).min(end),
                    channel: note.channel,
                    pitch: note.pitch,
                    velocity: note.velocity,
                });
            }
            for control in sequence.controls() {
                if control.frame < clip.offset {
                    continue;
                }
                let at = timeline(control.frame);
                if at < start || at >= end {
                    continue;
                }
                controls.push(ControlPoint {
                    frame: at,
                    channel: control.channel,
                    controller: control.controller,
                    value: control.value,
                });
            }
        }
    }
    (notes, controls)
}

/// A player for every MIDI track of `state` that has an event source in `event_sources`, its
/// clips flattened onto the timeline. Hand them to `Engine::set_midi_tracks` for playback, or to
/// `audio_engine::render` for export.
pub fn midi_tracks(
    state: &State,
    event_sources: &BTreeMap<TrackId, GraphNodeId>,
) -> Vec<MidiTrack> {
    state
        .tracks
        .iter()
        .filter(|track| track.layout().is_none())
        .filter_map(|track| {
            let &source = event_sources.get(&track.id)?;
            let (notes, controls) = flatten(track);
            Some(MidiTrack::new(source, notes, controls))
        })
        .collect()
}
