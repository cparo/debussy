//! [`App`]: the interface's state and how each [`Message`] changes it (`docs/architecture/
//! user-interface.md`, ADR-0056). Drawing is in `view`; nothing here needs a window, so it is
//! tested directly.
//!
//! Gestures follow ADR-0037 and ADR-0043: while a slider is dragged its value goes straight to
//! the running graph's parameter; releasing it records one undo step.

use std::collections::BTreeMap;
use std::ops::RangeInclusive;
use std::path::PathBuf;

use audio_effects::{Control, MAX_GAIN_DB, Mode, SILENCE_DB, parameter_name};

use audio_buffers::Layout;
use audio_engine::Engine;
use engine_bridge::Bridge;
use project_model::{
    Edit, Endpoint, Excerpt, NodeId, NodeKind, Position, Range, Session, Track, TrackId,
};
use signal_graph::{ParameterInfo, Scale};

use crate::click::Click;
use crate::effect_panel;
use crate::menu::Menu;
use crate::mixer_strip::{GAIN_STEP_DB, MixerFocus, PAN_STEP, toggled};
use crate::patch_board::{Pin, Selection, can_connect};
use crate::range_selection::RangeSelection;
use crate::recording::{Arming, InputChoice, Pass};
use crate::services::Services;
use crate::theme::Tone;
use crate::transport::{
    DEFAULT_ZOOM, MAX_SAMPLES_PER_COLUMN, MAX_ZOOM, level_below, samples_per_column, zoomed_start,
};
use crate::waveform::{Display, LANE_COLUMNS, Lane, MIN_SAMPLES_PER_COLUMN, channels, lanes};

/// The track pane's share of the window's width at launch.
const DEFAULT_SPLIT: f32 = 0.6;
/// The narrowest the divider leaves the track pane.
const MIN_SPLIT: f32 = 0.2;
/// The widest the divider leaves the track pane.
const MAX_SPLIT: f32 = 0.85;
/// How far a drag must go, in lane columns, before it selects a range rather than clicks.
const DRAG_COLUMNS: u64 = 4;

