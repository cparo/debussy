//! A mixer's or effect's parameters as its dialog shows them right now: the one view the panel,
//! display, preset picker, preset choice, keyboard stepping and the stopped-only guard all read
//! (`docs/architecture/user-interface.md` "Gestures").

use std::collections::BTreeMap;

use audio_effects::Preset;
use project_model::{Edit, NodeId, NodeKind, State};
use signal_graph::{ParameterInfo, Scale};

/// The share of a slider's range (of its logarithm, if logarithmic) an arrow key moves it.
const ARROW_STEP: f32 = 0.01;

/// A parameter being dragged: applied to the graph, not yet recorded.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Live {
    pub node: NodeId,
    pub name: String,
    pub value: f32,
}

/// Every mixer's and built-in effect's parameters, read once at launch.
#[derive(Debug)]
pub(crate) struct ParameterCatalog {
    /// Each built-in effect kind's parameters.
    effects: BTreeMap<&'static str, Vec<ParameterInfo>>,
    /// The parameters of a mixer of the most channels; a mixer of fewer has the first three per
    /// channel.
    mixer: Vec<ParameterInfo>,
}

impl ParameterCatalog {
    /// The parameters of every built-in effect kind and of a mixer of the most channels.
    pub(crate) fn new() -> ParameterCatalog {
        ParameterCatalog {
            effects: audio_effects::EFFECTS
                .iter()
                .filter_map(|e| Some((e.id, audio_effects::parameter_infos(e.id)?)))
                .collect(),
            mixer: audio_effects::mixer_parameter_infos(audio_effects::MAX_CHANNELS),
        }
    }

    /// Node `node` of `state`'s parameters as its dialog shows them, with the parameter being
    /// dragged anywhere `live` and whether playback is running; `None` unless it is a mixer or a
    /// built-in effect this build knows.
    pub(crate) fn view<'a>(
        &'a self,
        state: &'a State,
        node: NodeId,
        live: Option<&'a Live>,
        playing: bool,
    ) -> Option<NodeParameters<'a>> {
        let (kind, infos, stored) = match &state.node(node)?.kind {
            NodeKind::Effect { kind, parameters } => (
                Some(kind.as_str()),
                self.effects.get(kind.as_str())?.as_slice(),
                parameters,
            ),
            NodeKind::Mixer {
                channels,
                parameters,
            } => (
                None,
                self.mixer.get(..channels.saturating_mul(3))?,
                parameters,
            ),
            _ => return None,
        };
        Some(NodeParameters::new(
            node, kind, infos, stored, live, playing,
        ))
    }
}

/// A mixer's or effect's parameters as its dialog shows them right now.
#[derive(Debug, Clone, Copy)]
pub(crate) struct NodeParameters<'a> {
    node: NodeId,
    /// The effect kind; `None` for a mixer.
    kind: Option<&'a str>,
    /// The node's parameters, from static metadata, in the order its node declares them.
    infos: &'a [ParameterInfo],
    /// The model's values by name.
    stored: &'a BTreeMap<String, f32>,
    /// The parameter being dragged, if it is one of this node's.
    live: Option<&'a Live>,
    /// Whether playback is running.
    playing: bool,
}

