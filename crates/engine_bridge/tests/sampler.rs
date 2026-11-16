//! A sampler's zones loaded from the bundle's audio files and played from a MIDI clip
//! (`docs/architecture/instruments.md` "Sampler", ADR-0154).

use std::collections::BTreeMap;
use std::error::Error;
use std::fs::File;
use std::path::PathBuf;
use std::sync::Arc;

use audio_engine::render;
use engine_bridge::{Bridge, Samples, midi_tracks, render_graph};
use project_model::{
    AudioFile, Clip, Endpoint, NodeKind, Note, SAMPLER_KIND, SampleZone, Sequence, Session, Source,
    ZoneMode,
};
use signal_graph::Config;
use wav_files::{Channels, SampleFormat, Spec, WavWriter};

const RATE: u32 = 48_000;

/// An `audio/` directory holding `kick.wav`, 2000 frames of 0.5.
fn audio_dir(name: &str) -> Result<PathBuf, Box<dyn Error>> {
    let dir = std::env::temp_dir().join(format!(
        "engine-bridge-sampler-{}-{name}",
        std::process::id()
    ));
    if dir.exists() {
        std::fs::remove_dir_all(&dir)?;
    }
    std::fs::create_dir_all(&dir)?;
    let spec = Spec {
        channels: Channels::Mono,
        sample_rate: RATE,
        format: SampleFormat::Float32,
    };
    let mut writer = WavWriter::new(File::create(dir.join("kick.wav"))?, spec)?;
    writer.write(&[&[0.5; 2_000]])?;
    writer.finish()?;
    Ok(dir)
}

/// A MIDI track playing key 36 from frame 200, into a sampler with one zone on key 36 playing
/// `file`, into the output.
fn session(file: &str) -> Result<Session, Box<dyn Error>> {
    let mut session = Session::new();
    let keys = session.add_midi_track("drums")?;
    let note = Note {
        start: 200,
        length: 400,
        channel: 0,
        pitch: 36,
        velocity: 127,
    };
    let sequence = Arc::new(Sequence::new(1_000, vec![note], Vec::new())?);
    session.add_clip(keys, Clip::new(Source::Midi(sequence), 0, 1_000, 0))?;
    let sampler = session.add_node(NodeKind::Instrument {
        kind: SAMPLER_KIND.to_owned(),
        parameters: BTreeMap::new(),
        zones: vec![SampleZone {
            file: Source::Mono(AudioFile {
                name: file.to_owned(),
                frames: 2_000,
            }),
            low: 36,
            high: 36,
            root: 36,
            mode: ZoneMode::OneShot,
            choke: None,
        }],
    })?;
    session.connect(Endpoint::Track(keys), 0, Endpoint::Node(sampler), 0)?;
    session.connect(Endpoint::Node(sampler), 0, Endpoint::Output, 0)?;
    Ok(session)
}

/// The left channel of `session` rendered for 1000 frames through `samples`.
fn play(session: &Session, samples: &mut Samples) -> Result<Vec<f32>, Box<dyn Error>> {
    let mut graph = render_graph(session.state(), Config::new(RATE, 256), samples)?;
    let mut midi = midi_tracks(session.state(), &graph.event_sources);
    let mut left = Vec::new();
    render(
        &mut graph.runner,
        graph.output,
        &mut [],
        &mut midi,
        0,
        1_000,
        128,
        &mut |l, _| {
            left.extend_from_slice(l);
            Ok(())
        },
    )?;
    Ok(left)
}

#[test]
fn test_sampler_zone_plays_from_the_bundle_audio() -> Result<(), Box<dyn Error>> {
    let dir = audio_dir("plays")?;
    let left = play(&session("kick.wav")?, &mut Samples::new(&dir))?;
    assert!(
        left[..200].iter().all(|&s| s == 0.0),
        "sound before the note"
    );
    assert!(
        left[300..].iter().any(|s| s.abs() > 0.1),
        "the zone is silent"
    );
    Ok(())
}

#[test]
fn test_sampler_zone_with_missing_file_is_silent_with_a_warning() -> Result<(), Box<dyn Error>> {
    let dir = audio_dir("missing")?;
    let mut samples = Samples::new(&dir);
    let left = play(&session("gone.wav")?, &mut samples)?;
    assert!(left.iter().all(|&s| s == 0.0));
    assert_eq!(samples.take_warnings().len(), 1);
    // The live bridge reports it too.
    let (mut bridge, _runner) = Bridge::new(
        session("gone.wav")?.state(),
        Config::new(RATE, 256),
        Samples::new(&dir),
    )?;
    assert_eq!(bridge.take_warnings().len(), 1);
    Ok(())
}
