//! A built-in effect's dialog body: one control per parameter, chosen by its scale and grouped
//! into strips as tall as the tallest; and the keyboard focus that Tab and presses move through
//! them (`docs/architecture/user-interface.md` "Layout").

use iced::widget::{Space, column, container, pick_list, row, scrollable, slider, text, tooltip};
use iced::{Element, Length};
use signal_graph::{ParameterInfo, Scale};

use crate::app::Message;
use crate::effect_display::display_area;
use crate::equal_heights::equal_heights;
use crate::focus_target::{focus_target, ringed};
use crate::node_dialog::{DialogFocus, DialogMessage};
use crate::node_parameters::NodeParameters;
use crate::theme::{self, Tone};
use crate::view::toggle_button;

/// The gap between a dialog's strips (an effect's groups, a mixer's channels) and their
/// horizontal scrollbar, in logical pixels.
pub(crate) const STRIP_SCROLLBAR_GAP: f32 = 4.0;

/// The width of one control, and of a strip of them.
const CONTROL_WIDTH: f32 = 104.0;

/// The control after `from` (`forward`) or before it in `order`, wrapping at the ends. From no
/// control, or one not in `order`, forward is the first and back the last; `None` if `order` is
/// empty.
pub(crate) fn moved<T: Copy + PartialEq>(from: Option<T>, order: &[T], forward: bool) -> Option<T> {
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

/// The display and controls of the effect whose parameters are `parameters`, with the control
/// at `focus` ringed; empty for a mixer. The display, drawn in `tone` at `sample_rate`, for a
/// whole note of `frames_per_whole_note` frames, and at `scale` physical pixels per logical
/// pixel, comes first; then the preset picker, if the kind
/// has presets, and the ungrouped parameters, in a row; then one strip per group, scrolling
/// horizontally.
pub(crate) fn effect_panel<'a>(
    parameters: &NodeParameters<'a>,
    focus: Option<usize>,
    tone: Tone,
    sample_rate: u32,
    frames_per_whole_note: f64,
    scale: f32,
) -> Element<'a, Message> {
    let Some(kind) = parameters.kind() else {
        return Space::new().into();
    };
    let infos = parameters.infos();
    let display = display_area(
        kind,
        &parameters.values(),
        tone,
        sample_rate,
        frames_per_whole_note,
        scale,
    );
    // Drawn in Tab order, so the two cannot disagree: the ungrouped first, then each group's run
    // as a strip.
    let order = parameters.shown_order();
    let split = order.partition_point(|&i| infos[i].group.is_empty());
    let (ungrouped, grouped) = order.split_at(split);
    let control = |i: usize| control(parameters, i, focus == Some(i));
    let ungrouped = row(preset_picker(parameters, kind))
        .extend(ungrouped.iter().map(|&i| control(i)))
        .spacing(16);
    let strips = grouped
        .chunk_by(|&a, &b| infos[a].group == infos[b].group)
        .map(|run| {
            let group = run.first().map_or("", |&i| infos[i].group.as_str());
            container(
                column![text(capitalised(group)).size(13)]
                    .extend(run.iter().map(|&i| control(i)))
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
        // Embedded, so the scrollbar takes its own row below the strips instead of covering
        // their lowest controls.
        scrollable(strips).direction(scrollable::Direction::Horizontal(
            scrollable::Scrollbar::default().spacing(STRIP_SCROLLBAR_GAP),
        )),
    ]
    .spacing(10)
    .into()
}

/// A pick list of effect kind `kind`'s presets for the node of `parameters`, showing the preset
/// its values match or, if none, "Custom"; `None` for a kind with no presets.
fn preset_picker<'a>(parameters: &NodeParameters<'_>, kind: &str) -> Option<Element<'a, Message>> {
    let names: Vec<&'static str> = audio_effects::presets(kind).map(|p| p.name).collect();
    if names.is_empty() {
        return None;
    }
    let id = parameters.node();
    let selected = parameters.matching_preset().map(|p| p.name);
    let picker = pick_list(names, selected, move |preset| {
        Message::Dialog(DialogMessage::ApplyPreset { node: id, preset })
    })
    .placeholder("Custom")
    .text_size(12)
    .width(Length::Fill);
    Some(
        column![text("Preset").size(12), picker]
            .spacing(4)
            .width(Length::Fixed(CONTROL_WIDTH))
            .into(),
    )
}

/// The control for parameter `infos()[index]` of `parameters`, ringed while it has the keyboard
/// focus (`focused`) and taking the focus when pressed. A pick list keeps the keyboard and wheel;
/// a slider or toggle leaves them to the focused control.
fn control<'a>(
    parameters: &NodeParameters<'a>,
    index: usize,
    focused: bool,
) -> Element<'a, Message> {
    let info = &parameters.infos()[index];
    let keys = matches!(info.scale, Scale::Choice(_));
    ringed(
        focus_target(
            input(parameters, info),
            Message::Dialog(DialogMessage::SetFocus(Some(DialogFocus::Parameter(index)))),
            keys,
        ),
        focused,
    )
}

