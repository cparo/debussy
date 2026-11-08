//! The interface's update logic with fake services and the real bridge and engine, without a
//! window: gestures and undo steps, mute and solo, transport, undo while playing, zoom and
//! waveform lanes.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::error::Error;
use std::rc::Rc;

use audio_analysis::Bucket;
use audio_buffers::Layout;
use audio_effects::{Control, Mode};
use audio_engine::{Engine, Processor, Streams, TrackFeeds, TransportState};
use debussy_ui::{App, DeviceChoice, ImportedTrack, Message, SPECTRUM_BANDS, Services, Spectrum};
use engine_bridge::Bridge;
use project_model::{AudioFile, Clip, NodeId, NodeKind, Session, Source, State, TrackId};
use signal_graph::{Config, NodeId as GraphNodeId};

/// What the fake services saw.
#[derive(Default)]
struct Calls {
    track_feeds: usize,
    saves: usize,
    waveform: Vec<(String, u64, u64)>,
    spectrogram: Vec<(String, u64, u64)>,
    exports: Vec<bool>,
    device: Option<String>,
}

struct NoStreams;

impl Streams for NoStreams {
    fn seek(&mut self, _frame: u64) -> Result<(), Box<dyn Error + Send + Sync>> {
        Ok(())
    }
}

struct Fake {
    calls: Rc<RefCell<Calls>>,
    streams: NoStreams,
}

impl Services for Fake {
    fn streams(&mut self) -> &mut dyn Streams {
        &mut self.streams
    }

    fn track_feeds(
        &mut self,
        _state: &State,
        _sources: &BTreeMap<TrackId, GraphNodeId>,
    ) -> Result<Vec<TrackFeeds>, String> {
        self.calls.borrow_mut().track_feeds += 1;
        Ok(Vec::new())
    }

    fn waveform(&mut self, file: &str, start: u64, spc: u64, out: &mut [Bucket]) -> usize {
        self.calls
            .borrow_mut()
            .waveform
            .push((file.to_owned(), start, spc));
        out.fill(Bucket {
            min: -0.5,
            max: 0.5,
            rms: 0.25,
        });
        out.len()
    }

    fn spectrogram(&mut self, file: &str, start: u64, spc: u64, out: &mut [Spectrum]) -> usize {
        self.calls
            .borrow_mut()
            .spectrogram
            .push((file.to_owned(), start, spc));
        out.fill([-30.0; SPECTRUM_BANDS]);
        out.len()
    }

    fn save(&mut self, _session: &Session) -> Result<(), String> {
        self.calls.borrow_mut().saves += 1;
        Ok(())
    }

    fn import(&mut self) -> Result<Vec<ImportedTrack>, String> {
        let file = |name: &str| AudioFile {
            name: name.into(),
            frames: 4800,
        };
        Ok(vec![
            ImportedTrack {
                name: "Keys".into(),
                layout: Layout::Stereo,
                source: Source::Stereo {
                    left: file("keys L.wav"),
                    right: file("keys R.wav"),
                },
                frames: 4800,
            },
            ImportedTrack {
                name: "Bass".into(),
                layout: Layout::Mono,
                source: Source::Mono(file("bass.wav")),
                frames: 4800,
            },
        ])
    }

    fn export(&mut self, _state: &State, mono: bool) -> Result<Option<String>, String> {
        self.calls.borrow_mut().exports.push(mono);
        Ok(Some("mix.wav".into()))
    }

    fn devices(&mut self) -> (Vec<DeviceChoice>, Option<String>) {
        let choices = vec![
            DeviceChoice {
                id: "a".into(),
                name: "Built-in".into(),
            },
            DeviceChoice {
                id: "b".into(),
                name: "Interface".into(),
            },
        ];
        (
            choices,
            Some(
                self.calls
                    .borrow()
                    .device
                    .clone()
                    .unwrap_or_else(|| "a".into()),
            ),
        )
    }

    fn audio_error(&mut self) -> Option<String> {
        None
    }

    fn set_device(&mut self, id: &str) -> Result<(), String> {
        self.calls.borrow_mut().device = Some(id.into());
        Ok(())
    }
}

