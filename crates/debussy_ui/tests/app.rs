//! The interface's update logic with fake services and the real bridge and engine, without a
//! window: gestures and undo steps, mute and solo, transport, undo while playing, zoom and
//! waveform lanes.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::error::Error;
use std::rc::Rc;
use std::sync::{Arc, Mutex, PoisonError};

use audio_analysis::Bucket;
use audio_buffers::Layout;
use audio_effects::{Control, Mode};
use audio_engine::{Engine, Processor, Streams, TakeSink, TrackFeeds, TransportState};
use debussy_ui::{
    App, DeviceChoice, ImportedTrack, Message, SPECTRUM_BANDS, Services, Spectrum, TakeFiles,
    TakeStatus,
};
use engine_bridge::Bridge;
use project_model::{AudioFile, Clip, NodeId, NodeKind, Session, Source, State, TrackId};
use signal_graph::{Config, NodeId as GraphNodeId};

/// What the fake services saw.
#[derive(Default)]
struct Calls {
    track_feeds: usize,
    saves: usize,
    /// Whether saving fails.
    save_fails: bool,
    waveform: Vec<(String, u64, u64)>,
    spectrogram: Vec<(String, u64, u64)>,
    samples: Vec<(String, u64, usize)>,
    exports: Vec<bool>,
    device: Option<String>,
    /// Each call to open or close the device's inputs.
    input: Vec<bool>,
    /// Input channels the device has.
    inputs: usize,
    /// The files of each recording pass started.
    started: Vec<Vec<String>>,
    kept: Vec<String>,
    discarded: Vec<String>,
}

/// What the fake take sinks report when they end.
type Ended = Arc<Mutex<Vec<TakeStatus>>>;

/// A take sink that counts what it is given and reports it finished when it ends.
struct FakeSink {
    take: usize,
    frames: u64,
    ended: Option<Ended>,
}

impl TakeSink for FakeSink {
    fn write(&mut self, samples: &[f32]) -> usize {
        if self.ended.is_some() {
            self.frames += samples.len() as u64;
        }
        samples.len()
    }

    fn end(&mut self) {
        if let Some(ended) = self.ended.take() {
            ended
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(TakeStatus::Finished {
                    take: self.take,
                    frames: self.frames,
                });
        }
    }
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
    ended: Ended,
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

