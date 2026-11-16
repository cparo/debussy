//! MIDI tracks' clips flattened onto the timeline for the engine: windows, offsets, overlap.

use std::error::Error;
use std::sync::Arc;

use audio_buffers::Layout;
use audio_engine::{ControlPoint, MidiTrack, NoteSpan};
use engine_bridge::{Bridge, midi_tracks, render_graph};
use project_model::{Clip, Control, Note, Sequence, Session, Source, TrackId};
use signal_graph::Config;

fn config() -> Config {
    Config::new(48_000, 256)
}

fn note(start: u64, length: u64, pitch: u8) -> Note {
    Note {
        start,
        length,
        channel: 0,
        pitch,
        velocity: 100,
    }
}

fn span(start: u64, end: u64, pitch: u8) -> NoteSpan {
    NoteSpan {
        start,
        end,
        channel: 0,
        pitch,
        velocity: 100,
    }
}

fn control(frame: u64, value: u8) -> Control {
    Control {
        frame,
        channel: 0,
        controller: 7,
        value,
    }
}

/// A sequence 1000 frames long.
fn sequence(notes: Vec<Note>, controls: Vec<Control>) -> Result<Source, Box<dyn Error>> {
    Ok(Source::Midi(Arc::new(Sequence::new(
        1000, notes, controls,
    )?)))
}

/// A project of one MIDI track holding `clips`, bottom first, and its one flattened track.
fn flattened(clips: Vec<Clip>) -> Result<MidiTrack, Box<dyn Error>> {
    let mut session = Session::new_project();
    let track = session.add_midi_track("keys")?;
    for clip in clips {
        session.add_clip(track, clip)?;
    }
    let (bridge, _runner) = Bridge::new(session.state(), config())?;
    let mut tracks = midi_tracks(session.state(), &bridge.event_sources());
    assert_eq!(tracks.len(), 1);
    tracks.pop().ok_or_else(|| "no track".into())
}

#[test]
fn test_midi_tracks_cut_notes_at_window_end() -> Result<(), Box<dyn Error>> {
    let source = sequence(vec![note(100, 50, 60), note(300, 400, 62)], Vec::new())?;
    let track = flattened(vec![Clip::new(source, 0, 500, 0)])?;
    assert_eq!(track.notes(), &[span(100, 150, 60), span(300, 500, 62)]);
    Ok(())
}

#[test]
fn test_midi_tracks_drop_notes_starting_before_window() -> Result<(), Box<dyn Error>> {
    let source = sequence(
        vec![note(100, 400, 60), note(300, 50, 62)],
        vec![control(150, 1), control(250, 2)],
    )?;
    let track = flattened(vec![Clip::new(source, 200, 600, 0)])?;
    assert_eq!(track.notes(), &[span(100, 150, 62)]);
    assert_eq!(
        track.controls(),
        &[ControlPoint {
            frame: 50,
            channel: 0,
            controller: 7,
            value: 2
        }]
    );
    Ok(())
}

#[test]
fn test_midi_tracks_apply_clip_offset() -> Result<(), Box<dyn Error>> {
    let source = sequence(vec![note(250, 100, 60)], vec![control(300, 9)])?;
    let track = flattened(vec![Clip::new(source, 200, 300, 5000)])?;
    assert_eq!(track.notes(), &[span(5050, 5150, 60)]);
    assert_eq!(
        track.controls().iter().map(|c| c.frame).collect::<Vec<_>>(),
        vec![5100]
    );
    Ok(())
}

#[test]
fn test_midi_tracks_top_clip_hides_lower() -> Result<(), Box<dyn Error>> {
    // The lower clip has a note every 100 frames over 0..1000; the upper covers 300..600.
    let lower = sequence(
        (0..10).map(|i| note(i * 100, 80, 40 + i as u8)).collect(),
        Vec::new(),
    )?;
    let upper = sequence(vec![note(0, 500, 90)], Vec::new())?;
    let track = flattened(vec![
        Clip::new(lower, 0, 1000, 0),
        Clip::new(upper, 0, 300, 300),
    ])?;
    let starts: Vec<_> = track
        .notes()
        .iter()
        .map(|n| (n.start, n.end, n.pitch))
        .collect();
    assert_eq!(
        starts,
        vec![
            (0, 80, 40),
            (100, 180, 41),
            (200, 280, 42),
            (300, 600, 90),
            (600, 680, 46),
            (700, 780, 47),
            (800, 880, 48),
            (900, 980, 49),
        ]
    );
    Ok(())
}

#[test]
fn test_render_graph_lists_event_sources() -> Result<(), Box<dyn Error>> {
    let mut session = Session::new_project();
    let audio = session.add_track("audio", Layout::Mono)?;
    let midi: TrackId = session.add_midi_track("keys")?;
    let graph = render_graph(session.state(), config())?;
    assert_eq!(
        graph.event_sources.keys().copied().collect::<Vec<_>>(),
        vec![midi]
    );
    assert!(graph.sources.contains_key(&audio));
    assert!(!graph.sources.contains_key(&midi));
    let tracks = midi_tracks(session.state(), &graph.event_sources);
    assert_eq!(tracks.len(), 1);
    assert_eq!(
        Some(tracks[0].source()),
        graph.event_sources.get(&midi).copied()
    );
    Ok(())
}