impl<'a> NodeParameters<'a> {
    /// The view of node `node`, of effect kind `kind` (`None` for a mixer), with parameters
    /// `infos`, model values `stored`, the parameter being dragged anywhere `live`, and whether
    /// playback is running.
    pub(crate) fn new(
        node: NodeId,
        kind: Option<&'a str>,
        infos: &'a [ParameterInfo],
        stored: &'a BTreeMap<String, f32>,
        live: Option<&'a Live>,
        playing: bool,
    ) -> NodeParameters<'a> {
        NodeParameters {
            node,
            kind,
            infos,
            stored,
            live: live.filter(|l| l.node == node),
            playing,
        }
    }

    /// The node.
    pub(crate) fn node(&self) -> NodeId {
        self.node
    }

    /// The effect kind; `None` for a mixer.
    pub(crate) fn kind(&self) -> Option<&'a str> {
        self.kind
    }

    /// The parameters, in the order the node declares them.
    pub(crate) fn infos(&self) -> &'a [ParameterInfo] {
        self.infos
    }

    /// Parameter `name`'s metadata, or `None` if the node has no such parameter.
    pub(crate) fn info(&self, name: &str) -> Option<&'a ParameterInfo> {
        self.infos.iter().find(|i| i.name == name)
    }

    /// Parameter `name`'s value as shown: the one being dragged, else the model's, else its
    /// declared default, else 0.
    pub(crate) fn value(&self, name: &str) -> f32 {
        if let Some(live) = self.live.filter(|l| l.name == name) {
            return live.value;
        }
        self.stored
            .get(name)
            .copied()
            .or_else(|| self.info(name).map(|i| i.default))
            .unwrap_or(0.0)
    }

    /// Every parameter's value as shown, in [`NodeParameters::infos`] order.
    pub(crate) fn values(&self) -> Vec<f32> {
        self.infos.iter().map(|i| self.value(&i.name)).collect()
    }

    /// Whether parameter `info` is shown but not offered: stopped-only while playing (ADR-0088).
    pub(crate) fn is_frozen(&self, info: &ParameterInfo) -> bool {
        info.stopped_only && self.playing
    }

    /// Whether a change to parameter `name` is refused now: it is stopped-only and playback is
    /// running (ADR-0088).
    pub(crate) fn refuses(&self, name: &str) -> bool {
        self.info(name).is_some_and(|i| self.is_frozen(i))
    }

    /// The preset of this effect's kind its values match; `None` if none does, or for a mixer.
    pub(crate) fn matching_preset(&self) -> Option<&'static Preset> {
        audio_effects::matching_preset(self.kind?, self.infos, |name| self.value(name))
    }

    /// The edits that set every parameter to `preset`'s value, or back to its default where the
    /// preset names none; empty if the preset is for another kind. `Err` holds the status text
    /// if playback is running and the preset would change a stopped-only value.
    pub(crate) fn preset_edits(&self, preset: &Preset) -> Result<Vec<Edit>, String> {
        if self.kind != Some(preset.kind) {
            return Ok(Vec::new());
        }
        let blocked = self.infos.iter().any(|info| {
            self.is_frozen(info)
                && info.constrain(preset.value(info)) != Some(self.value(&info.name))
        });
        if blocked {
            return Err(format!(
                "Pause or stop playback to choose the {} preset.",
                preset.name
            ));
        }
        Ok(self
            .infos
            .iter()
            .map(|info| Edit::SetParameter {
                node: self.node,
                name: info.name.clone(),
                value: preset.get(&info.name),
            })
            .collect())
    }

    /// The parameter name and value `delta` arrow presses give parameter `infos[index]`
    /// ([`stepped`]); `None` past the end, or for a toggle.
    pub(crate) fn stepped(&self, index: usize, delta: i32) -> Option<(String, f32)> {
        let info = self.infos.get(index)?;
        let value = stepped(info, self.value(&info.name), delta)?;
        Some((info.name.clone(), value))
    }

    /// The value `delta` arrow presses give parameter `name` ([`stepped`]); `None` if the node
    /// has no such parameter, or for a toggle.
    pub(crate) fn stepped_named(&self, name: &str, delta: i32) -> Option<f32> {
        stepped(self.info(name)?, self.value(name), delta)
    }

    /// The parameter name and value a press gives parameter `infos[index]`: a toggle flipped;
    /// `None` past the end, or for any other scale.
    pub(crate) fn pressed(&self, index: usize) -> Option<(String, f32)> {
        let info = self.infos.get(index).filter(|i| i.scale == Scale::Toggle)?;
        let on = self.value(&info.name) >= 0.5;
        Some((info.name.clone(), if on { 0.0 } else { 1.0 }))
    }
}

/// The order an effect's controls are drawn and visited by Tab, as indices into `infos`: the
/// ungrouped parameters, then each group's, groups in the order they first appear.
pub(crate) fn tab_order(infos: &[ParameterInfo]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..infos.len())
        .filter(|&i| infos[i].group.is_empty())
        .collect();
    let mut groups: Vec<&str> = Vec::new();
    for info in infos.iter().filter(|i| !i.group.is_empty()) {
        if !groups.contains(&info.group.as_str()) {
            groups.push(&info.group);
        }
    }
    for group in groups {
        order.extend((0..infos.len()).filter(|&i| infos[i].group == group));
    }
    order
}

