//! A built-in effect's dialog body: one control per parameter, chosen by its scale and grouped
//! into strips as tall as the tallest; and the keyboard focus that Tab and presses move through
//! them (`docs/architecture/user-interface.md` "Layout").

use iced::widget::{Space, column, container, pick_list, row, scrollable, slider, text, tooltip};
use iced::{Element, Length};
use project_model::NodeId;
use signal_graph::{ParameterInfo, Scale};

use crate::app::{App, DialogFocus, Message};
use crate::effect_display::display_area;
use crate::equal_heights::equal_heights;
use crate::focus_target::{focus_target, ringed};
use crate::theme;
use crate::view::toggle_button;

/// The gap between a dialog's strips (an effect's groups, a mixer's channels) and their
/// horizontal scrollbar, in logical pixels.
pub(crate) const STRIP_SCROLLBAR_GAP: f32 = 4.0;

/// The width of one control, and of a strip of them.
const CONTROL_WIDTH: f32 = 104.0;

/// The share of a slider's range (of its logarithm, if logarithmic) an arrow key moves it.
const ARROW_STEP: f32 = 0.01;

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

/// The control after `from` (`forward`) or before it in `order`, wrapping at the ends. From no
/// control, or one not in `order`, forward is the first and back the last; `None` if `order` is
/// empty.
pub(crate) fn moved(from: Option<usize>, order: &[usize], forward: bool) -> Option<usize> {
    let count = order.len();
    if count == 0 {
        return None;
    }
    let next = match (
        from.and_then(|f| order.iter().position(|&i| i == f)),
        forward,
    ) {
        (None, true) => 0,
        (None, false) => count - 1,
        (Some(at), true) => (at + 1) % count,
        (Some(at), false) => (at + count - 1) % count,
    };
    Some(order[next])
}

/// Parameter `info` at `value` moved `delta` arrow presses up (positive) or down, within its
/// range: a linear slider by [`ARROW_STEP`] of its range, a logarithmic one by that share of its
/// logarithm's, an integer by one, a choice to the next or previous option. `None` for a toggle,
/// which arrows leave alone.
pub(crate) fn stepped(info: &ParameterInfo, value: f32, delta: i32) -> Option<f32> {
    #[allow(clippy::cast_precision_loss)] // a handful of key presses
    let delta = delta as f32;
    let (min, max) = (info.min, info.max);
    let value = match &info.scale {
        Scale::Toggle => return None,
        Scale::Linear => value + delta * ARROW_STEP * (max - min),
        Scale::Logarithmic => {
            let step = ARROW_STEP * (max.ln() - min.ln());
            (value.max(min).ln() + delta * step).exp()
        }
        Scale::Integer => value.round() + delta,
        Scale::Choice(options) => {
            #[allow(clippy::cast_precision_loss)] // a handful of options
            let last = options.len().saturating_sub(1) as f32;
            (value.round() + delta).clamp(0.0, last)
        }
    };
    Some(value.clamp(min, max))
}