/// A kind of node the board's "+" adds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NewNode {
    /// A two-channel mixer.
    Mixer,
    /// A stereo bus.
    Bus,
    /// A stereo feedback delay of a quarter second.
    FeedbackDelay,
    /// A built-in effect of this kind id (`audio_effects::EFFECTS`), at its defaults.
    Effect(&'static str),
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
            NewNode::Effect(kind) => NodeKind::Effect {
                kind: kind.to_owned(),
                parameters: std::collections::BTreeMap::new(),
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

/// The open dialog's control with the keyboard focus: Tab and Shift-Tab move it, the arrow keys
/// adjust it, and Space or Enter press it (ADR-0089).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialogFocus {
    /// A mixer dialog's control.
    Mixer(MixerFocus),
    /// An effect dialog's control, by index into its kind's parameters.
    Parameter(usize),
}

/// Something the user did, or a frame tick.
#[derive(Debug, Clone, PartialEq)]
pub enum Message {
    /// Start playing from the playhead.
    Play,
    /// Start recording the armed tracks from the playhead, with the others playing.
    Record,
    /// Arm this track for recording, or disarm it.
    Arm(TrackId),
    /// Turn this armed track's input monitoring on or off.
    Monitor(TrackId),
    /// Record this armed track from this input.
    Input(TrackId, InputChoice),
    /// Pause, holding the playhead.
    Pause,
    /// Stop, returning to the return point.
    Stop,
    /// Move the playhead to this timeline frame, playing on from there if playing; it becomes
    /// the return point.
    Seek(u64),
    /// A press in a lane of `track` at timeline frame `frame`: seeks and starts a drag, or with
    /// `extend` (Shift held) moves the range selection's nearer edge there.
    LanePress {
        /// The track whose lane was pressed.
        track: TrackId,
        /// The timeline frame pressed.
        frame: u64,
        /// Whether Shift was held.
        extend: bool,
    },
    /// The pointer, dragging from a lane press, is over a lane of `track` at `frame`.
    LaneDrag {
        /// The track under the pointer.
        track: TrackId,
        /// The timeline frame under the pointer.
        frame: u64,
    },
    /// The mouse button was released, ending a drag from a lane.
    LaneRelease,
    /// Clear the range selection.
    ClearRange,
    /// Cut the selected range to the clipboard, as one step.
    Cut,
    /// Copy the selected range to the clipboard.
    Copy,
    /// Paste the clipboard at the playhead, onto the top selected track or the current track,
    /// as one step.
    Paste,
    /// Delete the selected range, as one step.
    DeleteRange,
    /// Turn mono listening on (`true`) or off.
    SetMono(bool),
    /// Turn the click on (`true`) or off.
    SetClick(bool),
    /// The tempo being typed, in beats per minute.
    TempoText(String),
    /// Record the typed tempo as one step.
    CommitTempo,
    /// Set the meter's beats per bar, as one step.
    SetBeatsPerBar(u8),
    /// Set the meter's beat unit, as one step.
    SetBeatUnit(u8),
    /// Use this click sound, remembered on this machine.
    SetClickSound(engine_bridge::ClickSound),
    /// The click level slider moved, in decibels: applied live, remembered on
    /// [`Message::ReleaseClickLevel`].
    ClickLevel(f32),
    /// The click level slider was released: remember the level.
    ReleaseClickLevel,
    /// Count in this many bars (0, 1 or 2) before recording.
    SetCountIn(u32),
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
    /// Show the bars and beats grid over the lanes (`true`) or hide it.
    SetGrid(bool),
    /// Switch between light and dark chrome.
    ToggleTone,
    /// The window's scale factor, at launch or when it changes: physical pixels per logical
    /// pixel.
    Rescaled(f32),
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
    /// Show the new project panel; the window's shell handles it.
    NewProject,
    /// Create the new project at this sample rate, in hertz; the window's shell handles it.
    NewProjectRate(u32),
    /// Choose where to create the new project, and switch to it; the window's shell handles it.
    CreateProject,
    /// Close the new project panel; the window's shell handles it.
    CancelNewProject,
    /// Show the Open browser, listing the projects folder; the window's shell handles it.
    OpenProject,
    /// Open this project from the Open browser's list; the window's shell handles it.
    OpenListed(PathBuf),
    /// Choose a project anywhere through a folder dialog, and switch to it; the window's shell
    /// handles it.
    BrowseProjects,
    /// Close the Open browser; the window's shell handles it.
    CancelOpen,
    /// Undo the latest step.
    Undo,
    /// Redo the latest undone step.
    Redo,
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
    /// Ask to confirm deleting a node: its dialog's "Delete" button.
    AskDeleteNode(NodeId),
    /// Close the delete confirmation without deleting.
    CancelDelete,
    /// The text being typed in a node dialog's field: a feedback delay's time in seconds, or a
    /// mixer's channel count.
    NodeText(NodeId, String),
    /// Record the typed field as one step.
    CommitNodeText,
    /// Export the mix, through a file dialog; the mono mix if `true`.
    Export(bool),
    /// Use this audio device for input and output.
    Device(crate::services::DeviceChoice),
    /// Re-read the devices present, so one plugged in since shows.
    RefreshDevices,
    /// A parameter's slider moved: applied live, recorded on [`Message::Release`].
    Slide {
        /// The mixer or effect node.
        node: NodeId,
        /// The parameter's name, such as `"channel 1 gain"`.
        name: String,
        /// The value.
        value: f32,
    },
    /// The slider being dragged was released: record the value as one undo step.
    Release,
    /// Nothing: what a disabled control sends.
    Ignore,
    /// Move the open dialog's keyboard focus to its next control (Tab).
    FocusNext,
    /// Move the open dialog's keyboard focus to its previous control (Shift-Tab).
    FocusPrevious,
    /// Adjust the focused control up (`1`) or down (`-1`) a step (the arrow keys).
    FocusAdjust(i32),
    /// Step the focused effect slider up (`1`) or down (`-1`), as [`Message::FocusAdjust`] does;
    /// other controls ignore it (the right and left arrows).
    FocusSlide(i32),
    /// Press the focused button or toggle (Space or Enter); with a delete confirmation open,
    /// cancel it, its default.
    FocusPress,
    /// Give the open dialog's keyboard focus to a control, or take it away: a press on a control
    /// or on the dialog's background.
    SetFocus(Option<DialogFocus>),
    /// A mixer channel's mute or solo button: set the channel to `mode`, or back to playing if it
    /// is already. Soloing a channel unsolos the mixer's others in the same undo step.
    ToggleMode {
        /// The mixer node.
        node: NodeId,
        /// The channel index, from 0.
        channel: usize,
        /// [`Mode::Mute`] or [`Mode::Solo`].
        mode: Mode,
    },
    /// Change a mixer's channel count by `1` or `-1`, within [`App::channel_range`], as one
    /// step (the channel field's up and down buttons).
    StepMixerChannels(NodeId, i32),
    /// A button or choice set a parameter: applied and recorded as one undo step.
    SetParameter {
        /// The mixer or effect node.
        node: NodeId,
        /// The parameter's name, such as `"channel 1 mode"`.
        name: String,
        /// The new value.
        value: f32,
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
    /// Whether the lanes were left out of date while a node dialog covered them, to be
    /// recomputed when it closes.
    lanes_stale: bool,
    pub(crate) display: Display,
    /// Whether the bars and beats grid is drawn over the lanes; not saved.
    pub(crate) grid: bool,
    pub(crate) tone: Tone,
    /// The window's physical pixels per logical pixel, which the lanes are drawn at.
    pub(crate) scale_factor: f32,
    pub(crate) menu: Option<Menu>,
    /// The track being renamed, and the name typed so far.
    pub(crate) renaming: Option<(TrackId, String)>,
    /// How far the tracks, gutter and board are scrolled down, in logical pixels.
    pub(crate) scrolled: f32,
    /// The track pane's share of the window's width.
    pub(crate) split: f32,
    /// The selected node or wire.
    pub(crate) selection: Option<Selection>,
    /// The range selected in the lanes.
    range: Option<RangeSelection>,
    /// Where a drag from a lane began, while the button is held.
    drag: Option<(TrackId, u64)>,
    /// The track last pressed: where a paste goes without a range selection.
    current_track: Option<TrackId>,
    /// The excerpt last cut or copied.
    clipboard: Option<Excerpt>,
    /// The output a wire is being dragged from.
    pub(crate) wiring: Option<Pin>,
    /// A node or the outputs being dragged.
    pub(crate) moving: Option<Moving>,
    /// A node dialog's field being typed: a feedback delay's time, or a mixer's channel count.
    pub(crate) node_text: Option<(NodeId, String)>,
    /// The tracks the engine's feeds were made from.
    pub(crate) loaded: Option<Vec<Track>>,
    pub(crate) live: Option<Live>,
    /// The open dialog's control with the keyboard focus.
    pub(crate) focus: Option<DialogFocus>,
    /// The node whose deletion is waiting to be confirmed.
    pub(crate) confirming_delete: Option<NodeId>,
    /// The audio devices present, and the one in use.
    pub(crate) devices: (Vec<crate::services::DeviceChoice>, Option<String>),
    /// The last error or notice, for the status line.
    pub(crate) status: Option<String>,
    /// The session revision last saved, or last tried; autosave writes when it moves on.
    saved_revision: u64,
    /// The armed tracks' settings; not saved.
    pub(crate) arming: BTreeMap<TrackId, Arming>,
    /// Whether the device's inputs have been asked for.
    pub(crate) input_open: bool,
    /// The input channels open on the device.
    pub(crate) inputs: usize,
    /// A recording pass whose files are still being written.
    pub(crate) pass: Option<Pass>,
    /// Each armed track's input meter, one level per channel in dBFS.
    pub(crate) meters: BTreeMap<TrackId, Vec<f32>>,
    /// The click, and the tempo being typed.
    pub(crate) click: Click,
    /// Each built-in effect kind's parameters, read once at launch.
    effect_parameters: BTreeMap<&'static str, Vec<ParameterInfo>>,
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
        mut services: Box<dyn Services>,
    ) -> App {
        let preferences = services.click_preferences();
        engine.set_click_level(preferences.level_db);
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
            lanes_stale: false,
            generation: 0,
            display: Display::default(),
            grid: false,
            // Dark at launch, whatever the system's appearance.
            tone: Tone::Dark,
            scale_factor: 1.0,
            menu: None,
            renaming: None,
            scrolled: 0.0,
            split: DEFAULT_SPLIT,
            selection: None,
            range: None,
            drag: None,
            current_track: None,
            clipboard: None,
            wiring: None,
            moving: None,
            node_text: None,
            loaded: None,
            live: None,
            focus: None,
            confirming_delete: None,
            devices: (Vec::new(), None),
            status: None,
            saved_revision: 0,
            arming: BTreeMap::new(),
            input_open: false,
            inputs: 0,
            pass: None,
            meters: BTreeMap::new(),
            click: Click::new(preferences),
            effect_parameters: audio_effects::EFFECTS
                .iter()
                .filter_map(|e| Some((e.id, audio_effects::parameter_infos(e.id)?)))
                .collect(),
        };
        app.saved_revision = app.session.revision();
        app.refresh_devices();
        app.refresh_lanes();
        app.sync_click();
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

    /// The window's physical pixels per logical pixel, which the lanes are drawn at.
    pub fn scale_factor(&self) -> f32 {
        self.scale_factor
    }

    /// Whether the bars and beats grid is shown over the lanes.
    pub fn grid(&self) -> bool {
        self.grid
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

    /// The selected range: its tracks, top first, and frames.
    pub fn selected_range(&self) -> Option<(Vec<TrackId>, Range)> {
        self.range?.resolve(self.session.state())
    }

    /// Whether a drag from a lane is under way.
    pub fn is_dragging(&self) -> bool {
        self.drag.is_some()
    }

    /// The excerpt last cut or copied.
    pub fn clipboard(&self) -> Option<&Excerpt> {
        self.clipboard.as_ref()
    }

    /// The track a paste goes onto: the top selected track, or the current track.
    pub fn paste_target(&self) -> Option<TrackId> {
        let selected = self.selected_range().and_then(|(t, _)| t.first().copied());
        selected.or(self
            .current_track
            .filter(|&id| self.session.state().track(id).is_some()))
    }

    /// The channel counts mixer `node` may be given: from one past its highest connected input
    /// (at least 1) to [`audio_effects::MAX_CHANNELS`]; `None` if it is not a mixer.
    pub fn channel_range(&self, node: NodeId) -> Option<RangeInclusive<usize>> {
        let state = self.session.state();
        let NodeKind::Mixer { .. } = state.node(node)?.kind else {
            return None;
        };
        let min = state
            .connections
            .iter()
            .filter(|c| c.to == Endpoint::Node(node))
            .map(|c| c.input + 1)
            .max()
            .unwrap_or(1)
            .max(1);
        Some(min..=audio_effects::MAX_CHANNELS)
    }

    /// The open menu, if any.
    pub fn menu(&self) -> Option<Menu> {
        self.menu
    }

    /// Whether the interface needs a tick every display frame: while playing or starting.
    pub fn wants_ticks(&self) -> bool {
        // While armed, ticks drive the input meters.
        self.engine.is_playing() || self.recording_pending() || !self.arming.is_empty()
    }

    /// Handles `message`, then saves the project if it changed
    /// (`docs/architecture/project.md` "Saving").
    pub fn update(&mut self, message: Message) {
        self.handle(message);
        if self.lanes_stale && !self.node_dialog_open() {
            self.refresh_lanes();
        }
        // Any change of tempo, by an edit, undo or a restored version, reaches the click.
        self.sync_click();
        self.autosave();
    }

    /// Saves the project if the session changed since the last save or attempt. A failure goes
    /// to the status line and is not retried until the next change.
    fn autosave(&mut self) {
        let revision = self.session.revision();
        if revision == self.saved_revision {
            return;
        }
        self.saved_revision = revision;
        if let Err(message) = self.services.save(&self.session) {
            self.status = Some(format!("Autosave failed: {message}"));
        }
    }

    /// Handles `message`, without saving.
    fn handle(&mut self, message: Message) {
        // A frame tick, a scroll or typing leaves the status line and the open menu alone;
        // anything else replaces the status and closes the menu, except the click dialog's own
        // controls, which leave it open.
        let in_click_dialog = self.menu == Some(Menu::Click)
            && matches!(
                message,
                Message::SetClick(_)
                    | Message::CommitTempo
                    | Message::SetBeatsPerBar(_)
                    | Message::SetBeatUnit(_)
                    | Message::SetClickSound(_)
                    | Message::SetCountIn(_)
            );
        if !matches!(
            message,
            Message::Tick
                | Message::Scrolled(_)
                | Message::Rescaled(_)
                | Message::RenameText(_)
                | Message::OpenMenu(_)
                | Message::RefreshDevices
                | Message::DragNode(..)
                | Message::DragOutput(_)
                | Message::NodeText(..)
                | Message::LaneDrag { .. }
                | Message::LaneRelease
                | Message::TempoText(_)
                | Message::ClickLevel(_)
                | Message::ReleaseClickLevel
                | Message::FocusNext
                | Message::FocusPrevious
                | Message::FocusAdjust(_)
                | Message::FocusSlide(_)
                | Message::FocusPress
                | Message::SetFocus(_)
        ) {
            self.status = None;
            if !in_click_dialog {
                self.menu = None;
            }
        }
        // Doing anything else while typing a tempo records it.
        if !matches!(
            message,
            Message::Tick | Message::Scrolled(_) | Message::Rescaled(_) | Message::TempoText(_)
        ) {
            self.finish_tempo();
        }
        // Doing anything else while renaming a track finishes the rename.
        if !matches!(
            message,
            Message::Tick
                | Message::Scrolled(_)
                | Message::Rescaled(_)
                | Message::RenameText(_)
                | Message::FinishRename
        ) {
            self.finish_rename();
        }
        match message {
            Message::Play => self.play(),
            Message::Record => self.record(),
            Message::Arm(track) => self.toggle_arm(track),
            Message::Monitor(track) => self.toggle_monitor(track),
            Message::Input(track, choice) => self.set_input(track, choice.input),
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
            Message::LanePress {
                track,
                frame,
                extend,
            } => self.lane_press(track, frame, extend),
            Message::LaneDrag { track, frame } => self.lane_drag(track, frame),
            Message::LaneRelease => self.drag = None,
            Message::ClearRange if self.confirming_delete.is_some() => {
                self.confirming_delete = None;
            }
            Message::ClearRange => self.range = None,
            Message::Cut => self.cut(),
            Message::Copy => {
                if let Some((tracks, range)) = self.selected_range() {
                    let copied = self.session.copy_range(&tracks, range);
                    match copied {
                        Ok(excerpt) => self.clipboard = Some(excerpt),
                        Err(error) => self.status = Some(error.to_string()),
                    }
                }
            }
            Message::Paste => self.paste(),
            Message::DeleteRange => self.delete_range(),
            Message::SetMono(mono) => self.engine.set_mono(mono),
            Message::SetClick(on) => self.engine.set_click_on(on),
            Message::TempoText(text) => self.click.tempo_text = Some(text),
            // Recorded by `finish_tempo` above.
            Message::CommitTempo => {}
            Message::SetBeatsPerBar(beats) => self.set_meter(Some(beats), None),
            Message::SetBeatUnit(unit) => self.set_meter(None, Some(unit)),
            Message::SetClickSound(sound) => self.set_click_sound(sound),
            Message::ClickLevel(db) => self.set_click_level(db),
            Message::ReleaseClickLevel => self.save_click_preferences(),
            Message::SetCountIn(bars) => self.set_count_in(bars),
            Message::Tick => {
                self.engine.collect();
                self.poll_recording();
                self.update_meters();
                self.stop_at_end();
                self.follow_playhead();
                let result = self.bridge.commit();
                self.report(result.map_err(|e| e.to_string()));
                if let Some(error) = self.services.audio_error() {
                    // Pause, holding the playhead; a take being recorded ends here.
                    let _ = self.engine.pause();
                    let mut notice = format!("Audio stopped: {error}. Paused");
                    if self.services.audio_lost() {
                        // Its device is gone: carry on with the remembered or default device.
                        self.recover_audio(None);
                        if let Some(device) = self.current_device_name() {
                            notice.push_str(&format!("; now using {device}"));
                        }
                    }
                    notice.push('.');
                    self.status = Some(self.status.take().unwrap_or(notice));
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
                let before = self.start;
                let by = (f64::from(fraction.abs()) * self.view_span() as f64) as u64;
                self.start = if fraction < 0.0 {
                    self.start.saturating_sub(by)
                } else {
                    self.start.saturating_add(by)
                };
                // Forward panning stops at the end, but never pulls back a view already past it.
                self.start = self.start.min(self.max_start().max(before));
                self.refresh_lanes();
            }
            Message::SetDisplay(display) => {
                self.display = display;
                self.refresh_lanes();
            }
            Message::SetGrid(grid) => self.grid = grid,
            Message::ToggleTone => self.set_tone(self.tone.toggled()),
            Message::Rescaled(scale) => self.set_scale_factor(scale),
            Message::OpenMenu(menu) => {
                self.menu = (self.menu != Some(menu)).then_some(menu);
                // The device menu lists the devices present now, so one plugged in since shows.
                if self.menu == Some(Menu::Device) {
                    self.refresh_devices();
                }
            }
            // The shell handles these.
            Message::CloseMenu
            | Message::Quit
            | Message::NewProject
            | Message::NewProjectRate(_)
            | Message::CreateProject
            | Message::CancelNewProject
            | Message::OpenProject
            | Message::OpenListed(_)
            | Message::BrowseProjects
            | Message::CancelOpen => {}
            Message::Scrolled(offset) => self.scrolled = offset,
            Message::Split(share) => self.split = share.clamp(MIN_SPLIT, MAX_SPLIT),
            Message::Undo | Message::Redo => self.history(message == Message::Undo),
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
                if self.arming.contains_key(&track) {
                    self.sync_arming();
                }
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
                // A click on the board leaves any field being typed.
                self.node_text = None;
                self.focus = None;
                self.confirming_delete = None;
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
                if self.renaming.is_some() || self.node_text.is_some() {
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
                    Ok(node) => {
                        self.focus = None;
                        self.selection = Some(Selection::Node(node));
                    }
                    Err(error) => self.status = Some(error.to_string()),
                }
            }
            Message::AskDeleteNode(node) => self.confirming_delete = Some(node),
            Message::CancelDelete => self.confirming_delete = None,
            Message::DeleteNode(node) => {
                self.confirming_delete = None;
                let result = self.bridge.edit(&mut self.session, |s| s.remove_node(node));
                self.report(result.map_err(|e| e.to_string()));
                if self.selection == Some(Selection::Node(node)) {
                    self.selection = None;
                }
            }
            Message::NodeText(node, text) => {
                if self.dialog_mixer().is_some_and(|(mixer, _)| mixer == node) {
                    self.focus = Some(DialogFocus::Mixer(MixerFocus::Channels));
                }
                self.node_text = Some((node, text));
            }
            Message::StepMixerChannels(node, delta) => self.step_channels(node, delta),
            // The confirmation holds the keyboard while open.
            Message::FocusNext
            | Message::FocusPrevious
            | Message::FocusAdjust(_)
            | Message::FocusSlide(_)
                if self.confirming_delete.is_some() => {}
            Message::FocusPress if self.confirming_delete.is_some() => {
                self.confirming_delete = None;
            }
            Message::FocusNext | Message::FocusPrevious => {
                let forward = matches!(message, Message::FocusNext);
                let next = if let Some((_, channels)) = self.dialog_mixer() {
                    DialogFocus::Mixer(MixerFocus::moved(self.mixer_focus(), channels, forward))
                } else if let Some((_, infos)) = self.dialog_effect() {
                    let order = effect_panel::tab_order(infos);
                    match effect_panel::moved(self.effect_focus(), &order, forward) {
                        Some(i) => DialogFocus::Parameter(i),
                        None => return,
                    }
                } else {
                    return;
                };
                self.set_focus(Some(next));
            }
            Message::SetFocus(focus) => {
                let open = match focus {
                    None => true,
                    Some(DialogFocus::Mixer(_)) => self.dialog_mixer().is_some(),
                    Some(DialogFocus::Parameter(_)) => self.dialog_effect().is_some(),
                };
                if open {
                    self.set_focus(focus);
                }
            }
            Message::FocusSlide(delta) => {
                let slider = self.dialog_effect().zip(self.effect_focus()).is_some_and(
                    |((_, infos), index)| {
                        infos
                            .get(index)
                            .is_some_and(|i| effect_panel::is_slider(&i.scale))
                    },
                );
                if slider {
                    self.adjust_parameter_focus(delta);
                }
            }
            Message::FocusAdjust(delta) => {
                if self.dialog_mixer().is_some() {
                    self.adjust_focus(delta);
                } else {
                    self.adjust_parameter_focus(delta);
                }
            }
            Message::FocusPress if self.dialog_effect().is_some() => self.press_parameter_focus(),
            Message::FocusPress => {
                let Some((node, _)) = self.dialog_mixer() else {
                    return;
                };
                let (channel, mode) = match self.mixer_focus() {
                    Some(MixerFocus::Mute(ch)) => (ch, Mode::Mute),
                    Some(MixerFocus::Solo(ch)) => (ch, Mode::Solo),
                    _ => return,
                };
                self.toggle_mode(node, channel, mode);
            }
            Message::ToggleMode {
                node,
                channel,
                mode,
            } => self.toggle_mode(node, channel, mode),
            Message::CommitNodeText => {
                if let Some((node, text)) = self.node_text.take() {
                    self.commit_node_text(node, &text);
                }
            }
            Message::Export(mono) => match self.services.export(self.session.state(), mono) {
                Ok(Some(name)) => self.status = Some(format!("Exported {name}")),
                Ok(None) => {}
                Err(message) => self.status = Some(message),
            },
            Message::RefreshDevices => self.refresh_devices(),
            Message::Ignore => {}
            Message::Device(choice) => {
                if self.engine.is_playing() {
                    self.status =
                        Some("Pause or stop playback to change the audio device.".to_owned());
                } else if self.services.audio_lost() {
                    self.recover_audio(Some(&choice.id));
                } else {
                    let result = self.services.set_device(&choice.id);
                    self.report(result);
                    self.refresh_devices();
                    self.inputs = self.services.input_channels();
                }
            }
            Message::Slide { node, ref name, .. }
            | Message::SetParameter { node, ref name, .. }
                if self.engine.is_playing() && self.is_stopped_only(node, name) =>
            {
                self.status = Some(format!("Pause or stop playback to change the {name}."));
            }
            Message::Slide { node, name, value } => {
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
            Message::SetParameter { node, name, value } => {
                let result = self.bridge.edit(&mut self.session, |s| {
                    s.set_parameter(node, &name, Some(value))
                });
                self.report(result.map_err(|e| e.to_string()));
            }
        }
    }

    /// The selected node and its channel count, if it is a mixer: the mixer whose dialog is open.
    pub(crate) fn dialog_mixer(&self) -> Option<(NodeId, usize)> {
        let Some(Selection::Node(id)) = self.selection else {
            return None;
        };
        self.channel_count(id).map(|channels| (id, channels))
    }

    /// Mixer `node`'s channel count; `None` if it is not a mixer.
    fn channel_count(&self, node: NodeId) -> Option<usize> {
        match self.session.state().node(node)?.kind {
            NodeKind::Mixer { channels, .. } => Some(channels),
            _ => None,
        }
    }

    /// The selected node and its kind's parameters, if it is a built-in effect this build
    /// knows: the effect whose dialog is open.
    pub(crate) fn dialog_effect(&self) -> Option<(NodeId, &[ParameterInfo])> {
        let Some(Selection::Node(id)) = self.selection else {
            return None;
        };
        match &self.session.state().node(id)?.kind {
            NodeKind::Effect { kind, .. } => self.effect_parameters(kind).map(|infos| (id, infos)),
            _ => None,
        }
    }

    /// The open dialog's control with the keyboard focus, if any.
    pub fn focus(&self) -> Option<DialogFocus> {
        self.focus
    }

    /// The mixer dialog control with the keyboard focus, if any.
    pub fn mixer_focus(&self) -> Option<MixerFocus> {
        match self.focus {
            Some(DialogFocus::Mixer(focus)) => Some(focus),
            _ => None,
        }
    }

    /// The effect dialog control with the keyboard focus, if any, by index into its kind's
    /// parameters.
    pub fn effect_focus(&self) -> Option<usize> {
        match self.focus {
            Some(DialogFocus::Parameter(index)) => Some(index),
            _ => None,
        }
    }

    /// Moves the open dialog's keyboard focus to `focus`; leaving a mixer's "Channels" field
    /// records what was typed in it.
    fn set_focus(&mut self, focus: Option<DialogFocus>) {
        let channels = Some(DialogFocus::Mixer(MixerFocus::Channels));
        if self.focus == channels
            && focus != self.focus
            && let Some((node, text)) = self.node_text.take()
        {
            self.commit_node_text(node, &text);
        }
        self.focus = focus;
    }

    /// Steps the focused effect control by `delta` presses, as one undo step
    /// ([`effect_panel::stepped`]).
    fn adjust_parameter_focus(&mut self, delta: i32) {
        let (Some((node, infos)), Some(index)) = (self.dialog_effect(), self.effect_focus()) else {
            return;
        };
        let Some(info) = infos.get(index) else {
            return;
        };
        if !self.parameter_enabled(node, infos, info) {
            return;
        }
        let current = self.parameter_value(node, &info.name);
        let Some(value) = effect_panel::stepped(info, current, delta) else {
            return;
        };
        let name = info.name.clone();
        self.update(Message::SetParameter { node, name, value });
    }

    /// Whether parameter `info` of effect `node`, one of `infos`, responds now
    /// ([`effect_panel::is_enabled`]).
    pub(crate) fn parameter_enabled(
        &self,
        node: NodeId,
        infos: &[ParameterInfo],
        info: &ParameterInfo,
    ) -> bool {
        let values: Vec<f32> = infos
            .iter()
            .map(|i| self.parameter_value(node, &i.name))
            .collect();
        effect_panel::is_enabled(info, infos, &values)
    }

    /// Flips the focused effect control if it is a toggle, as one undo step.
    fn press_parameter_focus(&mut self) {
        let (Some((node, infos)), Some(index)) = (self.dialog_effect(), self.effect_focus()) else {
            return;
        };
        let Some(info) = infos.get(index).filter(|i| i.scale == Scale::Toggle) else {
            return;
        };
        if !self.parameter_enabled(node, infos, info) {
            return;
        }
        let on = self.parameter_value(node, &info.name) >= 0.5;
        let name = info.name.clone();
        let value = if on { 0.0 } else { 1.0 };
        self.update(Message::SetParameter { node, name, value });
    }

    /// Sets mixer `node`'s channel `channel` to `mode`, or back to playing if it is already, as
    /// one undo step; soloing it unsolos the mixer's other channels in the same step.
    fn toggle_mode(&mut self, node: NodeId, channel: usize, mode: Mode) {
        let Some(channels) = self.channel_count(node) else {
            return;
        };
        let mode_of = |app: &Self, ch: usize| {
            Mode::from_value(app.parameter_value(node, &parameter_name(ch, Control::Mode)))
        };
        let set = |ch: usize, mode: Mode| Edit::SetParameter {
            node,
            name: parameter_name(ch, Control::Mode),
            value: Some(mode.value()),
        };
        let wanted = toggled(mode_of(self, channel), mode);
        let mut edits = vec![set(channel, wanted)];
        if wanted == Mode::Solo {
            edits.extend(
                (0..channels)
                    .filter(|&ch| ch != channel && mode_of(self, ch) == Mode::Solo)
                    .map(|ch| set(ch, Mode::Play)),
            );
        }
        let edit = match edits.len() {
            1 => edits.remove(0),
            _ => Edit::Batch(edits),
        };
        let result = self.bridge.edit(&mut self.session, |s| s.apply(edit));
        self.report(result.map_err(|e| e.to_string()));
    }

    /// Changes mixer `node`'s channel count by `delta`, within [`App::channel_range`], as one
    /// step; what was typed in its field is dropped.
    fn step_channels(&mut self, node: NodeId, delta: i32) {
        let (Some(channels), Some(range)) = (self.channel_count(node), self.channel_range(node))
        else {
            return;
        };
        if self.node_text.as_ref().is_some_and(|(n, _)| *n == node) {
            self.node_text = None;
        }
        let wanted = channels
            .saturating_add_signed(delta as isize)
            .clamp(*range.start(), *range.end());
        if wanted != channels {
            let result = self.bridge.edit(&mut self.session, |s| {
                s.apply(Edit::SetMixerChannels {
                    node,
                    channels: wanted,
                })
            });
            self.report(result.map_err(|e| e.to_string()));
        }
    }

    /// Adjusts the focused mixer control by `delta` steps, each recorded as one undo step: the
    /// channel count by one, gain by [`GAIN_STEP_DB`], pan by [`PAN_STEP`] (up is left).
    fn adjust_focus(&mut self, delta: i32) {
        let (Some((mixer, _)), Some(focus)) = (self.dialog_mixer(), self.mixer_focus()) else {
            return;
        };
        let (name, value) = match focus {
            MixerFocus::Channels => return self.step_channels(mixer, delta),
            MixerFocus::Gain(ch) => {
                let name = parameter_name(ch, Control::Gain);
                let gain = self.parameter_value(mixer, &name);
                let value = ((gain / GAIN_STEP_DB).round() + delta as f32) * GAIN_STEP_DB;
                (name, value.clamp(SILENCE_DB, MAX_GAIN_DB))
            }
            MixerFocus::Pan(ch) => {
                let name = parameter_name(ch, Control::Pan);
                let pan = self.parameter_value(mixer, &name);
                let value = ((pan / PAN_STEP).round() - delta as f32) * PAN_STEP;
                (name, value.clamp(-1.0, 1.0))
            }
            MixerFocus::Mute(_) | MixerFocus::Solo(_) => return,
        };
        self.update(Message::SetParameter {
            node: mixer,
            name,
            value,
        });
    }

    /// Records `text`, typed in node `node`'s dialog field, as one step: a feedback delay's time,
    /// or a mixer's channel count clamped into [`App::channel_range`].
    fn commit_node_text(&mut self, node: NodeId, text: &str) {
        let kind = self.session.state().node(node).map(|n| &n.kind);
        let edit = match kind {
            Some(NodeKind::FeedbackDelay { .. }) => match text.trim().parse::<f64>() {
                Ok(seconds) => Edit::SetDelay { node, seconds },
                Err(_) => {
                    self.status = Some(format!("{text:?} is not a time in seconds"));
                    return;
                }
            },
            Some(&NodeKind::Mixer { channels, .. }) => {
                let Ok(typed) = text.trim().parse::<usize>() else {
                    self.status = Some(format!("{text:?} is not a channel count"));
                    return;
                };
                let Some(range) = self.channel_range(node) else {
                    return;
                };
                let wanted = typed.clamp(*range.start(), *range.end());
                if wanted == channels {
                    return;
                }
                Edit::SetMixerChannels {
                    node,
                    channels: wanted,
                }
            }
            _ => return,
        };
        let result = self.bridge.edit(&mut self.session, |s| s.apply(edit));
        self.report(result.map_err(|e| e.to_string()));
    }

    /// The parameters of built-in effect kind `kind`, or `None` for a kind this build does not
    /// know.
    pub(crate) fn effect_parameters(&self, kind: &str) -> Option<&[ParameterInfo]> {
        self.effect_parameters.get(kind).map(Vec::as_slice)
    }

    /// Whether parameter `name` of node `node` may change only while playback is stopped or
    /// paused (ADR-0088).
    pub(crate) fn is_stopped_only(&self, node: NodeId, name: &str) -> bool {
        self.bridge
            .parameter(node, name)
            .is_some_and(|p| p.info().stopped_only)
    }

    /// A mixer or effect parameter's value as shown: the one being dragged, else the model's,
    /// else the running parameter's default, else 0.
    pub fn parameter_value(&self, node: NodeId, name: &str) -> f32 {
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
                NodeKind::Mixer { parameters, .. } | NodeKind::Effect { parameters, .. } => {
                    parameters.get(name).copied()
                }
                _ => None,
            })
            .or_else(|| self.bridge.parameter(node, name).map(|p| p.info().default))
            .unwrap_or(0.0)
    }

    fn play(&mut self) {
        // From the end there is nothing to play: start from the return point, or from the top
        // if that is at the end too.
        let end = self.project_end();
        if !self.engine.is_playing() && self.engine.playhead() >= end {
            let from = if self.return_point < end {
                self.return_point
            } else {
                0
            };
            let result = self.engine.locate(from, self.services.streams());
            self.report(result.map_err(|e| e.to_string()));
        }
        if !self.engine.is_playing() && !self.load_feeds() {
            return;
        }
        let result = self.engine.play(self.services.streams());
        self.report(result.map_err(|e| e.to_string()));
    }

    /// Gives the engine feeds for the tracks if they changed since it last took them. Returns
    /// `false`, with the reason on the status line, if that failed. Only while stopped.
    pub(crate) fn load_feeds(&mut self) -> bool {
        if self.loaded.as_ref() == Some(&self.session.state().tracks) {
            return true;
        }
        let sources = self.bridge.sources();
        match self.services.track_feeds(self.session.state(), &sources) {
            Ok(feeds) => {
                if let Err(error) = self.engine.set_tracks(feeds) {
                    self.status = Some(error.to_string());
                    return false;
                }
                self.loaded = Some(self.session.state().tracks.clone());
                true
            }
            Err(message) => {
                self.status = Some(message);
                false
            }
        }
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

    /// A press in a lane: with `extend`, moves the selection's nearer edge (or selects from the
    /// playhead on the current track); otherwise seeks, clears the selection and starts a drag.
    fn lane_press(&mut self, track: TrackId, frame: u64, extend: bool) {
        if extend {
            let state = self.session.state();
            let selection = match self.range.and_then(|r| r.extended(state, (track, frame))) {
                Some(selection) => selection,
                None => RangeSelection {
                    anchor: (self.paste_target().unwrap_or(track), self.engine.playhead()),
                    head: (track, frame),
                },
            };
            self.range = Some(selection);
            self.drag = Some(selection.anchor);
            return;
        }
        self.seek(frame);
        self.current_track = Some(track);
        self.range = None;
        self.drag = Some((track, frame));
    }

    /// The pointer, dragging, is over `track` at `frame`: the selection runs from the drag's
    /// start to there, once it has gone further than a click's wobble.
    fn lane_drag(&mut self, track: TrackId, frame: u64) {
        let Some(anchor) = self.drag else {
            return;
        };
        let wobble = self.samples_per_column().saturating_mul(DRAG_COLUMNS);
        if self.range.is_none() && track == anchor.0 && frame.abs_diff(anchor.1) < wobble {
            return;
        }
        self.range = Some(RangeSelection {
            anchor,
            head: (track, frame),
        });
    }

    fn cut(&mut self) {
        let Some((tracks, range)) = self.selected_range() else {
            return;
        };
        if self.refuse_while_playing("cut") {
            return;
        }
        let result = self
            .bridge
            .edit(&mut self.session, |s| s.cut_range(&tracks, range));
        match result {
            Ok(excerpt) => self.clipboard = Some(excerpt),
            Err(error) => self.status = Some(error.to_string()),
        }
        self.refresh_lanes();
    }

    fn delete_range(&mut self) {
        // Delete and Backspace edit the text being typed instead.
        if self.renaming.is_some() || self.node_text.is_some() {
            return;
        }
        let Some((tracks, range)) = self.selected_range() else {
            return;
        };
        if self.refuse_while_playing("delete") {
            return;
        }
        let result = self
            .bridge
            .edit(&mut self.session, |s| s.delete_range(&tracks, range));
        self.report(result.map_err(|e| e.to_string()));
        self.refresh_lanes();
    }

    fn paste(&mut self) {
        let (Some(excerpt), Some(track)) = (self.clipboard.clone(), self.paste_target()) else {
            return;
        };
        if self.refuse_while_playing("paste") {
            return;
        }
        let position = self.engine.playhead();
        let result = self
            .bridge
            .edit(&mut self.session, |s| s.paste(&excerpt, track, position));
        self.report(result.map(|_| ()).map_err(|e| e.to_string()));
        self.refresh_lanes();
    }

    /// Moves the playhead to `frame` and makes it the return point, bringing it into view.
    fn seek(&mut self, frame: u64) {
        let result = self.engine.locate(frame, self.services.streams());
        self.report(result.map_err(|e| e.to_string()));
        self.return_point = frame;
        let span = self.view_span();
        if frame < self.start || frame >= self.start.saturating_add(span) {
            // Centred, within the scroll bound; the engine may not report the new playhead
            // yet, so the bound takes in `frame` itself.
            let bound = self.scroll_bound().max(frame);
            self.start = frame.saturating_sub(span / 2).min(last_start(bound, span));
            self.refresh_lanes();
        }
    }

    /// Readies the project to be closed for another (`docs/architecture/user-interface.md`
    /// "Application startup"): finishes any typing, stops the transport, discards a pass still
    /// being recorded, disarms, and saves any change.
    pub fn close(&mut self) {
        self.finish_rename();
        self.finish_tempo();
        let _ = self.engine.stop();
        self.discard_pass();
        self.autosave();
    }

    /// Whether another project can be opened or created: stopped, nothing armed, and no take
    /// still being saved.
    pub fn can_switch_project(&self) -> bool {
        !self.engine.is_playing() && self.arming.is_empty() && !self.recording_pending()
    }

    /// Sets the window's scale factor: physical pixels per logical pixel. Ignores one that is
    /// not finite and positive.
    pub fn set_scale_factor(&mut self, scale: f32) {
        if scale.is_finite() && scale > 0.0 {
            self.scale_factor = scale;
        }
    }

    /// Uses the light or dark chrome.
    pub fn set_tone(&mut self, tone: Tone) {
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
    pub(crate) fn refuse_while_playing(&mut self, what: &str) -> bool {
        let playing = self.engine.is_playing();
        if playing {
            self.status = Some(format!("Stop playback to {what}."));
        }
        playing
    }

    /// The name of the device in use, if any.
    pub(crate) fn current_device_name(&self) -> Option<String> {
        let (choices, current) = &self.devices;
        choices
            .iter()
            .find(|c| Some(&c.id) == current.as_ref())
            .map(|c| c.name.clone())
    }

    /// The audio devices present when last read, for the device dropdown.
    pub fn device_choices(&self) -> &[crate::services::DeviceChoice] {
        &self.devices.0
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
        if !self.arming.is_empty() {
            // Undo or redo may have removed an armed track.
            self.sync_arming();
        }
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
        let within =
            self.start.saturating_add(self.view_span()) <= self.scroll_bound().saturating_add(1);
        self.zoom = zoom;
        self.start = zoomed_start(self.start, old, self.samples_per_column(), anchor);
        if within {
            self.start = self.start.min(self.max_start());
        }
        self.refresh_lanes();
    }

    /// Stops playback that has reached the project's end, leaving the playhead there; the
    /// return point is kept for Stop and the next Play. Recording runs on past the end.
    fn stop_at_end(&mut self) {
        let end = self.project_end();
        if !self.engine.is_playing() || self.engine.is_recording() || self.engine.playhead() < end {
            return;
        }
        let result = self
            .engine
            .stop()
            .and_then(|()| self.engine.locate(end, self.services.streams()));
        self.report(result.map_err(|e| e.to_string()));
    }

    /// Pages the view along with a playing playhead (`docs/architecture/user-interface.md`
    /// "Transport and seeking"): once it has been seen in view, reaching 80 % of the lane's
    /// width moves the view to put it at 20 %, or as far as the scroll bound allows. A playhead
    /// out of view that was not followed there is left alone.
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
            // Within the scroll bound, which takes in the playhead, and never backwards.
            let target = playhead.saturating_sub(span / 5).min(self.max_start());
            if target > self.start {
                self.start = target;
                self.refresh_lanes();
            }
        }
    }

    /// Recomputes every track's lanes for the visible stretch. While a node dialog covers them
    /// and the tracks and their channels are unchanged, it only marks them out of date, to be
    /// recomputed when the dialog closes (`docs/architecture/user-interface.md` "Layout").
    pub(crate) fn refresh_lanes(&mut self) {
        if self.node_dialog_open() && self.lanes_fit_tracks() {
            self.lanes_stale = true;
            return;
        }
        self.lanes_stale = false;
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

    /// Whether a node's dialog is open over the track pane.
    pub(crate) fn node_dialog_open(&self) -> bool {
        matches!(self.selection, Some(Selection::Node(id)) if self.session.state().node(id).is_some())
    }

    /// Whether the lanes are one per channel of each track, in track order, as the patch
    /// board's ports need.
    fn lanes_fit_tracks(&self) -> bool {
        let tracks = &self.session.state().tracks;
        tracks.len() == self.lanes.len()
            && tracks
                .iter()
                .zip(&self.lanes)
                .all(|(track, (id, lanes))| track.id == *id && lanes.len() == channels(track))
    }

    /// Where the playhead falls in the visible stretch, as a fraction of its width.
    pub(crate) fn playhead_fraction(&self) -> Option<f32> {
        let span = self.view_span();
        let at = self.engine.playhead().checked_sub(self.start)?;
        (at < span).then(|| at as f32 / span as f32)
    }

    /// The playhead's time for the bottom bar, as [`clock`](crate::transport::clock) formats it
    /// for this project's length.
    pub fn playhead_time(&self) -> String {
        let playhead = self.engine.playhead();
        let longest = self.project_end().max(playhead);
        crate::transport::clock(playhead, self.sample_rate, longest)
    }

    /// The timeline's length for the position bar: the end of the last clip, or further to
    /// take in the visible window and the playhead.
    pub(crate) fn timeline_length(&self) -> u64 {
        self.project_end()
            .max(self.start.saturating_add(self.view_span()))
            .max(self.engine.playhead())
            .max(1)
    }

    /// How far panning and zooming out may scroll (`docs/architecture/user-interface.md`
    /// "Transport and seeking"): the later of the project's end and the playhead.
    fn scroll_bound(&self) -> u64 {
        self.project_end().max(self.engine.playhead())
    }

    /// The furthest view start that panning, zooming out, paging and seeking reach: the scroll
    /// bound at the lane's last column, so a playhead there is drawn, or 0 when it fits.
    fn max_start(&self) -> u64 {
        last_start(self.scroll_bound(), self.view_span())
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

    pub(crate) fn report(&mut self, result: Result<(), String>) {
        if let Err(message) = result {
            self.status = Some(message);
        }
    }
}

/// The view start of `span` frames that puts frame `bound` last in the view, or 0.
fn last_start(bound: u64, span: u64) -> u64 {
    bound.saturating_add(1).saturating_sub(span)
}
