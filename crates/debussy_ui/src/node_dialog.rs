//! The node dialog: the keyboard focus and slider drag shared by the mixer and effect dialogs,
//! and what each of their messages asks of the project (`docs/architecture/user-interface.md`
//! "Gestures", ADR-0089, ADR-0096, ADR-0107). It holds no session: [`NodeDialog::update`] reads
//! the project through a [`Context`] and returns [`Command`]s for `App` to carry out.

use audio_effects::{Control, MAX_GAIN_DB, Mode, SILENCE_DB, parameter_name};
use project_model::{Edit, NodeId, NodeKind, State};

use crate::effect_panel;
use crate::mixer_strip::{GAIN_STEP_DB, MixerFocus, PAN_STEP, toggled};
use crate::node_parameters::{Live, NodeParameters, ParameterCatalog, tab_order};

/// The open dialog's control with the keyboard focus: Tab and Shift-Tab move it, the arrow keys
/// adjust it, and Space or Enter press it (ADR-0089).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialogFocus {
    /// A mixer dialog's control.
    Mixer(MixerFocus),
    /// An effect dialog's control, by index into its kind's parameters.
    Parameter(usize),
    /// The dialog's "Delete" button, last in Tab order.
    Delete,
}

/// Something the user did in a mixer's or effect's dialog.
#[derive(Debug, Clone, PartialEq)]
pub enum DialogMessage {
    /// A parameter's slider moved: applied live, recorded on [`DialogMessage::Release`].
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
    /// Move the open dialog's keyboard focus to its next control (Tab).
    FocusNext,
    /// Move the open dialog's keyboard focus to its previous control (Shift-Tab).
    FocusPrevious,
    /// Adjust the focused control up (`1`) or down (`-1`) a step (the arrow keys).
    FocusAdjust(i32),
    /// Step the focused effect slider up (`1`) or down (`-1`), as
    /// [`DialogMessage::FocusAdjust`] does; other controls ignore it (the right and left arrows).
    FocusSlide(i32),
    /// Press the focused button or toggle (Space or Enter); with a delete confirmation open,
    /// press its focused button.
    FocusPress,
    /// Give the open dialog's keyboard focus to a control, or take it away: a press on a control
    /// or on the dialog's background.
    SetFocus(Option<DialogFocus>),
    /// Preset `preset` (`audio_effects::PRESETS`) was chosen for effect node `node`: every
    /// parameter takes the preset's value or its default, as one undo step.
    ApplyPreset {
        /// The effect node.
        node: NodeId,
        /// The preset's name.
        preset: &'static str,
    },
    /// A button or choice set a parameter: applied and recorded as one undo step.
    SetParameter {
        /// The mixer or effect node.
        node: NodeId,
        /// The parameter's name, such as `"channel 1 mode"`.
        name: String,
        /// The new value.
        value: f32,
    },
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
}

impl DialogMessage {
    /// Whether this is a key the dialog's focus takes: Tab, Shift-Tab, the arrows, Space or
    /// Enter. An open delete confirmation takes these instead.
    pub(crate) fn is_key(&self) -> bool {
        matches!(
            self,
            DialogMessage::FocusNext
                | DialogMessage::FocusPrevious
                | DialogMessage::FocusAdjust(_)
                | DialogMessage::FocusSlide(_)
                | DialogMessage::FocusPress
        )
    }

    /// Whether this only moves or uses the keyboard focus, leaving the status line and the open
    /// menu alone: a focus key, or a press that gives or takes the focus.
    pub(crate) fn is_focus_key(&self) -> bool {
        self.is_key() || matches!(self, DialogMessage::SetFocus(_))
    }
}

/// What the dialog asks `App` to do; `App` owns the session, the bridge and the status line.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Command {
    /// Apply this edit as one undo step.
    Edit(Edit),
    /// Set the running graph's parameter, unrecorded: a slider being dragged.
    SetLive {
        /// The mixer or effect node.
        node: NodeId,
        /// The parameter's name.
        name: String,
        /// The value.
        value: f32,
    },
    /// Show this on the status line.
    Status(String),
    /// Change this mixer's channel count by this many, within its allowed range, as one step.
    StepChannels(NodeId, i32),
    /// Record what was typed in the node dialog's field: the focus left a mixer's "Channels".
    CommitNodeText,
    /// Delete this node: its delete confirmation's "Delete" was pressed.
    DeleteNode(NodeId),
}

/// The delete confirmation's button with the keyboard focus: what Space or Enter presses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConfirmFocus {
    /// "Cancel", the default.
    Cancel,
    /// "Delete".
    Delete,
}