impl App {
    /// The display and controls of effect node `id` of kind `kind`; empty for a kind this build does not know. The display comes first; then the
    /// ungrouped parameters, in a row; then one strip per group, scrolling horizontally.
    pub(crate) fn effect_panel(&self, id: NodeId, kind: &str) -> Element<'_, Message> {
        let Some(infos) = self.effect_parameters(kind) else {
            return Space::new().into();
        };
        let values: Vec<f32> = infos
            .iter()
            .map(|info| self.parameter_value(id, &info.name))
            .collect();
        let display = display_area(
            kind,
            infos,
            values,
            self.tone(),
            self.sample_rate(),
            self.scale_factor(),
        );
        // Drawn in Tab order, so the two cannot disagree: the ungrouped first, then each group's
        // run as a strip.
        let order = tab_order(infos);
        let split = order.partition_point(|&i| infos[i].group.is_empty());
        let (ungrouped, grouped) = order.split_at(split);
        let ungrouped = row(ungrouped.iter().map(|&i| self.control(id, infos, i))).spacing(16);
        let strips = grouped
            .chunk_by(|&a, &b| infos[a].group == infos[b].group)
            .map(|run| {
                let group = run.first().map_or("", |&i| infos[i].group.as_str());
                container(
                    column![text(capitalised(group)).size(13)]
                        .extend(run.iter().map(|&i| self.control(id, infos, i)))
                        .spacing(8),
                )
                .padding(6)
                .style(theme::strip)
                .into()
            });
        let strips = equal_heights(strips, 8.0);
        column![
            display,
            ungrouped,
            // Embedded, so the scrollbar takes its own row below the strips instead of
            // covering their lowest controls.
            scrollable(strips).direction(scrollable::Direction::Horizontal(
                scrollable::Scrollbar::default().spacing(STRIP_SCROLLBAR_GAP),
            )),
        ]
        .spacing(10)
        .into()
    }

    /// The control for parameter `infos[index]` of node `id`, ringed while it has the keyboard
    /// focus and taking the focus when pressed. A pick list keeps the keyboard and wheel; a
    /// slider or toggle leaves them to the focused control.
    fn control<'a>(
        &'a self,
        id: NodeId,
        infos: &'a [ParameterInfo],
        index: usize,
    ) -> Element<'a, Message> {
        let info = &infos[index];
        let keys = matches!(info.scale, Scale::Choice(_));
        ringed(
            focus_target(
                self.input(id, info),
                Message::SetFocus(Some(DialogFocus::Parameter(index))),
                keys,
            ),
            self.effect_focus() == Some(index),
        )
    }

    /// The input for parameter `info` of node `id`. A toggle is one button named for the
    /// parameter, lit while on; any other is labelled with its name, and a slider shows its value
    /// as a tooltip.
    fn input<'a>(&'a self, id: NodeId, info: &'a ParameterInfo) -> Element<'a, Message> {
        let value = self.parameter_value(id, &info.name);
        let (min, max) = (info.min, info.max);
        let slide = move |value: f32| Message::Slide {
            node: id,
            name: info.name.clone(),
            value: value.clamp(min, max),
        };
        let set = move |value: f32| Message::SetParameter {
            node: id,
            name: info.name.clone(),
            value,
        };
        let frozen = info.stopped_only && self.engine().is_playing();
        if !frozen && info.scale == Scale::Toggle {
            let on = value >= 0.5;
            return container(toggle_button(
                toggle_label(info, on),
                set(if on { 0.0 } else { 1.0 }),
                on,
                !info.group.is_empty(),
            ))
            .width(Length::Fixed(CONTROL_WIDTH))
            .into();
        }
        let input: Element<'a, Message> = match &info.scale {
            // Shown, not offered, until playback stops (ADR-0088).
            _ if frozen => text("Stop to change").size(11).into(),
            Scale::Linear => slider(min..=max, value, slide)
                .step((max - min) / 1000.0)
                .on_release(Message::Release)
                .into(),
            Scale::Logarithmic => slider(min.ln()..=max.ln(), value.max(min).ln(), move |v| {
                slide(v.exp())
            })
            .step((max.ln() - min.ln()) / 1000.0)
            .on_release(Message::Release)
            .into(),
            Scale::Integer => slider(min..=max, value, slide)
                .step(1.0_f32)
                .on_release(Message::Release)
                .into(),
            Scale::Choice(options) => {
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // a small index
                let selected = options.get(value.round().max(0.0) as usize).cloned();
                pick_list(options.as_slice(), selected, move |choice: String| {
                    let index = options.iter().position(|o| *o == choice).unwrap_or(0);
                    #[allow(clippy::cast_precision_loss)] // a handful of options
                    set(index as f32)
                })
                .text_size(12)
                .into()
            }
            // A toggle is a button unless frozen.
            Scale::Toggle => Space::new().into(),
        };
        let input = if !frozen && is_slider(&info.scale) {
            tooltip(
                input,
                container(text(format_value(info, value)).size(12))
                    .padding([4, 8])
                    .style(theme::popup),
                tooltip::Position::FollowCursor,
            )
            .into()
        } else {
            input
        };
        column![text(label(info)).size(12), input]
            .spacing(4)
            .width(Length::Fixed(CONTROL_WIDTH))
            .into()
    }
}

/// The text on toggle `info`'s button while it is `on`: its state's label if it names its
/// states, else its [`label`].
fn toggle_label(info: &ParameterInfo, on: bool) -> String {
    match &info.state_labels {
        Some((on_label, _)) if on => on_label.clone(),
        Some((_, off_label)) => off_label.clone(),
        None => label(info),
    }
}