/// A project with one mono track at frame 4096 for 65536 frames, routed through a 1-channel
/// mixer, and the app over it.
/// The app, the engine's real-time side, what the fake services saw, and the mixer's id.
type Fixture = (App, Processor, Rc<RefCell<Calls>>, NodeId);

fn app() -> Result<Fixture, Box<dyn Error>> {
    let mut session = Session::new_project();
    let track = session.add_track("Vocal", Layout::Mono)?;
    let file = AudioFile {
        name: "vox.wav".into(),
        frames: 100_000,
    };
    session.add_clip(track, Clip::new(Source::Mono(file), 0, 65_536, 4096))?;
    let mixer = session.state().project_mixer().ok_or("no project mixer")?;
    let session = Session::open(session.state().clone(), BTreeMap::new());
    let (bridge, runner) = Bridge::new(session.state(), Config::new(48_000, 512))?;
    let output = bridge.output().ok_or("no output")?;
    let (engine, processor) = Engine::new(runner, output, 48_000)?;
    let calls = Rc::new(RefCell::new(Calls::default()));
    let services = Box::new(Fake {
        calls: Rc::clone(&calls),
        streams: NoStreams,
    });
    Ok((
        App::new(session, 48_000, bridge, engine, services),
        processor,
        calls,
        mixer,
    ))
}

fn block(processor: &mut Processor) -> Result<(), Box<dyn Error>> {
    let (mut l, mut r) = ([0.0; 256], [0.0; 256]);
    processor.process(&mut l, &mut r)?;
    Ok(())
}

#[test]
fn test_slider_drag_is_live_and_release_records_one_step() -> Result<(), Box<dyn Error>> {
    let (mut app, _processor, _, mixer) = app()?;
    assert!(!app.session().can_undo());
    for value in [-1.0, -3.0, -7.5] {
        app.update(Message::Slide {
            node: mixer,
            channel: 0,
            control: Control::Gain,
            value,
        });
    }
    // Live in the graph and shown, but not yet an undo step or in the model.
    assert_eq!(
        app.bridge()
            .parameter(mixer, "channel 1 gain")
            .map(|p| p.get()),
        Some(-7.5)
    );
    assert_eq!(app.mixer_value(mixer, 0, Control::Gain), -7.5);
    assert!(!app.session().can_undo());
    app.update(Message::Release);
    assert!(app.session().can_undo());
    assert_eq!(app.mixer_value(mixer, 0, Control::Gain), -7.5);
    app.update(Message::Undo);
    assert!(!app.session().can_undo(), "the drag was one step");
    assert_eq!(app.mixer_value(mixer, 0, Control::Gain), 0.0);
    assert_eq!(
        app.bridge()
            .parameter(mixer, "channel 1 gain")
            .map(|p| p.get()),
        Some(0.0)
    );
    Ok(())
}

#[test]
fn test_mute_and_solo_are_steps() -> Result<(), Box<dyn Error>> {
    let (mut app, _processor, _, mixer) = app()?;
    app.update(Message::SetMode {
        node: mixer,
        channel: 0,
        mode: Mode::Mute,
    });
    assert_eq!(
        Mode::from_value(app.mixer_value(mixer, 0, Control::Mode)),
        Mode::Mute
    );
    app.update(Message::SetMode {
        node: mixer,
        channel: 0,
        mode: Mode::Solo,
    });
    assert_eq!(
        Mode::from_value(app.mixer_value(mixer, 0, Control::Mode)),
        Mode::Solo
    );
    app.update(Message::Undo);
    assert_eq!(
        Mode::from_value(app.mixer_value(mixer, 0, Control::Mode)),
        Mode::Mute
    );
    Ok(())
}

#[test]
fn test_transport_loads_feeds_once_and_follows_the_engine() -> Result<(), Box<dyn Error>> {
    let (mut app, mut processor, calls, _) = app()?;
    app.update(Message::Play);
    block(&mut processor)?;
    assert_eq!(app.engine().real_time_state(), TransportState::Playing);
    assert!(app.wants_ticks());
    // Undo is refused while playing.
    app.update(Message::Undo);
    assert!(app.status().is_some());
    app.update(Message::Tick);
    app.update(Message::Pause);
    block(&mut processor)?;
    assert_eq!(app.engine().real_time_state(), TransportState::Paused);
    app.update(Message::Play);
    app.update(Message::Stop);
    block(&mut processor)?;
    assert_eq!(app.engine().real_time_state(), TransportState::Stopped);
    // The clips did not change, so the feeds were made once.
    assert_eq!(calls.borrow().track_feeds, 1);
    app.update(Message::Save);
    assert_eq!(calls.borrow().saves, 1);
    Ok(())
}