/// What [`NodeDialog::update`] reads from the rest of the interface.
pub(crate) struct Context<'a> {
    /// The parameters of every mixer and built-in effect.
    pub catalog: &'a ParameterCatalog,
    /// The project.
    pub state: &'a State,
    /// Whether playback is running.
    pub playing: bool,
    /// The selected node: the one whose dialog is open, if any.
    pub open: Option<NodeId>,
}

/// The open node dialog's keyboard focus, the slider being dragged, and its delete
/// confirmation.
#[derive(Debug, Default)]
pub(crate) struct NodeDialog {
    /// The open dialog's control with the keyboard focus.
    focus: Option<DialogFocus>,
    /// The parameter being dragged, applied live and not yet recorded.
    live: Option<Live>,
    /// The node whose deletion is waiting to be confirmed, and the confirmation's focused button.
    confirming: Option<(NodeId, ConfirmFocus)>,
}

impl NodeDialog {
    /// Handles `message` against the project as `cx` shows it, returning what `App` must do.
    pub(crate) fn update(&mut self, message: DialogMessage, cx: &Context<'_>) -> Vec<Command> {
        if message.is_key()
            && let Some((node, focus)) = self.confirming
        {
            return self.confirm_key(&message, node, focus);
        }
        match message {
            DialogMessage::Slide { node, name, value } => {
                if let Some(refusal) = self.refusal(cx, node, &name) {
                    return vec![refusal];
                }
                self.live = Some(Live {
                    node,
                    name: name.clone(),
                    value,
                });
                vec![Command::SetLive { node, name, value }]
            }
            DialogMessage::Release => self
                .live
                .take()
                .map(|live| set_parameter(live.node, live.name, live.value))
                .into_iter()
                .collect(),
            DialogMessage::SetParameter { node, name, value } => {
                self.set_parameter(cx, node, name, value)
            }
            DialogMessage::ApplyPreset { node, preset } => self.apply_preset(cx, node, preset),
            DialogMessage::ToggleMode {
                node,
                channel,
                mode,
            } => self.toggle_mode(cx, node, channel, mode),
            DialogMessage::FocusNext | DialogMessage::FocusPrevious => {
                let forward = matches!(message, DialogMessage::FocusNext);
                match effect_panel::moved(self.focus, &self.tab_order(cx), forward) {
                    Some(next) => self.set_focus(Some(next)),
                    None => Vec::new(),
                }
            }
            DialogMessage::SetFocus(focus) => {
                let open = match focus {
                    None => true,
                    Some(DialogFocus::Mixer(_)) => mixer(cx).is_some(),
                    Some(DialogFocus::Parameter(_)) => self.effect(cx).is_some(),
                    Some(DialogFocus::Delete) => open_node(cx).is_some(),
                };
                if open {
                    self.set_focus(focus)
                } else {
                    Vec::new()
                }
            }
            DialogMessage::FocusSlide(delta) => {
                let slider =
                    self.effect(cx)
                        .zip(self.effect_focus())
                        .is_some_and(|(parameters, index)| {
                            parameters
                                .infos()
                                .get(index)
                                .is_some_and(|i| effect_panel::is_slider(&i.scale))
                        });
                if slider {
                    self.adjust_parameter_focus(cx, delta)
                } else {
                    Vec::new()
                }
            }
            DialogMessage::FocusAdjust(delta) => {
                if mixer(cx).is_some() {
                    self.adjust_mixer_focus(cx, delta)
                } else {
                    self.adjust_parameter_focus(cx, delta)
                }
            }
            DialogMessage::FocusPress => {
                if self.focus == Some(DialogFocus::Delete) {
                    if let Some(node) = open_node(cx) {
                        self.ask_delete(node);
                    }
                    return Vec::new();
                }
                if self.effect(cx).is_some() {
                    return self.press_parameter_focus(cx);
                }
                let Some((node, _)) = mixer(cx) else {
                    return Vec::new();
                };
                let (channel, mode) = match self.mixer_focus() {
                    Some(MixerFocus::Mute(ch)) => (ch, Mode::Mute),
                    Some(MixerFocus::Solo(ch)) => (ch, Mode::Solo),
                    _ => return Vec::new(),
                };
                self.toggle_mode(cx, node, channel, mode)
            }
        }
    }

    /// The open dialog's control with the keyboard focus, if any.
    pub(crate) fn focus(&self) -> Option<DialogFocus> {
        self.focus
    }

    /// The mixer dialog control with the keyboard focus, if any.
    pub(crate) fn mixer_focus(&self) -> Option<MixerFocus> {
        match self.focus {
            Some(DialogFocus::Mixer(focus)) => Some(focus),
            _ => None,
        }
    }