/// Whether a parameter of `scale` is shown as a slider.
fn is_slider(scale: &Scale) -> bool {
    matches!(scale, Scale::Linear | Scale::Logarithmic | Scale::Integer)
}

/// `text` with its first letter in capitals.
fn capitalised(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

/// A parameter's label: its name without its group's prefix, capitalised, so `"band 1 q"` in
/// group `"band 1"` is `"Q"`.
fn label(info: &ParameterInfo) -> String {
    let name = info
        .name
        .strip_prefix(info.group.as_str())
        .map_or(info.name.as_str(), str::trim_start);
    capitalised(name)
}

/// `value` with its unit, to a precision that suits its size; hertz from 1000 up as kilohertz.
fn format_value(info: &ParameterInfo, value: f32) -> String {
    let (value, unit) = match info.unit.as_str() {
        "Hz" if value >= 1000.0 => (value / 1000.0, "kHz"),
        unit => (value, unit),
    };
    let number = if info.scale == Scale::Integer || value.abs() >= 100.0 {
        format!("{value:.0}")
    } else if value.abs() >= 10.0 {
        format!("{value:.1}")
    } else {
        format!("{value:.2}")
    };
    if unit.is_empty() {
        number
    } else {
        format!("{number} {unit}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_label_strips_the_group_and_capitalises() {
        let info = ParameterInfo::new("band 3 q", 0.1, 18.0, 0.7).with_group("band 3");
        assert_eq!(label(&info), "Q");
        assert_eq!(
            label(&ParameterInfo::new("cutoff", 20.0, 2e4, 80.0)),
            "Cutoff"
        );
        assert_eq!(capitalised(""), "");
    }

    #[test]
    fn test_format_value_scales_units_and_precision() {
        let hz = ParameterInfo::new("cutoff", 20.0, 2e4, 80.0).with_unit("Hz");
        assert_eq!(format_value(&hz, 80.0), "80.0 Hz");
        assert_eq!(format_value(&hz, 250.0), "250 Hz");
        assert_eq!(format_value(&hz, 12_000.0), "12.0 kHz");
        assert_eq!(format_value(&hz, 1_500.0), "1.50 kHz");
        let db = ParameterInfo::new("gain", -24.0, 24.0, 0.0).with_unit("dB");
        assert_eq!(format_value(&db, -3.25), "-3.25 dB");
        let order = ParameterInfo::new("order", 1.0, 8.0, 2.0).with_scale(Scale::Integer);
        assert_eq!(format_value(&order, 4.0), "4");
    }

    #[test]
    fn test_toggle_label_names_each_state_or_falls_back_to_the_name() {
        let plain = ParameterInfo::new("band 1 enabled", 0.0, 1.0, 0.0)
            .with_scale(Scale::Toggle)
            .with_group("band 1");
        assert_eq!(toggle_label(&plain, true), "Enabled");
        assert_eq!(toggle_label(&plain, false), "Enabled");
        let named = plain.with_state_labels("Enabled", "Disabled");
        assert_eq!(toggle_label(&named, true), "Enabled");
        assert_eq!(toggle_label(&named, false), "Disabled");
    }

    #[test]
    fn test_tab_order_lists_ungrouped_then_groups() -> Result<(), Box<dyn std::error::Error>> {
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
    fn test_moved_wraps_and_starts_from_the_ends() {
        let order = [1, 4, 0];
        assert_eq!(moved(None, &order, true), Some(1));
        assert_eq!(moved(None, &order, false), Some(0));
        assert_eq!(moved(Some(4), &order, true), Some(0));
        assert_eq!(moved(Some(0), &order, true), Some(1));
        assert_eq!(moved(Some(1), &order, false), Some(0));
        assert_eq!(moved(Some(9), &order, true), Some(1));
        assert_eq!(moved(Some(0), &[], true), None);
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
    fn test_is_slider_covers_the_numeric_scales() {
        assert!(is_slider(&Scale::Linear) && is_slider(&Scale::Logarithmic));
        assert!(is_slider(&Scale::Integer));
        assert!(!is_slider(&Scale::Toggle) && !is_slider(&Scale::Choice(Vec::new())));
    }
}