#[test]
fn test_zoom_and_lanes_follow_the_clips() -> Result<(), Box<dyn Error>> {
    let (mut app, _processor, calls, _) = app()?;
    for _ in 0..40 {
        app.update(Message::ZoomIn);
    }
    // Clamped at the summaries' finest bucket.
    let last = calls
        .borrow()
        .waveform
        .last()
        .cloned()
        .ok_or("no waveform reads")?;
    assert_eq!(last.2, 64);
    // At 64 frames per column the clip starts at column 64 (frame 4096) and reads from the
    // file's start.
    assert_eq!(last, ("vox.wav".to_owned(), 0, 64));
    for _ in 0..40 {
        app.update(Message::ZoomOut);
    }
    let last = calls
        .borrow()
        .waveform
        .last()
        .cloned()
        .ok_or("no waveform reads")?;
    assert_eq!(last.2, debussy_ui::MAX_SAMPLES_PER_COLUMN);
    app.update(Message::ZoomIn);
    // Scrolling forward past the clip's start reads from inside the file.
    for _ in 0..30 {
        app.update(Message::ZoomIn);
    }
    app.update(Message::Pan(0.1));
    let last = calls
        .borrow()
        .waveform
        .last()
        .cloned()
        .ok_or("no waveform reads")?;
    assert_eq!(last.0, "vox.wav");
    assert!(last.1 > 0);
    // The view builds.
    let _ = app.view();
    Ok(())
}

#[test]
fn test_import_adds_one_step_per_file_onto_the_mixer() -> Result<(), Box<dyn Error>> {
    let (mut app, _processor, _, mixer) = app()?;
    app.update(Message::Import);
    let state = app.session().state();
    assert_eq!(state.tracks.len(), 3);
    // Vocal on channel 1, Keys L/R on 2 and 3, Bass on 4.
    assert!(matches!(
        state.node(mixer).map(|n| &n.kind),
        Some(NodeKind::Mixer { channels: 4, .. })
    ));
    assert_eq!(state.tracks[1].clips.len(), 1);
    app.update(Message::Undo);
    assert_eq!(
        app.session().state().tracks.len(),
        2,
        "each file is one step"
    );
    let _ = app.view();
    Ok(())
}

#[test]
fn test_export_and_device_choice() -> Result<(), Box<dyn Error>> {
    let (mut app, mut processor, calls, _) = app()?;
    app.update(Message::Export(true));
    assert_eq!(calls.borrow().exports, vec![true]);
    assert_eq!(app.status(), Some("Exported mix.wav"));
    let interface = DeviceChoice {
        id: "b".into(),
        name: "Interface".into(),
    };
    app.update(Message::Play);
    block(&mut processor)?;
    app.update(Message::Device(interface.clone()));
    assert_eq!(calls.borrow().device, None, "no switching while playing");
    app.update(Message::Stop);
    block(&mut processor)?;
    app.update(Message::Device(interface));
    assert_eq!(calls.borrow().device.as_deref(), Some("b"));
    Ok(())
}

#[test]
fn test_seek_sets_the_return_point_and_stop_returns_there() -> Result<(), Box<dyn Error>> {
    let (mut app, mut processor, _, _) = app()?;
    app.update(Message::Seek(9600));
    block(&mut processor)?;
    assert_eq!(app.return_point(), 9600);
    assert_eq!(app.engine().playhead(), 9600);
    app.update(Message::Play);
    for _ in 0..8 {
        block(&mut processor)?;
    }
    // Seeking while playing plays on from there, and stop returns there.
    app.update(Message::Seek(48_000));
    for _ in 0..8 {
        block(&mut processor)?;
    }
    assert_eq!(app.engine().real_time_state(), TransportState::Playing);
    assert!(app.engine().playhead() > 48_000);
    assert_eq!(app.return_point(), 48_000);
    // Pausing and playing again does not move the return point.
    app.update(Message::Pause);
    block(&mut processor)?;
    app.update(Message::Play);
    for _ in 0..4 {
        block(&mut processor)?;
    }
    app.update(Message::Stop);
    block(&mut processor)?;
    assert_eq!(app.engine().real_time_state(), TransportState::Stopped);
    assert_eq!(app.engine().playhead(), 48_000);
    Ok(())
}

