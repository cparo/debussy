//! [`App`]: the interface's state and how each [`Message`] changes it (`docs/architecture/
//! user-interface.md`, ADR-0056). Drawing is in `view`; nothing here needs a window, so it is
//! tested directly.
//!
//! Gestures follow ADR-0037 and ADR-0043: while a slider is dragged its value goes straight to
//! the running graph's parameter; releasing it records one undo step.

use audio_effects::{Control, Mode, parameter_name};
use audio_engine::Engine;
use engine_bridge::Bridge;
use project_model::{NodeId, NodeKind, Session, Track, TrackId};

use crate::services::Services;
use crate::waveform::{LANE_COLUMNS, Lane, MIN_SAMPLES_PER_COLUMN, lane};

/// The finest zoom out: frames per lane column.
pub const MAX_SAMPLES_PER_COLUMN: u64 = 1 << 24;

/// Something the user did, or a frame tick.
#[derive(Debug, Clone, PartialEq)]
pub enum Message {
    /// Start playing from the playhead.
    Play,
    /// Pause, holding the playhead.
    Pause,
    /// Stop, returning to where playback started.
    Stop,
    /// Move the playhead to the start.
    Rewind,
    /// Turn mono listening on or off.
    ToggleMono,
    /// A display frame while playing: refresh the playhead and finish pending work.
    Tick,
    /// Show half as much time.
    ZoomIn,
    /// Show twice as much time.
    ZoomOut,
    /// Scroll by this many lane columns (negative is earlier).
    Scroll(i64),
    /// Undo the latest step.
    Undo,
    /// Redo the latest undone step.
    Redo,
    /// Save the project.
    Save,
    /// A mixer gain or pan slider moved: applied live, recorded on [`Message::Release`].
    Slide {
        /// The mixer node.
        node: NodeId,
        /// The channel, from 0.
        channel: usize,
        /// Gain or pan.
        control: Control,
        /// The value.
        value: f32,
    },
    /// The slider being dragged was released: record the value as one undo step.
    Release,
    /// A channel's mute or solo button: sets its mode as one undo step.
    SetMode {
        /// The mixer node.
        node: NodeId,
        /// The channel, from 0.
        channel: usize,
        /// The new mode.
        mode: Mode,
    },
}

/// A parameter being dragged: applied to the graph, not yet recorded.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Live {
    pub node: NodeId,
    pub name: String,
    pub value: f32,
}

/// The interface's state.
pub struct App {
    pub(crate) session: Session,
    pub(crate) bridge: Bridge,
    pub(crate) engine: Engine,
    pub(crate) services: Box<dyn Services>,
    sample_rate: u32,
    /// The first timeline frame shown.
    pub(crate) start: u64,
    /// Frames per lane column.
    pub(crate) samples_per_column: u64,
    pub(crate) lanes: Vec<(TrackId, Lane)>,
    /// The tracks the engine's feeds were made from.
    loaded: Option<Vec<Track>>,
    pub(crate) live: Option<Live>,
    /// The last error or notice, for the status line.
    pub(crate) status: Option<String>,
}

impl App {
    /// The interface over a project at `sample_rate`: its `session`, the `bridge` running its
    /// routing, the engine's control side, and the application's `services`.
    pub fn new(
        session: Session,
        sample_rate: u32,
        bridge: Bridge,
        engine: Engine,
        services: Box<dyn Services>,
    ) -> App {
        let mut app = App {
            session,
            bridge,
            engine,
            services,
            sample_rate: sample_rate.max(1),
            start: 0,
            samples_per_column: 1024,
            lanes: Vec::new(),
            loaded: None,
            live: None,
            status: None,
        };
        app.refresh_lanes();
        app
    }

    /// The project's session.
    pub fn session(&self) -> &Session {
        &self.session
    }

    /// The project sample rate.
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// The engine's control side.
    pub fn engine(&self) -> &Engine {
        &self.engine
    }

    /// The bridge to the running graph.
    pub fn bridge(&self) -> &Bridge {
        &self.bridge
    }

    /// The status line's text, if any.
    pub fn status(&self) -> Option<&str> {
        self.status.as_deref()
    }

    /// Whether the interface needs a tick every display frame: while playing or starting.
    pub fn wants_ticks(&self) -> bool {
        self.engine.is_playing()
    }