    /// The effect dialog control with the keyboard focus, if any, by index into its kind's
    /// parameters.
    pub(crate) fn effect_focus(&self) -> Option<usize> {
        match self.focus {
            Some(DialogFocus::Parameter(index)) => Some(index),
            _ => None,
        }
    }

    /// The parameter being dragged, if any.
    pub(crate) fn live(&self) -> Option<&Live> {
        self.live.as_ref()
    }

    /// The node whose deletion is waiting to be confirmed, if any.
    pub(crate) fn confirming(&self) -> Option<NodeId> {
        self.confirming.map(|(node, _)| node)
    }

    /// The delete confirmation's button with the keyboard focus, if it is open.
    pub(crate) fn confirm_focus(&self) -> Option<ConfirmFocus> {
        self.confirming.map(|(_, focus)| focus)
    }

    /// Opens the confirmation to delete node `node`, with "Cancel" focused.
    pub(crate) fn ask_delete(&mut self, node: NodeId) {
        self.confirming = Some((node, ConfirmFocus::Cancel));
    }

    /// Closes the delete confirmation, if open.
    pub(crate) fn cancel_delete(&mut self) {
        self.confirming = None;
    }

    /// Takes the keyboard focus away and closes any delete confirmation, without recording a
    /// field: another node, or none, was selected.
    pub(crate) fn close(&mut self) {
        self.focus = None;
        self.confirming = None;
    }

    /// Gives the keyboard focus to the open mixer's "Channels" field: it is being typed in.
    pub(crate) fn focus_channels(&mut self) {
        self.focus = Some(DialogFocus::Mixer(MixerFocus::Channels));
    }

    /// Key `message` with the confirmation to delete `node` open and `focus` on one of its
    /// buttons: Tab and Shift-Tab switch buttons, Space or Enter presses the focused one, and the
    /// arrows do nothing.
    fn confirm_key(
        &mut self,
        message: &DialogMessage,
        node: NodeId,
        focus: ConfirmFocus,
    ) -> Vec<Command> {
        match message {
            DialogMessage::FocusNext | DialogMessage::FocusPrevious => {
                let other = match focus {
                    ConfirmFocus::Cancel => ConfirmFocus::Delete,
                    ConfirmFocus::Delete => ConfirmFocus::Cancel,
                };
                self.confirming = Some((node, other));
                Vec::new()
            }
            DialogMessage::FocusPress => {
                self.confirming = None;
                match focus {
                    ConfirmFocus::Cancel => Vec::new(),
                    ConfirmFocus::Delete => vec![Command::DeleteNode(node)],
                }
            }
            _ => Vec::new(),
        }
    }

    /// The open dialog's controls in Tab order: a mixer's or effect's controls, then "Delete";
    /// empty with no dialog open.
    fn tab_order(&self, cx: &Context<'_>) -> Vec<DialogFocus> {
        if open_node(cx).is_none() {
            return Vec::new();
        }
        let mut order: Vec<DialogFocus> = if let Some((_, channels)) = mixer(cx) {
            MixerFocus::order(channels)
                .map(DialogFocus::Mixer)
                .collect()
        } else if let Some(parameters) = self.effect(cx) {
            let order = tab_order(parameters.infos());
            order.into_iter().map(DialogFocus::Parameter).collect()
        } else {
            Vec::new()
        };
        order.push(DialogFocus::Delete);
        order
    }

    /// Node `node`'s parameters as its dialog shows them, with the slider being dragged.
    fn parameters<'a>(&'a self, cx: &Context<'a>, node: NodeId) -> Option<NodeParameters<'a>> {
        cx.catalog
            .view(cx.state, node, self.live.as_ref(), cx.playing)
    }

    /// The open node's parameters, if it is a built-in effect this build knows.
    fn effect<'a>(&'a self, cx: &Context<'a>) -> Option<NodeParameters<'a>> {
        self.parameters(cx, cx.open?).filter(|p| p.kind().is_some())
    }

    /// The status-line refusal of a change to node `node`'s parameter `name`, if it is
    /// stopped-only and playback is running (ADR-0088).
    fn refusal(&self, cx: &Context<'_>, node: NodeId, name: &str) -> Option<Command> {
        self.parameters(cx, node)
            .is_some_and(|p| p.refuses(name))
            .then(|| Command::Status(format!("Pause or stop playback to change the {name}.")))
    }

    /// Sets node `node`'s parameter `name` to `value` as one undo step, unless refused.
    fn set_parameter(
        &self,
        cx: &Context<'_>,
        node: NodeId,
        name: String,
        value: f32,
    ) -> Vec<Command> {
        let command = self
            .refusal(cx, node, &name)
            .unwrap_or_else(|| set_parameter(node, name, value));
        vec![command]
    }