#[test]
fn test_seek_out_of_view_brings_it_into_view() -> Result<(), Box<dyn Error>> {
    let (mut app, _processor, _, _) = app()?;
    let far = app.view_span() * 10;
    app.update(Message::Seek(far));
    assert!(app.view_start() <= far && far < app.view_start() + app.view_span());
    Ok(())
}

#[test]
fn test_two_zoom_steps_halve_or_double_the_time_shown() -> Result<(), Box<dyn Error>> {
    let (mut app, _processor, _, _) = app()?;
    let spc = app.samples_per_column();
    app.update(Message::ZoomIn);
    let step = app.samples_per_column();
    assert!(step < spc && step > spc / 2);
    app.update(Message::ZoomIn);
    assert_eq!(app.samples_per_column(), spc / 2);
    for _ in 0..4 {
        app.update(Message::ZoomOut);
    }
    assert_eq!(app.samples_per_column(), spc * 2);
    Ok(())
}

#[test]
fn test_zoom_keeps_the_view_centre_when_the_cursor_is_out_of_view() -> Result<(), Box<dyn Error>> {
    let (mut app, mut processor, _, _) = app()?;
    // Seek ahead, then pan away so the cursor is out of view.
    app.update(Message::Seek(2_000_000));
    block(&mut processor)?;
    app.update(Message::Pan(-3.0));
    let centre = app.view_start() + app.view_span() / 2;
    app.update(Message::ZoomIn);
    app.update(Message::ZoomIn);
    let after = app.view_start() + app.view_span() / 2;
    assert!(centre.abs_diff(after) <= app.samples_per_column());
    // With the cursor in view, it stays at the same place on screen.
    app.update(Message::Seek(app.view_start() + app.view_span() / 4));
    block(&mut processor)?;
    let at = app.engine().playhead();
    let before = (at - app.view_start()) as f64 / app.view_span() as f64;
    app.update(Message::ZoomOut);
    let after = (at - app.view_start()) as f64 / app.view_span() as f64;
    assert!((before - after).abs() < 0.01);
    Ok(())
}

#[test]
fn test_spectrogram_display_reads_spectra_per_channel() -> Result<(), Box<dyn Error>> {
    let (mut app, _processor, calls, _) = app()?;
    app.update(Message::SetDisplay(debussy_ui::Display::Spectrogram));
    assert_eq!(app.display(), debussy_ui::Display::Spectrogram);
    let reads = calls.borrow().spectrogram.clone();
    assert_eq!(reads.len(), 1);
    // Spectrogram columns are four waveform columns wide.
    assert_eq!(reads[0].2, 4 * app.samples_per_column());
    let _ = app.view();
    Ok(())
}

#[test]
fn test_tone_toggles_and_follows_the_system() -> Result<(), Box<dyn Error>> {
    let (mut app, _processor, _, _) = app()?;
    app.update(Message::SystemTone(debussy_ui::Tone::Light));
    assert_eq!(app.tone(), debussy_ui::Tone::Light);
    app.update(Message::ToggleTone);
    assert_eq!(app.tone(), debussy_ui::Tone::Dark);
    let _ = app.view();
    Ok(())
}

#[test]
fn test_menus_open_toggle_and_close_on_a_choice() -> Result<(), Box<dyn Error>> {
    let (mut app, _processor, _, _) = app()?;
    app.update(Message::OpenMenu(debussy_ui::Menu::App));
    assert_eq!(app.menu(), Some(debussy_ui::Menu::App));
    let _ = app.view();
    app.update(Message::OpenMenu(debussy_ui::Menu::App));
    assert_eq!(app.menu(), None);
    app.update(Message::OpenMenu(debussy_ui::Menu::AddTrack));
    app.update(Message::Tick);
    assert_eq!(
        app.menu(),
        Some(debussy_ui::Menu::AddTrack),
        "ticks leave it open"
    );
    app.update(Message::AddTrack(Layout::Stereo));
    assert_eq!(app.menu(), None);
    Ok(())
}