/// Parameter `info` at `value` moved `delta` arrow presses up (positive) or down, within its
/// range. A parameter that declares a [`step`](ParameterInfo::step) snaps to that grid and moves
/// by it; otherwise a linear slider moves by [`ARROW_STEP`] of its range, a logarithmic one by
/// that share of its logarithm's, an integer by one, a choice to the next or previous option.
/// `None` for a toggle, which arrows leave alone.
pub(crate) fn stepped(info: &ParameterInfo, value: f32, delta: i32) -> Option<f32> {
    #[allow(clippy::cast_precision_loss)] // a handful of key presses
    let delta = delta as f32;
    let (min, max) = (info.min, info.max);
    let value = match (&info.scale, info.step.filter(|&s| s > 0.0)) {
        (Scale::Toggle, _) => return None,
        (_, Some(step)) => ((value / step).round() + delta) * step,
        (Scale::Linear, None) => value + delta * ARROW_STEP * (max - min),
        (Scale::Logarithmic, None) => {
            let step = ARROW_STEP * (max.ln() - min.ln());
            (value.max(min).ln() + delta * step).exp()
        }
        (Scale::Integer, None) => value.round() + delta,
        (Scale::Choice(options), None) => {
            #[allow(clippy::cast_precision_loss)] // a handful of options
            let last = options.len().saturating_sub(1) as f32;
            (value.round() + delta).clamp(0.0, last)
        }
    };
    Some(value.clamp(min, max))
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    const NODE: NodeId = NodeId(7);

    fn infos() -> Vec<ParameterInfo> {
        vec![
            ParameterInfo::new("gain", -24.0, 24.0, 3.0),
            ParameterInfo::new("on", 0.0, 1.0, 0.0).with_scale(Scale::Toggle),
            ParameterInfo::new("order", 1.0, 8.0, 2.0)
                .with_scale(Scale::Integer)
                .stopped_only(),
        ]
    }

    fn stored(values: &[(&str, f32)]) -> BTreeMap<String, f32> {
        values.iter().map(|&(n, v)| (n.to_owned(), v)).collect()
    }

    fn live(node: NodeId, name: &str, value: f32) -> Live {
        Live {
            node,
            name: name.to_owned(),
            value,
        }
    }

    #[test]
    fn test_value_prefers_live_then_stored_then_default() {
        let infos = infos();
        let stored = stored(&[("gain", -6.0)]);
        let view = NodeParameters::new(NODE, None, &infos, &stored, None, false);
        assert_eq!(view.value("gain"), -6.0);
        assert_eq!(view.value("order"), 2.0);
        assert_eq!(view.value("missing"), 0.0);
        let dragged = live(NODE, "gain", 9.0);
        let view = NodeParameters::new(NODE, None, &infos, &stored, Some(&dragged), false);
        assert_eq!((view.value("gain"), view.value("order")), (9.0, 2.0));
        let elsewhere = live(NodeId(8), "gain", 9.0);
        let view = NodeParameters::new(NODE, None, &infos, &stored, Some(&elsewhere), false);
        assert_eq!(view.value("gain"), -6.0);
    }

    #[test]
    fn test_values_follows_infos_order() {
        let infos = infos();
        let stored = stored(&[("on", 1.0)]);
        let view = NodeParameters::new(NODE, None, &infos, &stored, None, false);
        assert_eq!(view.values(), [3.0, 1.0, 2.0]);
        assert_eq!(view.node(), NODE);
        assert_eq!(view.kind(), None);
        assert_eq!(view.infos().len(), 3);
    }

    #[test]
    fn test_refuses_only_stopped_only_while_playing() {
        let infos = infos();
        let stored = BTreeMap::new();
        let playing = NodeParameters::new(NODE, None, &infos, &stored, None, true);
        assert!(playing.refuses("order") && playing.is_frozen(&infos[2]));
        assert!(!playing.refuses("gain") && !playing.refuses("missing"));
        let stopped = NodeParameters::new(NODE, None, &infos, &stored, None, false);
        assert!(!stopped.refuses("order") && !stopped.is_frozen(&infos[2]));
    }

    fn dynamics() -> Result<(Vec<ParameterInfo>, &'static Preset, &'static Preset), String> {
        let infos = audio_effects::parameter_infos("dynamics").ok_or("no dynamics")?;
        let compressor = audio_effects::preset("Compressor").ok_or("no compressor")?;
        let limiter = audio_effects::preset("Limiter").ok_or("no limiter")?;
        Ok((infos, compressor, limiter))
    }

    #[test]
    fn test_preset_edits_set_every_parameter_and_reset_the_rest() -> TestResult {
        let (infos, _, limiter) = dynamics()?;
        let stored = stored(&[("knee", 1.5)]);
        let view = NodeParameters::new(NODE, Some("dynamics"), &infos, &stored, None, false);
        let edits = view.preset_edits(limiter)?;
        assert_eq!(edits.len(), infos.len());
        let value_of = |name: &str| {
            edits.iter().find_map(|e| match e {
                Edit::SetParameter { name: n, value, .. } if n == name => Some(*value),
                _ => None,
            })
        };
        assert_eq!(value_of("ceiling"), Some(Some(-1.0)));
        assert_eq!(value_of("knee"), Some(None));
        Ok(())
    }

    #[test]
    fn test_preset_edits_refused_while_playing_if_a_stopped_only_value_changes() -> TestResult {
        let (infos, compressor, limiter) = dynamics()?;
        let stored = BTreeMap::new();
        let view = NodeParameters::new(NODE, Some("dynamics"), &infos, &stored, None, true);
        // The limiter sets lookahead, which is stopped-only; the compressor keeps the default.
        assert_eq!(
            view.preset_edits(limiter),
            Err("Pause or stop playback to choose the Limiter preset.".to_owned())
        );
        assert_eq!(view.preset_edits(compressor)?.len(), infos.len());
        Ok(())
    }

    #[test]
    fn test_preset_edits_ignore_another_kinds_preset() -> TestResult {
        let (_, compressor, _) = dynamics()?;
        let infos = audio_effects::parameter_infos("low-pass").ok_or("no low-pass")?;
        let stored = BTreeMap::new();
        let view = NodeParameters::new(NODE, Some("low-pass"), &infos, &stored, None, false);
        assert_eq!(view.preset_edits(compressor)?, Vec::new());
        let mixer = NodeParameters::new(NODE, None, &infos, &stored, None, false);
        assert_eq!(mixer.preset_edits(compressor)?, Vec::new());
        Ok(())
    }

    #[test]
    fn test_matching_preset_follows_the_values() -> TestResult {
        let (infos, compressor, _) = dynamics()?;
        let empty = BTreeMap::new();
        let view = NodeParameters::new(NODE, Some("dynamics"), &infos, &empty, None, false);
        assert_eq!(view.matching_preset(), Some(compressor));
        let changed = stored(&[("knee", 1.5)]);
        let view = NodeParameters::new(NODE, Some("dynamics"), &infos, &changed, None, false);
        assert_eq!(view.matching_preset(), None);
        let mixer = NodeParameters::new(NODE, None, &infos, &empty, None, false);
        assert_eq!(mixer.matching_preset(), None);
        Ok(())
    }

    #[test]
    fn test_stepped_and_pressed_give_the_edit_for_a_key() {
        let infos = infos();
        let stored = stored(&[("on", 1.0)]);
        let view = NodeParameters::new(NODE, None, &infos, &stored, None, false);
        assert_eq!(view.stepped(0, 1), Some(("gain".to_owned(), 3.48)));
        assert_eq!(view.stepped(2, 1), Some(("order".to_owned(), 3.0)));
        assert_eq!(view.stepped(1, 1), None);
        assert_eq!(view.stepped(9, 1), None);
        assert_eq!(view.pressed(1), Some(("on".to_owned(), 0.0)));
        assert_eq!(view.pressed(0), None);
        assert_eq!(view.pressed(9), None);
        let empty = BTreeMap::new();
        let view = NodeParameters::new(NODE, None, &infos, &empty, None, false);
        assert_eq!(view.pressed(1), Some(("on".to_owned(), 1.0)));
    }

    #[test]
    fn test_tab_order_lists_ungrouped_then_groups() -> TestResult {
        let eq = audio_effects::parameter_infos("parametric-eq").ok_or("no parametric EQ")?;
        // All grouped, each group's parameters together: the order they are declared.
        assert_eq!(tab_order(&eq), (0..eq.len()).collect::<Vec<_>>());
        let infos = [
            ParameterInfo::new("a gain", 0.0, 1.0, 0.0).with_group("a"),
            ParameterInfo::new("mix", 0.0, 1.0, 0.0),
            ParameterInfo::new("b gain", 0.0, 1.0, 0.0).with_group("b"),
            ParameterInfo::new("a q", 0.0, 1.0, 0.0).with_group("a"),
            ParameterInfo::new("level", 0.0, 1.0, 0.0),
        ];
        assert_eq!(tab_order(&infos), [1, 4, 0, 3, 2]);
        Ok(())
    }

    #[test]
    fn test_stepped_moves_by_scale_and_clamps() {
        let linear = ParameterInfo::new("gain", -24.0, 24.0, 0.0);
        assert_eq!(stepped(&linear, 0.0, 1), Some(0.48));
        assert_eq!(stepped(&linear, 23.9, 1), Some(24.0));
        let log = ParameterInfo::new("cutoff", 20.0, 20_000.0, 80.0).with_scale(Scale::Logarithmic);
        let up = stepped(&log, 20.0, 100).unwrap_or_default();
        assert!((up - 20_000.0).abs() < 1.0);
        assert_eq!(stepped(&log, 20.0, -1), Some(20.0));
        let order = ParameterInfo::new("order", 1.0, 8.0, 2.0).with_scale(Scale::Integer);
        assert_eq!(stepped(&order, 2.4, 1), Some(3.0));
        assert_eq!(stepped(&order, 1.0, -1), Some(1.0));
        let options = vec!["a".to_owned(), "b".to_owned(), "c".to_owned()];
        let choice = ParameterInfo::new("type", 0.0, 2.0, 0.0).with_scale(Scale::Choice(options));
        assert_eq!(stepped(&choice, 1.0, 1), Some(2.0));
        assert_eq!(stepped(&choice, 2.0, 1), Some(2.0));
        assert_eq!(stepped(&choice, 0.0, -1), Some(0.0));
        let toggle = ParameterInfo::new("on", 0.0, 1.0, 0.0).with_scale(Scale::Toggle);
        assert_eq!(stepped(&toggle, 0.0, 1), None);
    }

    #[test]
    fn test_stepped_snaps_to_a_declared_step_and_clamps() {
        let gain = ParameterInfo::new("gain", -96.0, 12.0, 0.0).with_step(1.0);
        assert_eq!(stepped(&gain, 0.0, 1), Some(1.0));
        assert_eq!(stepped(&gain, -7.4, 1), Some(-6.0));
        assert_eq!(stepped(&gain, -7.6, -1), Some(-9.0));
        assert_eq!(stepped(&gain, 11.8, 3), Some(12.0));
        assert_eq!(stepped(&gain, -95.5, -1), Some(-96.0));
        let pan = ParameterInfo::new("pan", -1.0, 1.0, 0.0).with_step(0.01);
        assert_eq!(stepped(&pan, 0.0, -1), Some(-0.01));
        assert_eq!(stepped(&pan, 1.0, 1), Some(1.0));
        let toggle = ParameterInfo::new("on", 0.0, 1.0, 0.0)
            .with_scale(Scale::Toggle)
            .with_step(1.0);
        assert_eq!(stepped(&toggle, 0.0, 1), None);
    }

    #[test]
    fn test_stepped_without_a_step_keeps_the_one_percent_rule() {
        let linear = ParameterInfo::new("pan", -1.0, 1.0, 0.0);
        assert_eq!(linear.step, None);
        assert_eq!(stepped(&linear, 0.0, 1), Some(0.02));
    }

    #[test]
    fn test_stepped_named_steps_by_name() {
        let infos = [ParameterInfo::new("gain", -96.0, 12.0, 0.0).with_step(1.0)];
        let stored = stored(&[("gain", -3.2)]);
        let view = NodeParameters::new(NODE, None, &infos, &stored, None, false);
        assert_eq!(view.stepped_named("gain", 1), Some(-2.0));
        assert_eq!(view.stepped_named("missing", 1), None);
    }
}