    /// Moves the keyboard focus to `focus`; leaving a mixer's "Channels" field records what was
    /// typed in it.
    fn set_focus(&mut self, focus: Option<DialogFocus>) -> Vec<Command> {
        let channels = Some(DialogFocus::Mixer(MixerFocus::Channels));
        let leaving = self.focus == channels && focus != self.focus;
        self.focus = focus;
        if leaving {
            vec![Command::CommitNodeText]
        } else {
            Vec::new()
        }
    }

    /// Steps the focused effect control by `delta` presses, as one undo step
    /// ([`NodeParameters::stepped`]).
    fn adjust_parameter_focus(&self, cx: &Context<'_>, delta: i32) -> Vec<Command> {
        let Some(index) = self.effect_focus() else {
            return Vec::new();
        };
        let edit = self
            .effect(cx)
            .and_then(|p| Some((p.node(), p.stepped(index, delta)?)));
        match edit {
            Some((node, (name, value))) => self.set_parameter(cx, node, name, value),
            None => Vec::new(),
        }
    }

    /// Flips the focused effect control if it is a toggle, as one undo step.
    fn press_parameter_focus(&self, cx: &Context<'_>) -> Vec<Command> {
        let Some(index) = self.effect_focus() else {
            return Vec::new();
        };
        let edit = self
            .effect(cx)
            .and_then(|p| Some((p.node(), p.pressed(index)?)));
        match edit {
            Some((node, (name, value))) => self.set_parameter(cx, node, name, value),
            None => Vec::new(),
        }
    }

    /// Adjusts the focused mixer control by `delta` steps, each recorded as one undo step: the
    /// channel count by one, gain by [`GAIN_STEP_DB`], pan by [`PAN_STEP`] (up is left).
    fn adjust_mixer_focus(&self, cx: &Context<'_>, delta: i32) -> Vec<Command> {
        let (Some((node, _)), Some(focus)) = (mixer(cx), self.mixer_focus()) else {
            return Vec::new();
        };
        let value_of = |name: &str| self.parameters(cx, node).map_or(0.0, |p| p.value(name));
        let (name, value) = match focus {
            MixerFocus::Channels => return vec![Command::StepChannels(node, delta)],
            MixerFocus::Gain(ch) => {
                let name = parameter_name(ch, Control::Gain);
                let gain = value_of(&name);
                let value = ((gain / GAIN_STEP_DB).round() + delta as f32) * GAIN_STEP_DB;
                (name, value.clamp(SILENCE_DB, MAX_GAIN_DB))
            }
            MixerFocus::Pan(ch) => {
                let name = parameter_name(ch, Control::Pan);
                let pan = value_of(&name);
                let value = ((pan / PAN_STEP).round() - delta as f32) * PAN_STEP;
                (name, value.clamp(-1.0, 1.0))
            }
            MixerFocus::Mute(_) | MixerFocus::Solo(_) => return Vec::new(),
        };
        self.set_parameter(cx, node, name, value)
    }

    /// Sets mixer `node`'s channel `channel` to `mode`, or back to playing if it is already, as
    /// one undo step; soloing it unsolos the mixer's other channels in the same step.
    fn toggle_mode(
        &self,
        cx: &Context<'_>,
        node: NodeId,
        channel: usize,
        mode: Mode,
    ) -> Vec<Command> {
        let Some(channels) = channel_count(cx.state, node) else {
            return Vec::new();
        };
        let parameters = self.parameters(cx, node);
        let mode_of = |ch: usize| {
            let name = parameter_name(ch, Control::Mode);
            Mode::from_value(parameters.map_or(0.0, |p| p.value(&name)))
        };
        let set = |ch: usize, mode: Mode| Edit::SetParameter {
            node,
            name: parameter_name(ch, Control::Mode),
            value: Some(mode.value()),
        };
        let wanted = toggled(mode_of(channel), mode);
        let mut edits = vec![set(channel, wanted)];
        if wanted == Mode::Solo {
            edits.extend(
                (0..channels)
                    .filter(|&ch| ch != channel && mode_of(ch) == Mode::Solo)
                    .map(|ch| set(ch, Mode::Play)),
            );
        }
        let edit = match edits.len() {
            1 => edits.remove(0),
            _ => Edit::Batch(edits),
        };
        vec![Command::Edit(edit)]
    }

    /// Sets every parameter of effect node `node` to preset `name`'s value or its default, as
    /// one undo step. Refused while playing if it would change a stopped-only value; ignored if
    /// the preset is for another kind.
    fn apply_preset(&self, cx: &Context<'_>, node: NodeId, name: &str) -> Vec<Command> {
        let Some(preset) = audio_effects::preset(name) else {
            return Vec::new();
        };
        let Some(edits) = self.parameters(cx, node).map(|p| p.preset_edits(preset)) else {
            return Vec::new();
        };
        match edits {
            Err(refusal) => vec![Command::Status(refusal)],
            Ok(edits) if edits.is_empty() => Vec::new(),
            Ok(edits) => vec![Command::Edit(Edit::Batch(edits))],
        }
    }
}

