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
use debussy_ui::{App, Message, Services};
use engine_bridge::Bridge;
use project_model::{AudioFile, Clip, Endpoint, NodeId, NodeKind, Session, Source, State, TrackId};
use signal_graph::{Config, NodeId as GraphNodeId};

/// What the fake services saw.
#[derive(Default)]
struct Calls {
    track_feeds: usize,
    saves: usize,
    waveform: Vec<(String, u64, u64)>,
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

    fn save(&mut self, _session: &Session) -> Result<(), String> {
        self.calls.borrow_mut().saves += 1;
        Ok(())
    }
}

/// A project with one mono track at frame 4096 for 65536 frames, routed through a 1-channel
/// mixer, and the app over it.
/// The app, the engine's real-time side, what the fake services saw, and the mixer's id.
type Fixture = (App, Processor, Rc<RefCell<Calls>>, NodeId);

fn app() -> Result<Fixture, Box<dyn Error>> {
    let mut session = Session::new();
    let track = session.add_track("Vocal", Layout::Mono)?;
    let file = AudioFile {
        name: "vox.wav".into(),
        frames: 100_000,
    };
    session.add_clip(track, Clip::new(Source::Mono(file), 0, 65_536, 4096))?;
    let direct = session.state().connections[0].id;
    session.apply(project_model::Edit::Disconnect(direct))?;
    let mixer = session.add_node(NodeKind::Mixer {
        channels: 1,
        parameters: BTreeMap::new(),
    })?;
    session.connect(Endpoint::Track(track), 0, Endpoint::Node(mixer), 0)?;
    session.connect(Endpoint::Node(mixer), 0, Endpoint::Output, 0)?;
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
    app.update(Message::Scroll(100));
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