    fn samples(&mut self, file: &str, start: u64, out: &mut [f32]) -> usize {
        self.calls
            .borrow_mut()
            .samples
            .push((file.to_owned(), start, out.len()));
        out.fill(0.5);
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
        let mut calls = self.calls.borrow_mut();
        calls.saves += 1;
        if calls.save_fails {
            return Err("disk full".into());
        }
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

    fn set_input(&mut self, input: bool) -> Result<(), String> {
        self.calls.borrow_mut().input.push(input);
        Ok(())
    }

    fn input_channels(&mut self) -> usize {
        let calls = self.calls.borrow();
        if calls.input.last() == Some(&true) {
            calls.inputs
        } else {
            0
        }
    }

    fn device_latency(&mut self) -> u64 {
        LATENCY
    }

    fn start_takes(&mut self, tracks: &[(String, usize)]) -> Result<Vec<TakeFiles>, String> {
        let mut take = 0;
        let mut all = Vec::new();
        let takes = tracks
            .iter()
            .map(|(name, channels)| {
                let files: Vec<String> =
                    (0..*channels).map(|c| format!("{name} {c}.wav")).collect();
                all.extend(files.iter().cloned());
                let sinks = files
                    .iter()
                    .map(|_| {
                        take += 1;
                        Box::new(FakeSink {
                            take: take - 1,
                            frames: 0,
                            ended: Some(Arc::clone(&self.ended)),
                        }) as Box<dyn TakeSink>
                    })
                    .collect();
                TakeFiles { files, sinks }
            })
            .collect();
        self.calls.borrow_mut().started.push(all);
        Ok(takes)
    }

    fn take_status(&mut self) -> Vec<TakeStatus> {
        std::mem::take(&mut *self.ended.lock().unwrap_or_else(PoisonError::into_inner))
    }

    fn take_overruns(&mut self) -> u64 {
        0
    }

    fn keep_takes(&mut self, files: &[String]) -> Result<(), String> {
        self.calls.borrow_mut().kept.extend_from_slice(files);
        Ok(())
    }

    fn discard_takes(&mut self, files: &[String]) {
        self.calls.borrow_mut().discarded.extend_from_slice(files);
    }
}

/// The fake device's input plus output latency.
const LATENCY: u64 = 100;

/// A project with one mono track at frame 4096 for 65536 frames, routed through a 1-channel
/// mixer, and the app over it.
/// The app, the engine's real-time side, what the fake services saw, and the mixer's id.
type Fixture = (App, Processor, Rc<RefCell<Calls>>, NodeId);

fn app() -> Result<Fixture, Box<dyn Error>> {
    app_with_gain(0.0)
}

/// The fixture with the clip at `gain_db`.
fn app_with_gain(gain_db: f32) -> Result<Fixture, Box<dyn Error>> {
    let mut session = Session::new_project();
    let track = session.add_track("Vocal", Layout::Mono)?;
    let file = AudioFile {
        name: "vox.wav".into(),
        frames: 100_000,
    };
    let mut clip = Clip::new(Source::Mono(file), 0, 65_536, 4096);
    clip.gain_db = gain_db;
    session.add_clip(track, clip)?;
    let mixer = session.state().project_mixer().ok_or("no project mixer")?;
    let session = Session::open(session.state().clone(), BTreeMap::new());
    let (bridge, runner) = Bridge::new(session.state(), Config::new(48_000, 512))?;
    let output = bridge.output().ok_or("no output")?;
    let (engine, processor) = Engine::new(runner, output, 48_000)?;
    let calls = Rc::new(RefCell::new(Calls {
        inputs: 2,
        ..Calls::default()
    }));
    let services = Box::new(Fake {
        calls: Rc::clone(&calls),
        streams: NoStreams,
        ended: Ended::default(),
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
    processor.process(&[], 0, &mut l, &mut r)?;
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
    // Playback changes nothing to save.
    assert_eq!(calls.borrow().saves, 0);
    Ok(())
}

#[test]
fn test_zoom_and_lanes_follow_the_clips() -> Result<(), Box<dyn Error>> {
    let (mut app, _processor, calls, _) = app()?;
    // Eight steps in from 1024 frames per column: the summaries' finest bucket.
    for _ in 0..8 {
        app.update(Message::ZoomIn);
    }
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
    // Zoom out stops at the project: 69 632 frames over 1024 columns.
    let last = calls
        .borrow()
        .waveform
        .last()
        .cloned()
        .ok_or("no waveform reads")?;
    assert_eq!(last.2, 68);
    // Back on the grid, finer than 64 frames per column so the view is short enough to scroll,
    // scrolling forward past the clip's start reads samples from inside the file.
    app.update(Message::ZoomIn);
    app.update(Message::ZoomIn);
    app.update(Message::Pan(0.1));
    let last = calls
        .borrow()
        .samples
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
fn test_stop_shows_the_return_point_before_the_real_time_side_runs() -> Result<(), Box<dyn Error>> {
    let (mut app, mut processor, _, _) = app()?;
    app.update(Message::Seek(9600));
    block(&mut processor)?;
    app.update(Message::Play);
    for _ in 0..8 {
        block(&mut processor)?;
    }
    app.update(Message::Stop);
    // The redraw that follows the click reads the playhead before the audio thread runs again.
    assert_eq!(app.engine().playhead(), 9600);
    Ok(())
}

#[test]
fn test_zoom_in_stops_at_two_frames_per_column() -> Result<(), Box<dyn Error>> {
    let (mut app, _processor, calls, _) = app()?;
    for _ in 0..40 {
        app.update(Message::ZoomIn);
    }
    assert_eq!(app.samples_per_column(), 2);
    // The clip starts at frame 4096, past the lane's 2048 frames; seeking into it brings it
    // into view.
    app.update(Message::Seek(5000));
    let last = calls
        .borrow()
        .samples
        .last()
        .cloned()
        .ok_or("no sample reads")?;
    assert_eq!(last.0, "vox.wav");
    Ok(())
}

#[test]
fn test_fine_zooms_carry_the_trace_with_clip_gain() -> Result<(), Box<dyn Error>> {
    // +6.02 dB doubles the fake's 0.5 samples.
    let (mut app, _processor, calls, _) = app_with_gain(20.0 * 2f32.log10())?;
    // At 64 frames per column, the summaries: columns, no trace.
    for _ in 0..8 {
        app.update(Message::ZoomIn);
    }
    let lane = &app.lanes()[0].1[0];
    assert!(lane.trace.is_empty());
    assert!(lane.columns.iter().any(Option::is_some));
    assert!(calls.borrow().samples.is_empty());
    // One step finer, the trace: a frame per column's frame, plus the closing one.
    app.update(Message::ZoomIn);
    let spc = app.samples_per_column();
    assert!(spc < 64);
    let lane = &app.lanes()[0].1[0];
    assert!(lane.columns.is_empty());
    assert_eq!(lane.samples_per_column, spc);
    assert_eq!(
        lane.trace.len(),
        debussy_ui::LANE_COLUMNS * spc as usize + 1
    );
    // Blank before the clip's start at frame 4096, the gained audio after.
    let at =
        |frame: u64| lane.trace[((frame - app.view_start()) as usize).min(lane.trace.len() - 1)];
    assert!(app.view_start() < 4096);
    assert_eq!(at(app.view_start()), None);
    let value = at(4096).ok_or("no audio at the clip's start")?;
    assert!((value - 1.0).abs() < 1e-3, "{value}");
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
    // Ten steps in, to 32 frames per column, so the view is shorter than the project and can
    // zoom out again.
    for _ in 0..10 {
        app.update(Message::ZoomIn);
    }
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

/// The fixture's end: its clip at frame 4096 for 65 536 frames.
const PROJECT_END: u64 = 4096 + 65_536;

#[test]
fn test_zoom_out_stops_at_the_project_length() -> Result<(), Box<dyn Error>> {
    let (mut app, _processor, _, _) = app()?;
    for _ in 0..12 {
        app.update(Message::ZoomIn);
    }
    app.update(Message::Pan(1.0));
    for _ in 0..20 {
        app.update(Message::ZoomOut);
    }
    assert_eq!(app.view_start(), 0);
    assert!(app.view_span() >= PROJECT_END);
    assert!(app.view_span() < PROJECT_END + debussy_ui::LANE_COLUMNS as u64);
    let (start, spc) = (app.view_start(), app.samples_per_column());
    app.update(Message::ZoomOut);
    assert_eq!((app.view_start(), app.samples_per_column()), (start, spc));
    let _ = app.view();
    Ok(())
}

#[test]
fn test_pan_stops_at_the_project_end() -> Result<(), Box<dyn Error>> {
    let (mut app, _processor, _, _) = app()?;
    for _ in 0..12 {
        app.update(Message::ZoomIn);
    }
    // The project's last frame and the end itself, where a playhead stops, are shown.
    app.update(Message::Pan(100.0));
    assert_eq!(app.view_start() + app.view_span(), PROJECT_END + 1);
    app.update(Message::Pan(1.0));
    assert_eq!(app.view_start() + app.view_span(), PROJECT_END + 1);
    app.update(Message::Pan(-100.0));
    assert_eq!(app.view_start(), 0);
    Ok(())
}

#[test]
fn test_pan_forward_keeps_a_view_past_the_end() -> Result<(), Box<dyn Error>> {
    let (mut app, _processor, _, _) = app()?;
    app.update(Message::Seek(2_000_000));
    let start = app.view_start();
    assert!(start > PROJECT_END);
    app.update(Message::Pan(1.0));
    assert_eq!(app.view_start(), start);
    Ok(())
}

#[test]
fn test_zoom_out_near_the_end_stays_within_the_project() -> Result<(), Box<dyn Error>> {
    let (mut app, _processor, _, _) = app()?;
    for _ in 0..12 {
        app.update(Message::ZoomIn);
    }
    app.update(Message::Pan(100.0));
    app.update(Message::ZoomOut);
    assert!(app.view_start() + app.view_span() <= PROJECT_END + 1);
    Ok(())
}

#[test]
fn test_zoom_out_disabled_when_project_fits() -> Result<(), Box<dyn Error>> {
    let (mut app, _processor, calls, _) = app()?;
    let (start, spc) = (app.view_start(), app.samples_per_column());
    let reads = calls.borrow().waveform.len();
    app.update(Message::ZoomOut);
    assert_eq!((app.view_start(), app.samples_per_column()), (start, spc));
    assert_eq!(calls.borrow().waveform.len(), reads);
    Ok(())
}

#[test]
fn test_zoom_in_from_fit_returns_to_the_grid() -> Result<(), Box<dyn Error>> {
    let (mut app, _processor, _, _) = app()?;
    for _ in 0..12 {
        app.update(Message::ZoomIn);
    }
    for _ in 0..20 {
        app.update(Message::ZoomOut);
    }
    let fit = app.samples_per_column();
    app.update(Message::ZoomIn);
    // The coarsest grid level under the fit's 68: 64 frames per column.
    assert_eq!(app.samples_per_column(), 64);
    assert!(app.samples_per_column() < fit);
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
    // Each spectrogram column spans LANE_COLUMNS / SPECTRUM_COLUMNS waveform columns.
    let ratio = (debussy_ui::LANE_COLUMNS / debussy_ui::SPECTRUM_COLUMNS) as u64;
    assert_eq!(reads[0].2, ratio * app.samples_per_column());
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
fn test_rescaled_sets_scale_factor_and_ignores_nonsense() -> Result<(), Box<dyn Error>> {
    let (mut app, _processor, _, _) = app()?;
    assert_eq!(app.scale_factor(), 1.0);
    app.update(Message::Rescaled(2.0));
    assert_eq!(app.scale_factor(), 2.0);
    app.update(Message::Rescaled(0.0));
    app.update(Message::Rescaled(f32::NAN));
    assert_eq!(app.scale_factor(), 2.0);
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

fn input(node: NodeId, port: usize) -> debussy_ui::Pin {
    debussy_ui::Pin {
        endpoint: project_model::Endpoint::Node(node),
        output: false,
        port,
        side: None,
    }
}

#[test]
fn test_wires_connect_free_inputs_and_delete_as_steps() -> Result<(), Box<dyn Error>> {
    let (mut app, _processor, _, mixer) = app()?;
    let vocal = app.session().state().tracks[0].id;
    let out = debussy_ui::Pin {
        endpoint: project_model::Endpoint::Track(vocal),
        output: true,
        port: 0,
        side: None,
    };
    // Channel 1 is taken: dropping there does nothing.
    app.update(Message::StartWire(out));
    app.update(Message::FinishWire(Some(input(mixer, 0))));
    assert!(!app.session().can_undo());
    app.update(Message::AddMixerChannel(mixer));
    app.update(Message::StartWire(out));
    app.update(Message::FinishWire(Some(input(mixer, 1))));
    let state = app.session().state();
    let wire = state
        .connections
        .iter()
        .find(|c| c.to == project_model::Endpoint::Node(mixer) && c.input == 1)
        .ok_or("not connected")?
        .id;
    // Dropped nowhere: nothing.
    app.update(Message::StartWire(out));
    app.update(Message::FinishWire(None));
    let count = app.session().state().connections.len();
    app.update(Message::Select(Some(debussy_ui::Selection::Wire(wire))));
    app.update(Message::DeleteSelection);
    assert_eq!(app.session().state().connections.len(), count - 1);
    assert_eq!(app.selection(), None);
    app.update(Message::Undo);
    assert_eq!(app.session().state().connections.len(), count);
    Ok(())
}

#[test]
fn test_nodes_are_added_moved_and_deleted_as_steps() -> Result<(), Box<dyn Error>> {
    let (mut app, _processor, _, _) = app()?;
    app.update(Message::AddNode(debussy_ui::NewNode::Bus));
    let bus = app.session().state().nodes.last().ok_or("no node")?.id;
    assert_eq!(app.selection(), Some(debussy_ui::Selection::Node(bus)));
    let _ = app.view();
    // A drag shows live and records one step on drop.
    for x in [10.0, 50.0, 90.0] {
        app.update(Message::DragNode(
            bus,
            project_model::Position { x, y: 30.0 },
        ));
    }
    let board = app.board(iced::Size::new(600.0, 400.0));
    let shown = board
        .nodes
        .iter()
        .find(|n| n.id == bus)
        .ok_or("not shown")?;
    assert_eq!(shown.rect.x, 90.0);
    assert_eq!(
        app.session().state().node(bus).and_then(|n| n.position),
        None
    );
    app.update(Message::Drop);
    assert_eq!(
        app.session().state().node(bus).and_then(|n| n.position),
        Some(project_model::Position { x: 90.0, y: 30.0 })
    );
    app.update(Message::DragOutput(200.0));
    app.update(Message::Drop);
    assert_eq!(app.session().state().output_y, Some(200.0));
    // Delete is ignored while typing.
    app.update(Message::DelayText(bus, "1".into()));
    app.update(Message::DeleteSelection);
    assert!(app.session().state().node(bus).is_some());
    app.update(Message::Select(Some(debussy_ui::Selection::Node(bus))));
    app.update(Message::DeleteSelection);
    assert!(app.session().state().node(bus).is_none());
    // Undo the delete, the output move, the node move and the add.
    for _ in 0..4 {
        app.update(Message::Undo);
    }
    assert!(app.session().state().node(bus).is_none());
    assert_eq!(app.session().state().output_y, None);
    Ok(())
}

#[test]
fn test_node_dialogs_edit_mixers_and_delays() -> Result<(), Box<dyn Error>> {
    let (mut app, _processor, _, mixer) = app()?;
    app.update(Message::Select(Some(debussy_ui::Selection::Node(mixer))));
    let _ = app.view();
    // Channel 1 holds the vocal, so it cannot be removed; a new channel 2 can.
    assert_eq!(app.removable_channel(mixer), None);
    app.update(Message::AddMixerChannel(mixer));
    assert_eq!(app.removable_channel(mixer), Some(1));
    app.update(Message::RemoveMixerChannel(mixer));
    assert!(matches!(
        app.session().state().node(mixer).map(|n| &n.kind),
        Some(NodeKind::Mixer { channels: 1, .. })
    ));
    app.update(Message::AddNode(debussy_ui::NewNode::FeedbackDelay));
    let delay = app.session().state().nodes.last().ok_or("no node")?.id;
    let _ = app.view();
    app.update(Message::DelayText(delay, "0.5".into()));
    app.update(Message::CommitDelay);
    assert!(matches!(
        app.session().state().node(delay).map(|n| &n.kind),
        Some(NodeKind::FeedbackDelay { seconds, .. }) if *seconds == 0.5
    ));
    app.update(Message::DelayText(delay, "soon".into()));
    app.update(Message::CommitDelay);
    assert!(app.status().is_some());
    app.update(Message::DeleteNode(delay));
    assert!(app.session().state().node(delay).is_none());
    assert_eq!(app.selection(), None);
    Ok(())
}

/// Where the playhead falls in the view, as a fraction of its width.
fn playhead_at(app: &App) -> f64 {
    (app.engine().playhead() as f64 - app.view_start() as f64) / app.view_span() as f64
}

#[test]
fn test_tick_pages_view_when_playhead_reaches_80_percent() -> Result<(), Box<dyn Error>> {
    let (mut app, mut processor, _, _) = app()?;
    for _ in 0..40 {
        app.update(Message::ZoomIn);
    }
    app.update(Message::Play);
    block(&mut processor)?;
    app.update(Message::Tick);
    assert_eq!(app.view_start(), 0);
    // A block at a time until a tick sees the playhead at or past 80 %.
    let mut before = playhead_at(&app);
    for _ in 0..app.view_span() / 256 + 1 {
        block(&mut processor)?;
        app.update(Message::Tick);
        if app.view_start() > 0 {
            break;
        }
        assert!(playhead_at(&app) < 0.8);
        before = playhead_at(&app);
    }
    // It paged on the first tick past 80 %, not before.
    assert!(app.view_start() > 0);
    assert!(before < 0.8);
    assert!((playhead_at(&app) - 0.2).abs() < 0.01);
    Ok(())
}

#[test]
fn test_tick_does_not_follow_playhead_panned_out_of_view() -> Result<(), Box<dyn Error>> {
    let (mut app, mut processor, _, _) = app()?;
    for _ in 0..40 {
        app.update(Message::ZoomIn);
    }
    app.update(Message::Play);
    block(&mut processor)?;
    app.update(Message::Tick);
    // Ahead of the playhead.
    app.update(Message::Pan(3.0));
    let start = app.view_start();
    for _ in 0..4 {
        block(&mut processor)?;
        app.update(Message::Tick);
    }
    assert_eq!(app.view_start(), start);
    // Behind it: the playhead is past the right edge but was not followed there.
    app.update(Message::Seek(100_000));
    block(&mut processor)?;
    app.update(Message::Tick);
    app.update(Message::Pan(-3.0));
    let start = app.view_start();
    block(&mut processor)?;
    app.update(Message::Tick);
    assert_eq!(app.view_start(), start);
    Ok(())
}

#[test]
fn test_tick_follows_playhead_that_jumps_past_the_edge() -> Result<(), Box<dyn Error>> {
    let (mut app, mut processor, _, _) = app()?;
    for _ in 0..40 {
        app.update(Message::ZoomIn);
    }
    app.update(Message::Play);
    block(&mut processor)?;
    app.update(Message::Tick);
    // More audio than the lane shows between two ticks.
    let blocks = app.view_span() / 256 + 2;
    for _ in 0..blocks {
        block(&mut processor)?;
    }
    app.update(Message::Tick);
    assert!((playhead_at(&app) - 0.2).abs() < 0.01);
    Ok(())
}

#[test]
fn test_update_autosaves_after_rename() -> Result<(), Box<dyn Error>> {
    let (mut app, _processor, calls, _) = app()?;
    let track = app.session().state().tracks[0].id;
    app.update(Message::StartRename(track));
    app.update(Message::RenameText("Lead".into()));
    assert_eq!(calls.borrow().saves, 0);
    app.update(Message::FinishRename);
    assert_eq!(calls.borrow().saves, 1);
    Ok(())
}

#[test]
fn test_update_autosaves_a_rename_finished_by_quit() -> Result<(), Box<dyn Error>> {
    let (mut app, _processor, calls, _) = app()?;
    let track = app.session().state().tracks[0].id;
    app.update(Message::StartRename(track));
    app.update(Message::RenameText("Lead".into()));
    app.update(Message::Quit);
    assert_eq!(app.session().state().tracks[0].name, "Lead");
    assert_eq!(calls.borrow().saves, 1);
    Ok(())
}

#[test]
fn test_update_does_not_autosave_without_edit() -> Result<(), Box<dyn Error>> {
    let (mut app, _processor, calls, _) = app()?;
    app.update(Message::Tick);
    app.update(Message::Scrolled(10.0));
    app.update(Message::ZoomIn);
    app.update(Message::Undo);
    assert_eq!(calls.borrow().saves, 0);
    Ok(())
}

#[test]
fn test_update_autosaves_after_undo() -> Result<(), Box<dyn Error>> {
    let (mut app, _processor, calls, _) = app()?;
    app.update(Message::AddTrack(Layout::Mono));
    app.update(Message::Undo);
    app.update(Message::Redo);
    assert_eq!(calls.borrow().saves, 3);
    Ok(())
}

#[test]
fn test_update_reports_a_failed_autosave_once() -> Result<(), Box<dyn Error>> {
    let (mut app, _processor, calls, _) = app()?;
    calls.borrow_mut().save_fails = true;
    app.update(Message::AddTrack(Layout::Mono));
    assert_eq!(app.status(), Some("Autosave failed: disk full"));
    app.update(Message::Tick);
    assert_eq!(calls.borrow().saves, 1);
    // The next edit tries again.
    app.update(Message::AddTrack(Layout::Mono));
    assert_eq!(calls.borrow().saves, 2);
    Ok(())
}

/// The fixture with a second mono track holding a clip like the first's.
fn two_tracks() -> Result<(Fixture, TrackId, TrackId), Box<dyn Error>> {
    let (mut app, processor, calls, mixer) = app()?;
    app.update(Message::AddTrack(Layout::Mono));
    let (vocal, second) = {
        let tracks = &app.session().state().tracks;
        (tracks[0].id, tracks[1].id)
    };
    let clip = app.session().state().tracks[0].clips[0].1.clone();
    app.update(Message::LanePress {
        track: second,
        frame: 0,
        extend: false,
    });
    app.update(Message::LaneRelease);
    // Put a clip on the second track by pasting a copy of the first's.
    app.update(Message::LanePress {
        track: vocal,
        frame: clip.position,
        extend: false,
    });
    app.update(Message::LaneDrag {
        track: vocal,
        frame: clip.end(),
    });
    app.update(Message::LaneRelease);
    app.update(Message::Copy);
    app.update(Message::ClearRange);
    app.update(Message::LanePress {
        track: second,
        frame: clip.position,
        extend: false,
    });
    app.update(Message::LaneRelease);
    app.update(Message::Paste);
    Ok(((app, processor, calls, mixer), vocal, second))
}

#[test]
fn test_drag_selects_a_range_across_tracks() -> Result<(), Box<dyn Error>> {
    let ((mut app, _processor, _, _), vocal, second) = two_tracks()?;
    app.update(Message::LanePress {
        track: second,
        frame: 30_000,
        extend: false,
    });
    assert!(app.is_dragging());
    // A wobble smaller than a few columns is still a click.
    app.update(Message::LaneDrag {
        track: second,
        frame: 30_001,
    });
    assert_eq!(app.selected_range(), None);
    app.update(Message::LaneDrag {
        track: vocal,
        frame: 10_000,
    });
    app.update(Message::LaneRelease);
    assert!(!app.is_dragging());
    let (tracks, range) = app.selected_range().ok_or("no selection")?;
    assert_eq!(tracks, [vocal, second]);
    assert_eq!((range.start(), range.end()), (10_000, 30_000));
    // The press seeked to where the drag began.
    assert_eq!(app.return_point(), 30_000);
    // Moving without the button held changes nothing.
    app.update(Message::LaneDrag {
        track: second,
        frame: 50_000,
    });
    assert_eq!(app.selected_range().map(|(t, _)| t.len()), Some(2));
    app.update(Message::ClearRange);
    assert_eq!(app.selected_range(), None);
    Ok(())
}

#[test]
fn test_shift_press_extends_the_selection() -> Result<(), Box<dyn Error>> {
    let ((mut app, _processor, _, _), vocal, second) = two_tracks()?;
    app.update(Message::Seek(5_000));
    app.update(Message::LanePress {
        track: vocal,
        frame: 5_000,
        extend: false,
    });
    app.update(Message::LaneRelease);
    // With no selection, from the playhead on the current track.
    app.update(Message::LanePress {
        track: second,
        frame: 20_000,
        extend: true,
    });
    app.update(Message::LaneRelease);
    let (tracks, range) = app.selected_range().ok_or("no selection")?;
    assert_eq!(tracks, [vocal, second]);
    assert_eq!((range.start(), range.end()), (5_000, 20_000));
    // A shift-press does not seek; it moves the nearer edge.
    app.update(Message::LanePress {
        track: second,
        frame: 8_000,
        extend: true,
    });
    let (_, range) = app.selected_range().ok_or("no selection")?;
    assert_eq!((range.start(), range.end()), (8_000, 20_000));
    assert_eq!(app.return_point(), 5_000);
    Ok(())
}

#[test]
fn test_cut_and_paste_are_steps_onto_the_selection() -> Result<(), Box<dyn Error>> {
    let ((mut app, mut processor, _, _), vocal, second) = two_tracks()?;
    // The setup's paste put the copy on the second track.
    let pasted = app.session().state().tracks[1].clips.clone();
    assert_eq!(pasted.len(), 1);
    assert_eq!(pasted[0].1.position, 4096);

    app.update(Message::LanePress {
        track: vocal,
        frame: 10_000,
        extend: false,
    });
    app.update(Message::LaneDrag {
        track: second,
        frame: 20_000,
    });
    app.update(Message::LaneRelease);
    let before = app.session().state().clone();
    app.update(Message::Cut);
    assert_eq!(app.clipboard().map(|e| e.lanes().len()), Some(2));
    for track in &app.session().state().tracks {
        assert_eq!(track.clips.len(), 2, "split at both edges");
    }
    app.update(Message::Undo);
    assert_eq!(app.session().state(), &before);
    app.update(Message::Redo);

    // Paste at the playhead onto the top selected track.
    app.update(Message::Seek(100_000));
    block(&mut processor)?;
    app.update(Message::Paste);
    let tracks = &app.session().state().tracks;
    assert_eq!(tracks[0].clips.len(), 3);
    assert_eq!(tracks[0].clips[2].1.position, 100_000);
    assert_eq!(tracks[1].clips[2].1.position, 100_000);

    // Delete leaves the gap; with nothing there it is not a step.
    app.update(Message::DeleteRange);
    let after = app.session().revision();
    app.update(Message::DeleteRange);
    assert_eq!(app.session().revision(), after);
    Ok(())
}

#[test]
fn test_range_edits_wait_for_the_transport_but_copy_does_not() -> Result<(), Box<dyn Error>> {
    let ((mut app, mut processor, _, _), vocal, _) = two_tracks()?;
    app.update(Message::LanePress {
        track: vocal,
        frame: 10_000,
        extend: false,
    });
    app.update(Message::LaneDrag {
        track: vocal,
        frame: 20_000,
    });
    app.update(Message::LaneRelease);
    app.update(Message::Play);
    block(&mut processor)?;
    let state = app.session().state().clone();
    for message in [Message::Cut, Message::DeleteRange, Message::Paste] {
        app.update(message);
        assert_eq!(app.session().state(), &state);
        assert!(app.status().is_some());
    }
    app.update(Message::Copy);
    let lane = app.clipboard().map(|e| e.lanes()[0].clips().len());
    assert_eq!(lane, Some(1));
    Ok(())
}

/// The fixture's project end: its clip at 4096 for 65536 frames.
const CLIP_END: u64 = 4096 + 65_536;

/// Plays, a block and a tick at a time, until playback stops by itself or `blocks` run out.
fn play_until_stopped(
    app: &mut App,
    processor: &mut Processor,
    blocks: usize,
) -> Result<(), Box<dyn Error>> {
    app.update(Message::Play);
    for _ in 0..blocks {
        block(processor)?;
        app.update(Message::Tick);
        if !app.engine().is_playing() {
            break;
        }
    }
    Ok(())
}

#[test]
fn test_playback_stops_at_the_project_end() -> Result<(), Box<dyn Error>> {
    let (mut app, mut processor, _, _) = app()?;
    app.update(Message::Seek(CLIP_END - 2_000));
    block(&mut processor)?;
    play_until_stopped(&mut app, &mut processor, 100)?;
    assert!(!app.engine().is_playing());
    assert_eq!(app.engine().playhead(), CLIP_END);
    // Play from the end starts again from the return point.
    app.update(Message::Play);
    block(&mut processor)?;
    let resumed = app.engine().playhead();
    assert!((CLIP_END - 2_000..CLIP_END - 1_000).contains(&resumed));
    app.update(Message::Stop);
    block(&mut processor)?;
    assert_eq!(app.engine().playhead(), CLIP_END - 2_000);
    Ok(())
}

#[test]
fn test_play_from_past_the_end_starts_from_the_top() -> Result<(), Box<dyn Error>> {
    let (mut app, mut processor, _, _) = app()?;
    // A seek past the end is kept, for importing or pasting there.
    app.update(Message::Seek(CLIP_END + 10_000));
    block(&mut processor)?;
    assert_eq!(app.engine().playhead(), CLIP_END + 10_000);
    app.update(Message::Play);
    block(&mut processor)?;
    assert!(app.engine().playhead() < CLIP_END);
    assert!(app.engine().is_playing());
    Ok(())
}

/// Zooms in until the view is shorter than the fixture's project.
fn zoom_inside_the_project(app: &mut App) {
    while app.view_span() >= CLIP_END / 2 {
        app.update(Message::ZoomIn);
    }
}

#[test]
fn test_seek_near_the_end_keeps_the_view_within_the_project() -> Result<(), Box<dyn Error>> {
    let (mut app, mut processor, _, _) = app()?;
    zoom_inside_the_project(&mut app);
    app.update(Message::Seek(CLIP_END - 100));
    block(&mut processor)?;
    // The view ends just past the project's end.
    let view_end = app.view_start() + app.view_span();
    assert_eq!(view_end, CLIP_END + 1);
    // A seek past the end still brings the playhead into view.
    app.update(Message::Seek(CLIP_END * 3));
    assert_eq!(app.view_start() + app.view_span(), CLIP_END * 3 + 1);
    Ok(())
}

#[test]
fn test_paging_stops_at_the_project_end() -> Result<(), Box<dyn Error>> {
    let (mut app, mut processor, _, _) = app()?;
    zoom_inside_the_project(&mut app);
    // Start inside the view, at 70 %, so the first pages come within a few blocks.
    app.update(Message::Seek(CLIP_END - app.view_span() * 2));
    block(&mut processor)?;
    let mut views = Vec::new();
    app.update(Message::Play);
    for _ in 0..1_000 {
        block(&mut processor)?;
        app.update(Message::Tick);
        views.push(app.view_start());
        if !app.engine().is_playing() {
            break;
        }
    }
    assert!(!app.engine().is_playing());
    assert!(views.windows(2).all(|w| w[0] <= w[1]), "never backwards");
    assert!(views.first() < views.last(), "it paged");
    let view_end = app.view_start() + app.view_span();
    assert_eq!(view_end, CLIP_END + 1);
    Ok(())
}

/// The fixture's one track.
fn vocal(app: &App) -> Result<TrackId, Box<dyn Error>> {
    Ok(app.session().state().tracks.first().ok_or("no track")?.id)
}

#[test]
fn test_record_places_the_take_on_top_as_one_step() -> Result<(), Box<dyn Error>> {
    let (mut app, mut processor, calls, _) = app()?;
    let track = vocal(&app)?;
    app.update(Message::Arm(track));
    assert_eq!(calls.borrow().input, [true]);
    assert!(app.can_record());
    app.update(Message::Seek(10_000));
    app.update(Message::Record);
    assert!(app.is_recording());
    // Arming and the other track edits wait for the transport.
    app.update(Message::Arm(track));
    assert!(app.arming(track).is_some());
    for _ in 0..4 {
        block(&mut processor)?;
    }
    app.update(Message::Stop);
    block(&mut processor)?;
    app.update(Message::Tick);
    // The pass is saved, so Record is available again.
    assert!(app.can_record());
    assert_eq!(calls.borrow().kept, ["Vocal 0.wav"]);
    let clips = &app.session().state().track(track).ok_or("no track")?.clips;
    assert_eq!(clips.len(), 2);
    let (_, take) = clips.last().ok_or("no take")?;
    assert_eq!(
        (take.position, take.offset, take.length),
        (10_000 - LATENCY, 0, 4 * 256)
    );
    app.update(Message::Undo);
    assert_eq!(
        app.session()
            .state()
            .track(track)
            .ok_or("no track")?
            .clips
            .len(),
        1
    );
    Ok(())
}

#[test]
fn test_take_stopped_before_capture_is_discarded() -> Result<(), Box<dyn Error>> {
    let (mut app, mut processor, calls, _) = app()?;
    let track = vocal(&app)?;
    app.update(Message::Arm(track));
    app.update(Message::Record);
    app.update(Message::Stop);
    block(&mut processor)?;
    app.update(Message::Tick);
    assert_eq!(calls.borrow().discarded, ["Vocal 0.wav"]);
    assert!(!app.session().can_undo());
    Ok(())
}

#[test]
fn test_record_needs_an_armed_track_and_inputs() -> Result<(), Box<dyn Error>> {
    let (mut app, _processor, calls, _) = app()?;
    let track = vocal(&app)?;
    app.update(Message::Record);
    assert_eq!(app.status(), Some("Arm a track to record."));
    calls.borrow_mut().inputs = 0;
    app.update(Message::Arm(track));
    assert_eq!(
        app.status(),
        Some("The audio device has no inputs to record from.")
    );
    assert!(!app.can_record());
    app.update(Message::Record);
    assert!(!app.is_recording());
    assert!(calls.borrow().started.is_empty());
    // Disarming the last track closes the inputs.
    app.update(Message::Arm(track));
    assert_eq!(calls.borrow().input, [true, false]);
    Ok(())
}

#[test]
fn test_monitor_and_input_follow_the_track_layout() -> Result<(), Box<dyn Error>> {
    let (mut app, _processor, _, _) = app()?;
    let track = vocal(&app)?;
    assert!(app.arming(track).is_none());
    app.update(Message::Arm(track));
    let (monitor, input) = app.arming(track).ok_or("not armed")?;
    assert!(!monitor);
    assert_eq!(input.to_string(), "In 1");
    let choices = app.input_choices(track);
    assert_eq!(choices.len(), 2);
    app.update(Message::Monitor(track));
    app.update(Message::Input(track, choices[1]));
    let (monitor, input) = app.arming(track).ok_or("not armed")?;
    assert!(monitor);
    assert_eq!(input.to_string(), "In 2");
    app.update(Message::AddTrack(Layout::Stereo));
    let keys = app.session().state().tracks.last().ok_or("no track")?.id;
    let pairs: Vec<String> = app
        .input_choices(keys)
        .iter()
        .map(ToString::to_string)
        .collect();
    assert_eq!(pairs, ["In 1–2"]);
    // Deleting an armed track disarms it.
    app.update(Message::Arm(keys));
    app.update(Message::DeleteTrack(keys));
    assert!(app.arming(keys).is_none());
    Ok(())
}

#[test]
fn test_recording_runs_on_past_the_project_end() -> Result<(), Box<dyn Error>> {
    let (mut app, mut processor, _, _) = app()?;
    let track = vocal(&app)?;
    app.update(Message::Arm(track));
    app.update(Message::Seek(4096 + 65_536 - 256));
    app.update(Message::Record);
    for _ in 0..3 {
        block(&mut processor)?;
    }
    app.update(Message::Tick);
    assert!(app.is_recording());
    assert_eq!(app.engine().real_time_state(), TransportState::Playing);
    Ok(())
}

#[test]
fn test_input_meter_follows_the_track_input_and_falls() -> Result<(), Box<dyn Error>> {
    let (mut app, mut processor, _, _) = app()?;
    let track = vocal(&app)?;
    assert!(!app.wants_ticks());
    app.update(Message::Arm(track));
    // Armed: ticks drive the meter.
    assert!(app.wants_ticks());
    let choices = app.input_choices(track);
    app.update(Message::Input(track, choices[1]));
    // Input 2 at half scale (−6 dBFS); input 1 silent.
    let input: Vec<f32> = (0..256).flat_map(|_| [0.0, 0.5]).collect();
    let (mut l, mut r) = ([0.0; 256], [0.0; 256]);
    processor.process(&input, 2, &mut l, &mut r)?;
    app.update(Message::Tick);
    let level = app.input_levels(track)[0];
    assert!((level - 20.0 * 0.5_f32.log10()).abs() < 1e-4, "{level}");
    // No new input: it falls, not drops.
    app.update(Message::Tick);
    let fallen = app.input_levels(track)[0];
    assert!(fallen < level && fallen > debussy_ui::METER_FLOOR_DB);
    app.update(Message::Arm(track));
    app.update(Message::Tick);
    assert!(app.input_levels(track).is_empty());
    Ok(())
}
