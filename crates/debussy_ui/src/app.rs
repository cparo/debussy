//! [`App`]: the interface's state and how each [`Message`] changes it (`docs/architecture/
//! user-interface.md`, ADR-0056). Drawing is in `view`; nothing here needs a window, so it is
//! tested directly.
//!
//! Gestures follow ADR-0037 and ADR-0043: while a slider is dragged its value goes straight to
//! the running graph's parameter; releasing it records one undo step.

use audio_buffers::Layout;
use audio_effects::{Control, Mode, parameter_name};
use audio_engine::Engine;
use engine_bridge::Bridge;
use project_model::{Edit, Endpoint, NodeId, NodeKind, Position, Session, Track, TrackId};

use crate::menu::Menu;
use crate::patch_board::{Pin, Selection, can_connect};
use crate::services::Services;
use crate::theme::Tone;
use crate::transport::{
    DEFAULT_ZOOM, MAX_SAMPLES_PER_COLUMN, MAX_ZOOM, level_below, samples_per_column, zoomed_start,
};
use crate::waveform::{Display, LANE_COLUMNS, Lane, MIN_SAMPLES_PER_COLUMN, lanes};

/// The track pane's share of the window's width at launch.
const DEFAULT_SPLIT: f32 = 0.6;
/// The narrowest the divider leaves the track pane.
const MIN_SPLIT: f32 = 0.2;
/// The widest the divider leaves the track pane.
const MAX_SPLIT: f32 = 0.85;

/// A kind of node the board's "+" adds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NewNode {
    /// A two-channel mixer.
    Mixer,
    /// A stereo bus.
    Bus,
    /// A stereo feedback delay of a quarter second.
    FeedbackDelay,
}

impl NewNode {
    fn kind(self) -> NodeKind {
        match self {
            NewNode::Mixer => NodeKind::Mixer {
                channels: 2,
                parameters: std::collections::BTreeMap::new(),
            },
            NewNode::Bus => NodeKind::Bus(Layout::Stereo),
            NewNode::FeedbackDelay => NodeKind::FeedbackDelay {
                layout: Layout::Stereo,
                seconds: 0.25,
            },
        }
    }
}

/// A move on the board not yet recorded.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Moving {
    /// A node, now here.
    Node(NodeId, Position),
    /// The L/R outputs, L now at this height.
    Output(f32),
}