#[test]
fn test_tracks_are_added_renamed_and_deleted_as_steps() -> Result<(), Box<dyn Error>> {
    let (mut app, _processor, _, _) = app()?;
    app.update(Message::AddTrack(Layout::Stereo));
    let track = app.session().state().tracks[1].clone();
    assert_eq!(track.name, "Track 2");
    assert_eq!(track.layout, Layout::Stereo);
    app.update(Message::StartRename(track.id));
    app.update(Message::RenameText("  Piano ".into()));
    app.update(Message::FinishRename);
    assert_eq!(app.session().state().tracks[1].name, "Piano");
    // Renaming to nothing, or to the same name, is not a step.
    app.update(Message::StartRename(track.id));
    app.update(Message::RenameText(String::new()));
    app.update(Message::FinishRename);
    assert_eq!(app.session().state().tracks[1].name, "Piano");
    // Doing something else finishes a rename.
    app.update(Message::StartRename(track.id));
    app.update(Message::RenameText("Keys".into()));
    app.update(Message::ZoomIn);
    assert_eq!(app.session().state().tracks[1].name, "Keys");
    app.update(Message::DeleteTrack(track.id));
    assert_eq!(app.session().state().tracks.len(), 1);
    // Undo the delete, then the rename to Keys.
    for _ in 0..2 {
        app.update(Message::Undo);
    }
    assert_eq!(app.session().state().tracks[1].name, "Piano");
    Ok(())
}

#[test]
fn test_import_into_a_track_takes_files_of_its_layout() -> Result<(), Box<dyn Error>> {
    let (mut app, mut processor, _, _) = app()?;
    let vocal = app.session().state().tracks[0].id;
    app.update(Message::Seek(100_000));
    block(&mut processor)?;
    app.update(Message::ImportInto(vocal));
    // The fake imports a stereo file, refused on a mono track, then a mono one.
    let track = &app.session().state().tracks[0];
    assert_eq!(app.session().state().tracks.len(), 1);
    assert_eq!(track.clips.len(), 2);
    assert_eq!(track.clips[1].1.position, 100_000);
    assert!(app.status().is_some_and(|s| s.starts_with("Keys")));
    Ok(())
}

#[test]
fn test_track_edits_wait_for_the_transport_to_stop() -> Result<(), Box<dyn Error>> {
    let (mut app, mut processor, _, _) = app()?;
    let vocal = app.session().state().tracks[0].id;
    app.update(Message::Play);
    block(&mut processor)?;
    app.update(Message::AddTrack(Layout::Mono));
    app.update(Message::DeleteTrack(vocal));
    app.update(Message::ImportInto(vocal));
    assert_eq!(app.session().state().tracks.len(), 1);
    assert!(!app.session().can_undo());
    assert!(app.status().is_some());
    Ok(())
}

#[test]
fn test_divider_is_clamped() -> Result<(), Box<dyn Error>> {
    let (mut app, _processor, _, _) = app()?;
    assert_eq!(app.split(), 0.6);
    app.update(Message::Split(0.01));
    assert!(app.split() >= 0.2);
    app.update(Message::Split(0.99));
    assert!(app.split() <= 0.85);
    Ok(())
}

#[test]
fn test_board_ports_line_up_with_the_lanes() -> Result<(), Box<dyn Error>> {
    let (mut app, _processor, _, _) = app()?;
    app.update(Message::AddTrack(Layout::Stereo));
    let centres = app.lane_centres();
    // A 26 px header over 128 px lanes: the vocal's one lane, then the new track's two.
    assert_eq!(centres[0].1, vec![90.0]);
    assert_eq!(centres[1].1, vec![244.0, 372.0]);
    let board = app.board(iced::Size::new(400.0, 300.0));
    let gutter: Vec<f32> = board
        .pins
        .iter()
        .filter(|(p, _)| matches!(p.endpoint, project_model::Endpoint::Track(_)))
        .map(|(_, at)| at.y)
        .collect();
    assert_eq!(gutter.len(), 3);
    assert!(gutter.iter().all(|y| [90.0, 244.0, 372.0].contains(y)));
    // Three wires into the mixer, two out to L and R.
    assert_eq!(board.wires.len(), 5);
    Ok(())
}
