//! A built-in effect's dialog body: one control per parameter, chosen by its scale and grouped
//! into strips (`docs/architecture/user-interface.md` "Layout").

use iced::widget::{Space, column, container, pick_list, row, scrollable, slider, text, tooltip};
use iced::{Element, Length};
use project_model::NodeId;
use signal_graph::{ParameterInfo, Scale};

use crate::app::{App, Message};
use crate::effect_display::display_area;
use crate::theme;
use crate::view::toggle_button;

/// The width of one control, and of a strip of them.
const CONTROL_WIDTH: f32 = 104.0;

impl App {
    /// The display and controls of effect node `id` of kind `kind`, `width` logical pixels
    /// wide; empty for a kind this build does not know. The display comes first; then the
    /// ungrouped parameters, in a row; then one strip per group, scrolling horizontally.
    pub(crate) fn effect_panel(&self, id: NodeId, kind: &str, width: f32) -> Element<'_, Message> {
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
            &values,
            self.tone(),
            self.sample_rate(),
            width,
            self.scale_factor(),
        );
        let ungrouped = row(infos
            .iter()
            .filter(|i| i.group.is_empty())
            .map(|info| self.control(id, info)))
        .spacing(16);
        let mut groups: Vec<&str> = Vec::new();
        for info in infos.iter().filter(|i| !i.group.is_empty()) {
            if !groups.contains(&info.group.as_str()) {
                groups.push(&info.group);
            }
        }
        let strips = row(groups.into_iter().map(|group| {
            let controls = infos
                .iter()
                .filter(|i| i.group == group)
                .map(|info| self.control(id, info));
            container(
                column![text(capitalised(group)).size(13)]
                    .extend(controls)
                    .spacing(8),
            )
            .padding(6)
            .style(theme::strip)
            .into()
        }))
        .spacing(8);
        column![
            display,
            ungrouped,
            scrollable(strips).direction(scrollable::Direction::Horizontal(
                scrollable::Scrollbar::default(),
            )),
        ]
        .spacing(10)
        .into()
    }

    /// The control for parameter `info` of node `id`. A toggle is one button named for the
    /// parameter, lit while on; any other is labelled with its name, and a slider shows its value
    /// as a tooltip.
    fn control<'a>(&'a self, id: NodeId, info: &'a ParameterInfo) -> Element<'a, Message> {
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
                label(info),
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
    fn test_is_slider_covers_the_numeric_scales() {
        assert!(is_slider(&Scale::Linear) && is_slider(&Scale::Logarithmic));
        assert!(is_slider(&Scale::Integer));
        assert!(!is_slider(&Scale::Toggle) && !is_slider(&Scale::Choice(Vec::new())));
    }
}