/// The input for parameter `info` of `parameters`. A toggle is one button named for the
/// parameter, lit while on; any other is labelled with its name, and a slider shows its value as
/// a tooltip.
fn input<'a>(parameters: &NodeParameters<'a>, info: &'a ParameterInfo) -> Element<'a, Message> {
    let id = parameters.node();
    let value = parameters.value(&info.name);
    let (min, max) = (info.min, info.max);
    let slide = move |value: f32| {
        Message::Dialog(DialogMessage::Slide {
            node: id,
            name: info.name.clone(),
            value: value.clamp(min, max),
        })
    };
    let set = move |value: f32| {
        Message::Dialog(DialogMessage::SetParameter {
            node: id,
            name: info.name.clone(),
            value,
        })
    };
    let frozen = parameters.is_frozen(info);
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
            .step(info.step.unwrap_or((max - min) / 1000.0))
            .on_release(Message::Dialog(DialogMessage::Release))
            .into(),
        Scale::Logarithmic => slider(min.ln()..=max.ln(), value.max(min).ln(), move |v| {
            slide(v.exp())
        })
        .step((max.ln() - min.ln()) / 1000.0)
        .on_release(Message::Dialog(DialogMessage::Release))
        .into(),
        Scale::Integer => slider(min..=max, value, slide)
            .step(1.0_f32)
            .on_release(Message::Dialog(DialogMessage::Release))
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
            .width(Length::Fill)
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
pub(crate) fn is_slider(scale: &Scale) -> bool {
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
/// group `"band 1"` is `"Q"`; a name that is all prefix, such as `"ceiling"` in group
/// `"ceiling"`, is kept whole.
fn label(info: &ParameterInfo) -> String {
    let name = info
        .name
        .strip_prefix(info.group.as_str())
        .map(str::trim_start)
        .filter(|rest| !rest.is_empty())
        .unwrap_or(info.name.as_str());
    capitalised(name)
}

/// `value` with its unit, to the decimal places of the parameter's declared step if it has one,
/// else to a precision that suits its size; hertz from 1000 up as kilohertz; the parameter's
/// maximum label, if it has one, at the maximum.
pub(crate) fn format_value(info: &ParameterInfo, value: f32) -> String {
    if let Some(label) = info.max_label.as_ref().filter(|_| value >= info.max) {
        return label.clone();
    }
    let (value, unit) = match info.unit.as_str() {
        "Hz" if value >= 1000.0 => (value / 1000.0, "kHz"),
        unit => (value, unit),
    };
    let number = if let Some(places) = info.step.and_then(step_places) {
        format!("{value:.places$}")
    } else if info.scale == Scale::Integer || value.abs() >= 100.0 {
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

/// The decimal places a step of `step` needs: the fewest, up to 6, that write it exactly, such
/// as 0 for 1, 1 for 0.5 and 2 for 0.01 or 0.25; `None` unless it is positive and finite.
fn step_places(step: f32) -> Option<usize> {
    if !(step.is_finite() && step > 0.0) {
        return None;
    }
    let mut scaled = f64::from(step);
    for places in 0..6 {
        if (scaled - scaled.round()).abs() < 1e-4 * scaled.max(1.0) {
            return Some(places);
        }
        scaled *= 10.0;
    }
    Some(6)
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
        let whole = ParameterInfo::new("ceiling", -24.0, 0.0, -1.0).with_group("ceiling");
        assert_eq!(label(&whole), "Ceiling");
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
        let ratio = ParameterInfo::new("ratio", 1.0, 100.0, 4.0).with_max_label("∞");
        assert_eq!(format_value(&ratio, 4.0), "4.00");
        assert_eq!(format_value(&ratio, 100.0), "∞");
    }

    #[test]
    fn test_format_value_takes_precision_from_a_declared_step() {
        let gain = ParameterInfo::new("gain", -96.0, 12.0, 0.0)
            .with_unit("dB")
            .with_step(1.0);
        assert_eq!(format_value(&gain, -3.0), "-3 dB");
        assert_eq!(format_value(&gain, -2.6), "-3 dB");
        let pan = ParameterInfo::new("pan", -1.0, 1.0, 0.0).with_step(0.01);
        assert_eq!(format_value(&pan, 0.35), "0.35");
        let coarse = ParameterInfo::new("time", 0.0, 500.0, 0.0).with_step(0.5);
        assert_eq!(format_value(&coarse, 12.5), "12.5");
        assert_eq!(step_places(0.0), None);
        assert_eq!(step_places(10.0), Some(0));
        assert_eq!(step_places(0.25), Some(2));
        assert_eq!(step_places(0.01), Some(2));
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
    fn test_is_slider_covers_the_numeric_scales() {
        assert!(is_slider(&Scale::Linear) && is_slider(&Scale::Logarithmic));
        assert!(is_slider(&Scale::Integer));
        assert!(!is_slider(&Scale::Toggle) && !is_slider(&Scale::Choice(Vec::new())));
    }
}