    /// Handles `message`.
    pub fn update(&mut self, message: Message) {
        self.status = None;
        match message {
            Message::Play => self.play(),
            Message::Pause => {
                let result = self.engine.pause();
                self.report(result.map_err(|e| e.to_string()));
            }
            Message::Stop => {
                let result = self.engine.stop();
                self.report(result.map_err(|e| e.to_string()));
            }
            Message::Rewind => {
                let result = self.engine.locate(0, self.services.streams());
                self.report(result.map_err(|e| e.to_string()));
            }
            Message::ToggleMono => self.engine.set_mono(!self.engine.mono()),
            Message::Tick => {
                self.engine.collect();
                let result = self.bridge.commit();
                self.report(result.map_err(|e| e.to_string()));
            }
            Message::ZoomIn => self.zoom(self.samples_per_column / 2),
            Message::ZoomOut => self.zoom(self.samples_per_column.saturating_mul(2)),
            Message::Scroll(columns) => {
                let by = columns
                    .unsigned_abs()
                    .saturating_mul(self.samples_per_column);
                self.start = if columns < 0 {
                    self.start.saturating_sub(by)
                } else {
                    self.start.saturating_add(by)
                };
                self.refresh_lanes();
            }
            Message::Undo | Message::Redo => self.history(message == Message::Undo),
            Message::Save => {
                let result = self.services.save(&self.session);
                self.report(result);
            }
            Message::Slide {
                node,
                channel,
                control,
                value,
            } => {
                let name = parameter_name(channel, control);
                if let Some(parameter) = self.bridge.parameter(node, &name) {
                    parameter.set(value);
                }
                self.live = Some(Live { node, name, value });
            }
            Message::Release => {
                if let Some(live) = self.live.take() {
                    let result = self.bridge.edit(&mut self.session, |s| {
                        s.set_parameter(live.node, &live.name, Some(live.value))
                    });
                    self.report(result.map_err(|e| e.to_string()));
                }
            }
            Message::SetMode {
                node,
                channel,
                mode,
            } => {
                let name = parameter_name(channel, Control::Mode);
                let result = self.bridge.edit(&mut self.session, |s| {
                    s.set_parameter(node, &name, Some(mode.value()))
                });
                self.report(result.map_err(|e| e.to_string()));
            }
        }
    }

    /// A mixer parameter's value as shown: the one being dragged, else the model's, else its
    /// default.
    pub fn mixer_value(&self, node: NodeId, channel: usize, control: Control) -> f32 {
        let name = parameter_name(channel, control);
        if let Some(live) = self
            .live
            .as_ref()
            .filter(|l| l.node == node && l.name == name)
        {
            return live.value;
        }
        self.session
            .state()
            .node(node)
            .and_then(|n| match &n.kind {
                NodeKind::Mixer { parameters, .. } => parameters.get(&name).copied(),
                _ => None,
            })
            .unwrap_or(0.0)
    }

    fn play(&mut self) {
        if !self.engine.is_playing() && self.loaded.as_ref() != Some(&self.session.state().tracks) {
            let sources = self.bridge.sources();
            match self.services.track_feeds(self.session.state(), &sources) {
                Ok(feeds) => {
                    if let Err(error) = self.engine.set_tracks(feeds) {
                        self.status = Some(error.to_string());
                        return;
                    }
                    self.loaded = Some(self.session.state().tracks.clone());
                }
                Err(message) => {
                    self.status = Some(message);
                    return;
                }
            }
        }
        let result = self.engine.play(self.services.streams());
        self.report(result.map_err(|e| e.to_string()));
    }

    fn history(&mut self, undo: bool) {
        if self.engine.is_playing() {
            self.status = Some("Stop or pause playback to undo or redo.".to_owned());
            return;
        }
        let result = if undo {
            self.bridge.undo(&mut self.session)
        } else {
            self.bridge.redo(&mut self.session)
        };
        self.report(result.map(|_| ()).map_err(|e| e.to_string()));
        self.refresh_lanes();
    }

    fn zoom(&mut self, samples_per_column: u64) {
        self.samples_per_column =
            samples_per_column.clamp(MIN_SAMPLES_PER_COLUMN, MAX_SAMPLES_PER_COLUMN);
        self.refresh_lanes();
    }

    /// Recomputes every track's waveform lane for the visible stretch.
    pub(crate) fn refresh_lanes(&mut self) {
        let (start, spc) = (self.start, self.samples_per_column);
        let services = self.services.as_mut();
        self.lanes = self
            .session
            .state()
            .tracks
            .iter()
            .map(|track| (track.id, lane(track, start, spc, services)))
            .collect();
    }

    /// Where the playhead falls in the visible stretch, as a fraction of its width.
    pub(crate) fn playhead_fraction(&self) -> Option<f32> {
        let span = self.samples_per_column.saturating_mul(LANE_COLUMNS as u64);
        let at = self.engine.playhead().checked_sub(self.start)?;
        (at < span).then(|| at as f32 / span as f32)
    }

    fn report(&mut self, result: Result<(), String>) {
        if let Err(message) = result {
            self.status = Some(message);
        }
    }
}