/// Something the user did, or a frame tick.
#[derive(Debug, Clone, PartialEq)]
pub enum Message {
    /// Start playing from the playhead.
    Play,
    /// Pause, holding the playhead.
    Pause,
    /// Stop, returning to the return point.
    Stop,
    /// Move the playhead to this timeline frame, playing on from there if playing; it becomes
    /// the return point.
    Seek(u64),
    /// Turn mono listening on (`true`) or off.
    SetMono(bool),
    /// A display frame while playing: refresh the playhead and finish pending work.
    Tick,
    /// Show less time: one √2 step.
    ZoomIn,
    /// Show more time: one √2 step.
    ZoomOut,
    /// Pan by this fraction of the visible time (negative is earlier).
    Pan(f32),
    /// Show waveforms or spectrograms.
    SetDisplay(Display),
    /// Switch between light and dark chrome.
    ToggleTone,
    /// The system's appearance, at launch or when it changes.
    SystemTone(Tone),
    /// Open this menu, or close it if it is open.
    OpenMenu(Menu),
    /// Close the open menu.
    CloseMenu,
    /// The tracks, gutter and board scrolled to this offset from the top, in logical pixels.
    Scrolled(f32),
    /// The divider moved: the track pane's share of the window's width.
    Split(f32),
    /// Quit the application; the window's shell handles it.
    Quit,
    /// Undo the latest step.
    Undo,
    /// Redo the latest undone step.
    Redo,
    /// Save the project.
    Save,
    /// Import files as new tracks, through a file dialog.
    Import,
    /// Import files into this track, through a file dialog: each a clip from the playhead on.
    ImportInto(TrackId),
    /// Add an empty track of this layout after the others.
    AddTrack(Layout),
    /// Delete a track, its clips and its connections.
    DeleteTrack(TrackId),
    /// Start renaming a track.
    StartRename(TrackId),
    /// The new name being typed.
    RenameText(String),
    /// Finish renaming: record the typed name as one step.
    FinishRename,
    /// A wire is being dragged from this output.
    StartWire(Pin),
    /// The wire being dragged was dropped on this input, or elsewhere.
    FinishWire(Option<Pin>),
    /// Select a node (opening its dialog) or a wire; `None` clears the selection and closes
    /// the dialog.
    Select(Option<Selection>),
    /// A node is being dragged here: shown, not yet recorded.
    DragNode(NodeId, Position),
    /// The L/R outputs are being dragged: L's height, shown, not yet recorded.
    DragOutput(f32),
    /// The node or outputs being dragged were dropped: record the move as one step.
    Drop,
    /// Delete the selected node or wire.
    DeleteSelection,
    /// Add a node to the board.
    AddNode(NewNode),
    /// Delete a node and its connections.
    DeleteNode(NodeId),
    /// Add a channel to a mixer.
    AddMixerChannel(NodeId),
    /// Remove a mixer's last channel, if nothing is connected to it.
    RemoveMixerChannel(NodeId),
    /// The feedback delay time being typed, in seconds.
    DelayText(NodeId, String),
    /// Record the typed delay time as one step.
    CommitDelay,
    /// Export the mix, through a file dialog; the mono mix if `true`.
    Export(bool),
    /// Use this audio device for input and output.
    Device(crate::services::DeviceChoice),
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
    /// The project's name, for the top bar.
    pub(crate) project_name: String,
    /// The first timeline frame shown.
    pub(crate) start: u64,
    /// The zoom: a step on the grid or a fit to the project.
    zoom: Zoom,
    /// Whether the view pages along with the playhead: set once a playing playhead is seen in
    /// view, cleared when playback stops or the view is panned or zoomed.
    following: bool,
    /// Where stop returns to: the last place seeked to.
    return_point: u64,
    /// Each track's lanes, one per channel, in track order.
    pub(crate) lanes: Vec<(TrackId, Vec<Lane>)>,
    /// Changes whenever the lanes' contents or the tone change.
    pub(crate) generation: u64,
    pub(crate) display: Display,
    pub(crate) tone: Tone,
    pub(crate) menu: Option<Menu>,
    /// The track being renamed, and the name typed so far.
    pub(crate) renaming: Option<(TrackId, String)>,
    /// How far the tracks, gutter and board are scrolled down, in logical pixels.
    pub(crate) scrolled: f32,
    /// The track pane's share of the window's width.
    pub(crate) split: f32,
    /// The selected node or wire.
    pub(crate) selection: Option<Selection>,
    /// The output a wire is being dragged from.
    pub(crate) wiring: Option<Pin>,
    /// A node or the outputs being dragged.
    pub(crate) moving: Option<Moving>,
    /// A feedback delay's time being typed.
    pub(crate) delay_text: Option<(NodeId, String)>,
    /// The tracks the engine's feeds were made from.
    loaded: Option<Vec<Track>>,
    pub(crate) live: Option<Live>,
    /// The audio devices present, and the one in use.
    pub(crate) devices: (Vec<crate::services::DeviceChoice>, Option<String>),
    /// The last error or notice, for the status line.
    pub(crate) status: Option<String>,
}