/// The edit that sets node `node`'s parameter `name` to `value`.
fn set_parameter(node: NodeId, name: String, value: f32) -> Command {
    Command::Edit(Edit::SetParameter {
        node,
        name,
        value: Some(value),
    })
}

/// The open node, if it exists.
fn open_node(cx: &Context<'_>) -> Option<NodeId> {
    cx.open.filter(|&node| cx.state.node(node).is_some())
}

/// The open node and its channel count, if it is a mixer.
fn mixer(cx: &Context<'_>) -> Option<(NodeId, usize)> {
    let node = cx.open?;
    channel_count(cx.state, node).map(|channels| (node, channels))
}

/// Mixer `node`'s channel count; `None` if it is not a mixer.
fn channel_count(state: &State, node: NodeId) -> Option<usize> {
    match state.node(node)?.kind {
        NodeKind::Mixer { channels, .. } => Some(channels),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use project_model::Session;

    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    /// A session, the catalog, and the dialog of one node, with whether playback is running.
    struct Fixture {
        session: Session,
        catalog: ParameterCatalog,
        dialog: NodeDialog,
        node: NodeId,
        playing: bool,
    }

    impl Fixture {
        /// A dialog open on a new effect of `kind`.
        fn effect(kind: &str) -> Result<Fixture, Box<dyn std::error::Error>> {
            Fixture::new(NodeKind::Effect {
                kind: kind.to_owned(),
                parameters: BTreeMap::new(),
            })
        }

        /// A dialog open on a new mixer of `channels` channels.
        fn mixer(channels: usize) -> Result<Fixture, Box<dyn std::error::Error>> {
            Fixture::new(NodeKind::Mixer {
                channels,
                parameters: BTreeMap::new(),
            })
        }

        fn new(kind: NodeKind) -> Result<Fixture, Box<dyn std::error::Error>> {
            let mut session = Session::new();
            let node = session.add_node(kind)?;
            Ok(Fixture {
                session,
                catalog: ParameterCatalog::new(),
                dialog: NodeDialog::default(),
                node,
                playing: false,
            })
        }

        /// Sends `message`, returning the commands it gave.
        fn send(&mut self, message: DialogMessage) -> Vec<Command> {
            let cx = Context {
                catalog: &self.catalog,
                state: self.session.state(),
                playing: self.playing,
                open: Some(self.node),
            };
            self.dialog.update(message, &cx)
        }

        /// Sends `message` and applies the edits it gave to the session, returning the commands.
        fn apply(&mut self, message: DialogMessage) -> Result<Vec<Command>, project_model::Error> {
            let commands = self.send(message);
            for command in &commands {
                if let Command::Edit(edit) = command {
                    self.session.apply(edit.clone())?;
                }
            }
            Ok(commands)
        }

        /// Parameter `name`'s value as shown.
        fn value(&self, name: &str) -> f32 {
            self.catalog
                .view(
                    self.session.state(),
                    self.node,
                    self.dialog.live(),
                    self.playing,
                )
                .map_or(f32::NAN, |p| p.value(name))
        }

        /// The focus on parameter `name`.
        fn on(&self, name: &str) -> Result<DialogFocus, Box<dyn std::error::Error>> {
            let view = self
                .catalog
                .view(self.session.state(), self.node, None, false)
                .ok_or("no parameters")?;
            let index = view
                .infos()
                .iter()
                .position(|i| i.name == name)
                .ok_or_else(|| format!("no parameter {name}"))?;
            Ok(DialogFocus::Parameter(index))
        }
    }

    fn edit(node: NodeId, name: &str, value: f32) -> Command {
        set_parameter(node, name.to_owned(), value)
    }

    #[test]
    fn test_update_tab_visits_effect_controls_in_order_and_wraps() -> TestResult {
        let mut f = Fixture::effect("parametric-eq")?;
        // All grouped: the high-pass stage, each band, then the low-pass stage, as declared.
        assert_eq!(f.send(DialogMessage::FocusNext), []);
        assert_eq!(f.dialog.focus(), Some(f.on("high-pass enabled")?));
        assert_eq!(f.dialog.mixer_focus(), None);
        for _ in 0..6 {
            f.send(DialogMessage::FocusNext);
        }
        assert_eq!(f.dialog.focus(), Some(f.on("band 1 gain")?));
        f.send(DialogMessage::FocusPrevious);
        assert_eq!(f.dialog.focus(), Some(f.on("band 1 frequency")?));
        // "Delete" comes last; Shift-Tab from the first control wraps to it.
        f.send(DialogMessage::SetFocus(Some(f.on("high-pass enabled")?)));
        f.send(DialogMessage::FocusPrevious);
        assert_eq!(f.dialog.focus(), Some(DialogFocus::Delete));
        f.send(DialogMessage::FocusPrevious);
        assert_eq!(f.dialog.focus(), Some(f.on("low-pass order")?));
        f.send(DialogMessage::FocusNext);
        f.send(DialogMessage::FocusNext);
        assert_eq!(f.dialog.focus(), Some(f.on("high-pass enabled")?));
        Ok(())
    }

    #[test]
    fn test_update_nothing_focuses_without_an_open_dialog() -> TestResult {
        let mut f = Fixture::effect("parametric-eq")?;
        let cx = Context {
            catalog: &f.catalog,
            state: f.session.state(),
            playing: false,
            open: None,
        };
        assert_eq!(f.dialog.update(DialogMessage::FocusNext, &cx), []);
        assert_eq!(f.dialog.focus(), None);
        // A mixer's focus is not taken while an effect's dialog is open.
        f.send(DialogMessage::SetFocus(Some(DialogFocus::Mixer(
            MixerFocus::Channels,
        ))));
        assert_eq!(f.dialog.focus(), None);
        Ok(())
    }

    #[test]
    fn test_update_arrows_step_sliders() -> TestResult {
        let mut f = Fixture::effect("parametric-eq")?;
        let node = f.node;
        // A linear slider moves 1 % of its range a press, each an undo step.
        f.send(DialogMessage::SetFocus(Some(f.on("band 1 gain")?)));
        assert_eq!(
            f.apply(DialogMessage::FocusAdjust(1))?,
            [edit(node, "band 1 gain", 0.48)]
        );
        f.apply(DialogMessage::FocusAdjust(1))?;
        assert!((f.value("band 1 gain") - 0.96).abs() < 1e-5);
        // Right and left step a slider too.
        f.apply(DialogMessage::FocusSlide(-1))?;
        assert!((f.value("band 1 gain") - 0.48).abs() < 1e-5);
        // A logarithmic slider moves 1 % of its logarithm's range.
        f.send(DialogMessage::SetFocus(Some(f.on("band 1 frequency")?)));
        f.apply(DialogMessage::FocusAdjust(-1))?;
        let step = (20_000.0_f32.ln() - 20.0_f32.ln()) / 100.0;
        let expected = (100.0_f32.ln() - step).exp();
        assert!((f.value("band 1 frequency") - expected).abs() < 1e-2);
        Ok(())
    }

    #[test]
    fn test_update_arrows_step_choices_and_integers() -> TestResult {
        let mut f = Fixture::effect("parametric-eq")?;
        // Band 1's type: a choice of four, one option a press and held at the ends.
        f.send(DialogMessage::SetFocus(Some(f.on("band 1 type")?)));
        f.apply(DialogMessage::FocusAdjust(-1))?;
        assert_eq!(f.value("band 1 type"), 0.0);
        for _ in 0..5 {
            f.apply(DialogMessage::FocusAdjust(1))?;
        }
        assert_eq!(f.value("band 1 type"), 3.0);
        // Right and left leave a choice alone.
        assert_eq!(f.send(DialogMessage::FocusSlide(-1)), []);
        // The high-pass order: an integer, from 2 by one a press, held at 1.
        f.send(DialogMessage::SetFocus(Some(f.on("high-pass order")?)));
        f.apply(DialogMessage::FocusAdjust(1))?;
        assert_eq!(f.value("high-pass order"), 3.0);
        for _ in 0..4 {
            f.apply(DialogMessage::FocusAdjust(-1))?;
        }
        assert_eq!(f.value("high-pass order"), 1.0);
        // An integer is a slider: right steps it.
        f.apply(DialogMessage::FocusSlide(1))?;
        assert_eq!(f.value("high-pass order"), 2.0);
        Ok(())
    }

    #[test]
    fn test_update_press_flips_a_toggle_and_leaves_a_slider() -> TestResult {
        let mut f = Fixture::effect("parametric-eq")?;
        let node = f.node;
        f.send(DialogMessage::SetFocus(Some(f.on("high-pass enabled")?)));
        assert_eq!(
            f.apply(DialogMessage::FocusPress)?,
            [edit(node, "high-pass enabled", 1.0)]
        );
        assert_eq!(f.value("high-pass enabled"), 1.0);
        f.send(DialogMessage::SetFocus(Some(f.on("band 1 gain")?)));
        assert_eq!(f.send(DialogMessage::FocusPress), []);
        Ok(())
    }

    #[test]
    fn test_update_slide_is_live_and_release_records_one_edit() -> TestResult {
        let mut f = Fixture::effect("parametric-eq")?;
        let node = f.node;
        for value in [1.0, 4.5] {
            let commands = f.send(DialogMessage::Slide {
                node,
                name: "band 2 gain".to_owned(),
                value,
            });
            let live = Command::SetLive {
                node,
                name: "band 2 gain".to_owned(),
                value,
            };
            assert_eq!(commands, [live]);
        }
        assert_eq!(f.value("band 2 gain"), 4.5);
        assert_eq!(
            f.send(DialogMessage::Release),
            [edit(node, "band 2 gain", 4.5)]
        );
        assert_eq!(f.dialog.live(), None);
        assert_eq!(f.send(DialogMessage::Release), []);
        Ok(())
    }

    #[test]
    fn test_update_stopped_only_changes_refused_while_playing() -> TestResult {
        let mut f = Fixture::effect("high-pass")?;
        let node = f.node;
        f.playing = true;
        let refused = [Command::Status(
            "Pause or stop playback to change the order.".to_owned(),
        )];
        let slide = DialogMessage::Slide {
            node,
            name: "order".to_owned(),
            value: 4.0,
        };
        assert_eq!(f.send(slide), refused);
        assert_eq!(f.dialog.live(), None);
        let set = DialogMessage::SetParameter {
            node,
            name: "order".to_owned(),
            value: 4.0,
        };
        assert_eq!(f.send(set.clone()), refused);
        // The cutoff may move while playing.
        let cutoff = DialogMessage::SetParameter {
            node,
            name: "cutoff".to_owned(),
            value: 120.0,
        };
        assert_eq!(f.send(cutoff), [edit(node, "cutoff", 120.0)]);
        f.playing = false;
        assert_eq!(f.send(set), [edit(node, "order", 4.0)]);
        Ok(())
    }

    #[test]
    fn test_update_preset_is_one_batch_and_another_kinds_is_ignored() -> TestResult {
        let mut f = Fixture::effect("low-pass")?;
        let node = f.node;
        let compressor = DialogMessage::ApplyPreset {
            node,
            preset: "Compressor",
        };
        assert_eq!(f.send(compressor.clone()), []);
        let mut f = Fixture::effect("dynamics")?;
        let node = f.node;
        let limiter = DialogMessage::ApplyPreset {
            node,
            preset: "Limiter",
        };
        assert!(matches!(
            &f.send(limiter.clone())[..],
            [Command::Edit(Edit::Batch(_))]
        ));
        // While playing, a preset that would change a stopped-only value is refused.
        f.playing = true;
        assert!(matches!(&f.send(limiter)[..], [Command::Status(_)]));
        Ok(())
    }

    #[test]
    fn test_update_tab_runs_down_a_strip_then_across() -> TestResult {
        let mut f = Fixture::mixer(2)?;
        f.send(DialogMessage::FocusNext);
        assert_eq!(f.dialog.mixer_focus(), Some(MixerFocus::Channels));
        assert_eq!(f.dialog.effect_focus(), None);
        // Up on the field asks for another channel.
        assert_eq!(
            f.send(DialogMessage::FocusAdjust(1)),
            [Command::StepChannels(f.node, 1)]
        );
        // Leaving the field records what was typed.
        assert_eq!(f.send(DialogMessage::FocusNext), [Command::CommitNodeText]);
        let order = [
            MixerFocus::Mute(0),
            MixerFocus::Solo(0),
            MixerFocus::Gain(0),
            MixerFocus::Pan(0),
            MixerFocus::Mute(1),
        ];
        assert_eq!(f.dialog.mixer_focus(), Some(order[0]));
        for focus in &order[1..] {
            assert_eq!(f.send(DialogMessage::FocusNext), []);
            assert_eq!(f.dialog.mixer_focus(), Some(*focus));
        }
        f.send(DialogMessage::FocusPrevious);
        assert_eq!(f.dialog.mixer_focus(), Some(MixerFocus::Pan(0)));
        // Past the last channel's pan comes "Delete", then the field again.
        f.send(DialogMessage::SetFocus(Some(DialogFocus::Mixer(
            MixerFocus::Pan(1),
        ))));
        f.send(DialogMessage::FocusNext);
        assert_eq!(f.dialog.focus(), Some(DialogFocus::Delete));
        f.send(DialogMessage::FocusNext);
        assert_eq!(f.dialog.mixer_focus(), Some(MixerFocus::Channels));
        // Clicking away from the field records it too; elsewhere it does not.
        f.dialog.focus_channels();
        assert_eq!(
            f.send(DialogMessage::SetFocus(None)),
            [Command::CommitNodeText]
        );
        assert_eq!(f.send(DialogMessage::SetFocus(None)), []);
        Ok(())
    }

    #[test]
    fn test_update_arrows_step_mixer_gain_and_pan() -> TestResult {
        let mut f = Fixture::mixer(1)?;
        let node = f.node;
        f.send(DialogMessage::SetFocus(Some(DialogFocus::Mixer(
            MixerFocus::Gain(0),
        ))));
        f.apply(DialogMessage::FocusAdjust(1))?;
        f.apply(DialogMessage::FocusAdjust(1))?;
        assert_eq!(f.value("channel 1 gain"), 2.0);
        // Up on pan moves it left.
        f.send(DialogMessage::SetFocus(Some(DialogFocus::Mixer(
            MixerFocus::Pan(0),
        ))));
        assert_eq!(
            f.send(DialogMessage::FocusAdjust(1)),
            [edit(node, "channel 1 pan", -0.01)]
        );
        // Arrows leave mute alone.
        f.send(DialogMessage::SetFocus(Some(DialogFocus::Mixer(
            MixerFocus::Mute(0),
        ))));
        assert_eq!(f.send(DialogMessage::FocusAdjust(1)), []);
        Ok(())
    }

    #[test]
    fn test_update_solo_unsolos_the_others_in_one_batch() -> TestResult {
        let mut f = Fixture::mixer(3)?;
        let node = f.node;
        let toggle = |channel, mode| DialogMessage::ToggleMode {
            node,
            channel,
            mode,
        };
        f.apply(toggle(0, Mode::Solo))?;
        f.apply(toggle(2, Mode::Mute))?;
        let mode = |ch: usize| Edit::SetParameter {
            node,
            name: parameter_name(ch, Control::Mode),
            value: Some(if ch == 1 {
                Mode::Solo.value()
            } else {
                Mode::Play.value()
            }),
        };
        assert_eq!(
            f.apply(toggle(1, Mode::Solo))?,
            [Command::Edit(Edit::Batch(vec![mode(1), mode(0)]))]
        );
        // A focused mute button toggles on a press; the mute is left alone by the solo.
        assert_eq!(Mode::from_value(f.value("channel 3 mode")), Mode::Mute);
        f.send(DialogMessage::SetFocus(Some(DialogFocus::Mixer(
            MixerFocus::Mute(2),
        ))));
        f.apply(DialogMessage::FocusPress)?;
        assert_eq!(Mode::from_value(f.value("channel 3 mode")), Mode::Play);
        Ok(())
    }

    #[test]
    fn test_update_tab_reaches_delete_on_a_dialog_with_no_controls() -> TestResult {
        let mut f = Fixture::new(NodeKind::Bus(audio_buffers::Layout::Stereo))?;
        f.send(DialogMessage::FocusNext);
        assert_eq!(f.dialog.focus(), Some(DialogFocus::Delete));
        f.send(DialogMessage::FocusPrevious);
        assert_eq!(f.dialog.focus(), Some(DialogFocus::Delete));
        Ok(())
    }

    #[test]
    fn test_update_press_on_delete_asks_and_the_confirmation_takes_the_keys() -> TestResult {
        let mut f = Fixture::mixer(1)?;
        let node = f.node;
        f.send(DialogMessage::SetFocus(Some(DialogFocus::Delete)));
        assert_eq!(f.send(DialogMessage::FocusPress), []);
        assert_eq!(f.dialog.confirming(), Some(node));
        assert_eq!(f.dialog.confirm_focus(), Some(ConfirmFocus::Cancel));
        // Tab and Shift-Tab switch buttons; the arrows do nothing; the dialog's focus stays.
        f.send(DialogMessage::FocusNext);
        assert_eq!(f.dialog.confirm_focus(), Some(ConfirmFocus::Delete));
        assert_eq!(f.send(DialogMessage::FocusAdjust(1)), []);
        f.send(DialogMessage::FocusPrevious);
        assert_eq!(f.dialog.confirm_focus(), Some(ConfirmFocus::Cancel));
        assert_eq!(f.dialog.focus(), Some(DialogFocus::Delete));
        // Space or Enter on Cancel closes it.
        assert_eq!(f.send(DialogMessage::FocusPress), []);
        assert_eq!(f.dialog.confirming(), None);
        // On Delete, it asks App to delete the node.
        f.dialog.ask_delete(node);
        f.send(DialogMessage::FocusPrevious);
        assert_eq!(
            f.send(DialogMessage::FocusPress),
            [Command::DeleteNode(node)]
        );
        assert_eq!(f.dialog.confirming(), None);
        Ok(())
    }
}