/// How far the view is zoomed (`docs/architecture/user-interface.md` "Transport and seeking").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Zoom {
    /// A step on the √2 grid; see [`samples_per_column`].
    Level(u32),
    /// Frame 0 to the project's end, the farthest zoom out.
    Fit,
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
            project_name: String::new(),
            start: 0,
            zoom: Zoom::Level(DEFAULT_ZOOM),
            following: false,
            return_point: 0,
            lanes: Vec::new(),
            generation: 0,
            display: Display::default(),
            tone: Tone::default(),
            menu: None,
            renaming: None,
            scrolled: 0.0,
            split: DEFAULT_SPLIT,
            selection: None,
            wiring: None,
            moving: None,
            delay_text: None,
            loaded: None,
            live: None,
            devices: (Vec::new(), None),
            status: None,
        };
        app.refresh_devices();
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

    /// Names the project in the top bar.
    pub fn set_project_name(&mut self, name: String) {
        self.project_name = name;
    }

    /// Shows `message` on the status line.
    pub fn set_status(&mut self, message: String) {
        self.status = Some(message);
    }

    /// The status line's text, if any.
    pub fn status(&self) -> Option<&str> {
        self.status.as_deref()
    }

    /// Frames per lane column at the current zoom.
    pub fn samples_per_column(&self) -> u64 {
        match self.zoom {
            Zoom::Level(level) => samples_per_column(level),
            Zoom::Fit => match self.project_end() {
                0 => samples_per_column(DEFAULT_ZOOM),
                end => end
                    .div_ceil(LANE_COLUMNS as u64)
                    .clamp(MIN_SAMPLES_PER_COLUMN, MAX_SAMPLES_PER_COLUMN),
            },
        }
    }

    /// Whether zooming out would show more: the view is on the grid, not yet as long as the
    /// project, and not at the coarsest level.
    pub(crate) fn can_zoom_out(&self) -> bool {
        match self.zoom {
            Zoom::Level(level) => level < MAX_ZOOM && self.view_span() < self.project_end(),
            Zoom::Fit => false,
        }
    }

    /// The first timeline frame shown.
    pub fn view_start(&self) -> u64 {
        self.start
    }

    /// The frames shown across a lane.
    pub fn view_span(&self) -> u64 {
        self.samples_per_column()
            .saturating_mul(LANE_COLUMNS as u64)
    }

    /// Where stop returns to.
    pub fn return_point(&self) -> u64 {
        self.return_point
    }

    /// Light or dark chrome.
    pub fn tone(&self) -> Tone {
        self.tone
    }

    /// Waveforms or spectrograms.
    pub fn display(&self) -> Display {
        self.display
    }

    /// The track pane's share of the window's width.
    pub fn split(&self) -> f32 {
        self.split
    }

    /// Each track's lanes for the visible stretch, top to bottom.
    pub fn lanes(&self) -> &[(TrackId, Vec<Lane>)] {
        &self.lanes
    }

    /// Each track's lane centres, top to bottom, from the top of the track pane's contents.
    pub fn lane_centres(&self) -> Vec<(TrackId, Vec<f32>)> {
        let mut top = 0.0;
        self.lanes
            .iter()
            .map(|(id, lanes)| {
                let first = top + crate::view::HEADER_HEIGHT;
                let centres = (0..lanes.len())
                    .map(|k| first + crate::view::LANE_HEIGHT * (k as f32 + 0.5))
                    .collect();
                top = first + crate::view::LANE_HEIGHT * lanes.len() as f32;
                (*id, centres)
            })
            .collect()
    }

    /// The patch board laid out at least `min` in size.
    pub fn board(&self, min: iced::Size) -> crate::patch_board::Board {
        let state = self.session.state();
        let Some(moving) = self.moving else {
            return crate::patch_board::layout(state, &self.lane_centres(), min);
        };
        // Show the move under way without recording it.
        let mut shown = state.clone();
        match moving {
            Moving::Node(id, position) => {
                if let Some(node) = shown.nodes.iter_mut().find(|n| n.id == id) {
                    node.position = Some(position);
                }
            }
            Moving::Output(y) => shown.output_y = Some(y),
        }
        crate::patch_board::layout(&shown, &self.lane_centres(), min)
    }

    /// The selected node or wire.
    pub fn selection(&self) -> Option<Selection> {
        self.selection
    }

    /// The number of channels `node` would keep after removing its last one, if it is a mixer
    /// of more than one channel with nothing connected to its last.
    pub fn removable_channel(&self, node: NodeId) -> Option<usize> {
        let state = self.session.state();
        let NodeKind::Mixer { channels, .. } = state.node(node)?.kind else {
            return None;
        };
        let last = channels.checked_sub(1).filter(|&n| n >= 1)?;
        let used = state
            .connections
            .iter()
            .any(|c| c.to == Endpoint::Node(node) && c.input == last);
        (!used).then_some(last)
    }

    /// The open menu, if any.
    pub fn menu(&self) -> Option<Menu> {
        self.menu
    }

    /// Whether the interface needs a tick every display frame: while playing or starting.
    pub fn wants_ticks(&self) -> bool {
        self.engine.is_playing()
    }

    /// Handles `message`.
    pub fn update(&mut self, message: Message) {
        // A frame tick, a scroll or typing leaves the status line and the open menu alone;
        // anything else replaces the status and closes the menu.
        if !matches!(
            message,
            Message::Tick
                | Message::Scrolled(_)
                | Message::SystemTone(_)
                | Message::RenameText(_)
                | Message::OpenMenu(_)
                | Message::DragNode(..)
                | Message::DragOutput(_)
                | Message::DelayText(..)
        ) {
            self.status = None;
            self.menu = None;
        }
        // Doing anything else while renaming a track finishes the rename.
        if !matches!(
            message,
            Message::Tick
                | Message::Scrolled(_)
                | Message::SystemTone(_)
                | Message::RenameText(_)
                | Message::FinishRename
        ) {
            self.finish_rename();
        }
        match message {
            Message::Play => self.play(),
            Message::Pause => {
                let result = self.engine.pause();
                self.report(result.map_err(|e| e.to_string()));
            }
            Message::Stop => {
                let result = self.engine.stop().and_then(|()| {
                    self.engine
                        .locate(self.return_point, self.services.streams())
                });
                self.report(result.map_err(|e| e.to_string()));
            }
            Message::Seek(frame) => self.seek(frame),
            Message::SetMono(mono) => self.engine.set_mono(mono),
            Message::Tick => {
                self.engine.collect();
                self.follow_playhead();
                let result = self.bridge.commit();
                self.report(result.map_err(|e| e.to_string()));
                if let Some(error) = self.services.audio_error() {
                    // Nothing more can play until another device is chosen.
                    let _ = self.engine.stop();
                    self.status = Some(format!("Audio stopped: {error}"));
                }
            }
            Message::ZoomIn => {
                let level = match self.zoom {
                    Zoom::Level(level) => level.saturating_sub(1),
                    Zoom::Fit => level_below(self.samples_per_column()).unwrap_or(0),
                };
                self.zoom(Zoom::Level(level));
            }
            Message::ZoomOut => {
                if let Zoom::Level(level) = self.zoom
                    && self.can_zoom_out()
                {
                    let next = level + 1;
                    let spans = samples_per_column(next).saturating_mul(LANE_COLUMNS as u64);
                    if spans < self.project_end() {
                        self.zoom(Zoom::Level(next));
                    } else {
                        self.following = false;
                        self.zoom = Zoom::Fit;
                        self.start = 0;
                        self.refresh_lanes();
                    }
                }
            }
            Message::Pan(fraction) => {
                // The next tick follows again if the playhead is still in view.
                self.following = false;
                let by = (f64::from(fraction.abs()) * self.view_span() as f64) as u64;
                self.start = if fraction < 0.0 {
                    self.start.saturating_sub(by)
                } else {
                    self.start.saturating_add(by)
                };
                self.refresh_lanes();
            }
            Message::SetDisplay(display) => {
                self.display = display;
                self.refresh_lanes();
            }
            Message::ToggleTone => self.set_tone(self.tone.toggled()),
            Message::SystemTone(tone) => self.set_tone(tone),
            Message::OpenMenu(menu) => {
                self.menu = (self.menu != Some(menu)).then_some(menu);
            }
            Message::CloseMenu | Message::Quit => {}
            Message::Scrolled(offset) => self.scrolled = offset,
            Message::Split(share) => self.split = share.clamp(MIN_SPLIT, MAX_SPLIT),
            Message::Undo | Message::Redo => self.history(message == Message::Undo),
            Message::Save => {
                let result = self.services.save(&self.session);
                self.report(result);
            }
            Message::Import => self.import(None),
            Message::ImportInto(track) => self.import(Some(track)),
            Message::AddTrack(layout) => {
                if self.refuse_while_playing("add a track") {
                    return;
                }
                let name = format!("Track {}", self.session.state().tracks.len() + 1);
                let result = self
                    .bridge
                    .edit(&mut self.session, |s| s.add_track(&name, layout));
                self.report(result.map(|_| ()).map_err(|e| e.to_string()));
                self.refresh_lanes();
            }
            Message::DeleteTrack(track) => {
                if self.refuse_while_playing("delete a track") {
                    return;
                }
                let result = self
                    .bridge
                    .edit(&mut self.session, |s| s.remove_track(track));
                self.report(result.map_err(|e| e.to_string()));
                self.refresh_lanes();
            }
            Message::StartRename(track) => {
                self.renaming = self
                    .session
                    .state()
                    .track(track)
                    .map(|t| (track, t.name.clone()));
            }
            Message::RenameText(name) => {
                if let Some((_, typed)) = &mut self.renaming {
                    *typed = name;
                }
            }
            Message::FinishRename => self.finish_rename(),
            Message::StartWire(pin) => self.wiring = Some(pin),
            Message::FinishWire(to) => {
                if let (Some(from), Some(to)) = (self.wiring.take(), to)
                    && can_connect(self.session.state(), from, to)
                {
                    let result = self.bridge.edit(&mut self.session, |s| {
                        s.connect_sides(
                            from.endpoint,
                            from.port,
                            from.side,
                            to.endpoint,
                            to.port,
                            to.side,
                        )
                    });
                    self.report(result.map(|_| ()).map_err(|e| e.to_string()));
                }
                self.wiring = None;
            }
            Message::Select(selection) => {
                // A click on the board leaves any delay time being typed.
                self.delay_text = None;
                self.selection = selection;
            }
            Message::DragNode(node, position) => {
                self.moving = Some(Moving::Node(node, position));
            }
            Message::DragOutput(y) => self.moving = Some(Moving::Output(y)),
            Message::Drop => {
                let result = match self.moving.take() {
                    Some(Moving::Node(node, position)) => self
                        .bridge
                        .edit(&mut self.session, |s| s.move_node(node, Some(position))),
                    Some(Moving::Output(y)) => self
                        .bridge
                        .edit(&mut self.session, |s| s.move_output(Some(y))),
                    None => Ok(()),
                };
                self.report(result.map_err(|e| e.to_string()));
            }
            Message::DeleteSelection => {
                // Delete and Backspace edit the text being typed instead.
                if self.renaming.is_some() || self.delay_text.is_some() {
                    return;
                }
                let result = match self.selection.take() {
                    Some(Selection::Node(node)) => {
                        self.bridge.edit(&mut self.session, |s| s.remove_node(node))
                    }
                    Some(Selection::Wire(id)) => {
                        self.bridge.edit(&mut self.session, |s| s.disconnect(id))
                    }
                    None => Ok(()),
                };
                self.report(result.map_err(|e| e.to_string()));
            }
            Message::AddNode(choice) => {
                let result = self
                    .bridge
                    .edit(&mut self.session, |s| s.add_node(choice.kind()));
                match result {
                    Ok(node) => self.selection = Some(Selection::Node(node)),
                    Err(error) => self.status = Some(error.to_string()),
                }
            }
            Message::DeleteNode(node) => {
                let result = self.bridge.edit(&mut self.session, |s| s.remove_node(node));
                self.report(result.map_err(|e| e.to_string()));
                if self.selection == Some(Selection::Node(node)) {
                    self.selection = None;
                }
            }
            Message::AddMixerChannel(node) => {
                let result = self
                    .bridge
                    .edit(&mut self.session, |s| s.add_mixer_channels(node, 1));
                self.report(result.map_err(|e| e.to_string()));
            }
            Message::RemoveMixerChannel(node) => {
                if let Some(channels) = self.removable_channel(node) {
                    let result = self.bridge.edit(&mut self.session, |s| {
                        s.apply(Edit::SetMixerChannels { node, channels })
                    });
                    self.report(result.map_err(|e| e.to_string()));
                }
            }
            Message::DelayText(node, text) => self.delay_text = Some((node, text)),
            Message::CommitDelay => {
                if let Some((node, text)) = self.delay_text.take() {
                    match text.trim().parse::<f64>() {
                        Ok(seconds) => {
                            let result = self.bridge.edit(&mut self.session, |s| {
                                s.apply(Edit::SetDelay { node, seconds })
                            });
                            self.report(result.map_err(|e| e.to_string()));
                        }
                        Err(_) => self.status = Some(format!("{text:?} is not a time in seconds")),
                    }
                }
            }
            Message::Export(mono) => match self.services.export(self.session.state(), mono) {
                Ok(Some(name)) => self.status = Some(format!("Exported {name}")),
                Ok(None) => {}
                Err(message) => self.status = Some(message),
            },
            Message::Device(choice) => {
                if self.engine.is_playing() {
                    self.status = Some("Stop playback to change the audio device.".to_owned());
                } else {
                    let result = self.services.set_device(&choice.id);
                    self.report(result);
                    self.refresh_devices();
                }
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

    /// Imports through the file dialog: each file as a new track, or with `into`, as a clip on
    /// that track from the playhead on, end to end. Each file is one step.
    fn import(&mut self, into: Option<TrackId>) {
        if self.refuse_while_playing("import") {
            return;
        }
        let imported = match self.services.import() {
            Ok(imported) => imported,
            Err(message) => {
                self.status = Some(message);
                return;
            }
        };
        let mut position = self.engine.playhead();
        for track in imported {
            let clip = project_model::Clip::new(track.source, 0, track.frames, 0);
            let result = match into {
                None => self
                    .bridge
                    .edit(&mut self.session, |s| {
                        s.add_track_with_clip(&track.name, track.layout, clip)
                    })
                    .map(|_| ()),
                Some(id) => {
                    let clip = project_model::Clip { position, ..clip };
                    let end = clip.end();
                    let added = self
                        .bridge
                        .edit(&mut self.session, |s| s.add_clip(id, clip));
                    if added.is_ok() {
                        position = end;
                    }
                    added.map(|_| ())
                }
            };
            if let Err(error) = result {
                self.status = Some(format!("{}: {error}", track.name));
            }
        }
        self.refresh_lanes();
    }

    /// Moves the playhead to `frame` and makes it the return point, bringing it into view.
    fn seek(&mut self, frame: u64) {
        let result = self.engine.locate(frame, self.services.streams());
        self.report(result.map_err(|e| e.to_string()));
        self.return_point = frame;
        let span = self.view_span();
        if frame < self.start || frame >= self.start.saturating_add(span) {
            self.start = frame.saturating_sub(span / 2);
            self.refresh_lanes();
        }
    }

    fn set_tone(&mut self, tone: Tone) {
        self.tone = tone;
        self.generation = self.generation.wrapping_add(1);
    }

    fn finish_rename(&mut self) {
        let Some((track, name)) = self.renaming.take() else {
            return;
        };
        let name = name.trim().to_owned();
        let unchanged = self
            .session
            .state()
            .track(track)
            .is_none_or(|t| t.name == name);
        if name.is_empty() || unchanged {
            return;
        }
        let result = self.bridge.edit(&mut self.session, |s| {
            s.apply(Edit::RenameTrack { track, name })
        });
        self.report(result.map_err(|e| e.to_string()));
    }

    /// Reports and returns `true` while playing: `what` waits for the transport to stop.
    fn refuse_while_playing(&mut self, what: &str) -> bool {
        let playing = self.engine.is_playing();
        if playing {
            self.status = Some(format!("Stop playback to {what}."));
        }
        playing
    }

    /// Re-reads the device list and the device in use.
    pub(crate) fn refresh_devices(&mut self) {
        self.devices = self.services.devices();
    }

    fn history(&mut self, undo: bool) {
        if self.refuse_while_playing("undo or redo") {
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

    /// Zooms to `zoom`, keeping the playhead in place if it is in view, else the view's centre.
    fn zoom(&mut self, zoom: Zoom) {
        self.following = false;
        let old = self.samples_per_column();
        let anchor = match self.playhead_fraction() {
            Some(_) => self.engine.playhead(),
            None => self.start.saturating_add(self.view_span() / 2),
        };
        self.zoom = zoom;
        self.start = zoomed_start(self.start, old, self.samples_per_column(), anchor);
        self.refresh_lanes();
    }

    /// Pages the view along with a playing playhead (`docs/architecture/user-interface.md`
    /// "Transport and seeking"): once it has been seen in view, reaching 80 % of the lane's
    /// width moves the view to put it at 20 %. A playhead out of view that was not followed
    /// there is left alone.
    fn follow_playhead(&mut self) {
        if !self.engine.is_playing() {
            self.following = false;
            return;
        }
        let playhead = self.engine.playhead();
        let span = self.view_span();
        if playhead >= self.start && playhead < self.start.saturating_add(span) {
            self.following = true;
        }
        if self.following && playhead >= self.start.saturating_add(span * 4 / 5) {
            self.start = playhead.saturating_sub(span / 5);
            self.refresh_lanes();
        }
    }

    /// Recomputes every track's lanes for the visible stretch.
    pub(crate) fn refresh_lanes(&mut self) {
        let (start, spc, display) = (self.start, self.samples_per_column(), self.display);
        let services = self.services.as_mut();
        self.lanes = self
            .session
            .state()
            .tracks
            .iter()
            .map(|track| (track.id, lanes(track, start, spc, display, services)))
            .collect();
        self.generation = self.generation.wrapping_add(1);
    }

    /// Where the playhead falls in the visible stretch, as a fraction of its width.
    pub(crate) fn playhead_fraction(&self) -> Option<f32> {
        let span = self.view_span();
        let at = self.engine.playhead().checked_sub(self.start)?;
        (at < span).then(|| at as f32 / span as f32)
    }

    /// The timeline's length for the position bar: the end of the last clip, or further to
    /// take in the visible window and the playhead.
    pub(crate) fn timeline_length(&self) -> u64 {
        self.project_end()
            .max(self.start.saturating_add(self.view_span()))
            .max(self.engine.playhead())
            .max(1)
    }

    /// The project's end: the end of its last clip, or 0 if it has none.
    fn project_end(&self) -> u64 {
        self.session
            .state()
            .tracks
            .iter()
            .flat_map(|t| t.clips.iter().map(|(_, c)| c.end()))
            .max()
            .unwrap_or(0)
    }

    fn report(&mut self, result: Result<(), String>) {
        if let Err(message) = result {
            self.status = Some(message);
        }
    }
}
