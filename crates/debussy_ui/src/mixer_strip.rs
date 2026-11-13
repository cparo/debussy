//! A mixer's dialog body: a "Channels" field above narrow numbered strips, each with mute above
//! solo, then vertical gain and pan sliders with their values as hover tips; and the keyboard
//! focus that Tab and presses move through them (`docs/architecture/user-interface.md` "Layout").

use audio_effects::{Control, MAX_GAIN_DB, Mode, SILENCE_DB, parameter_name};
use iced::widget::canvas::{Frame, Geometry, Path, Program, Stroke};
use iced::widget::{
    button, canvas, column, container, row, scrollable, stack, text, text_input, tooltip,
    vertical_slider,
};
use iced::{Alignment, Color, Element, Fill, Length, Point, Rectangle, Renderer, Theme, mouse};
use project_model::NodeId;

use crate::app::{App, Message};
use crate::effect_panel::STRIP_SCROLLBAR_GAP;
use crate::focus_target::{focus_target, ringed};
use crate::node_dialog::{DialogFocus, DialogMessage};
use crate::theme::{self, Accent, Place, tokens, tone_of};

/// The gain slider's step, and an arrow key's change to it, in dB.
pub(crate) const GAIN_STEP_DB: f32 = 1.0;
/// The pan slider's step, and an arrow key's change to it.
pub(crate) const PAN_STEP: f32 = 0.01;
/// The id of the "Channels" field, to focus it.
pub(crate) const CHANNELS_INPUT: &str = "mixer-channels";
/// An id no widget has: focusing it takes the focus from every text field.
pub(crate) const NO_FIELD: &str = "no-field";
/// A strip's width.
const STRIP_WIDTH: f32 = 52.0;
/// The width of a slider, and of the tic mark layered under it.
const SLIDER_WIDTH: f32 = 24.0;
/// The tic mark's length: wider than the slider's rail.
const TIC_LENGTH: f32 = 18.0;
/// The gain slider's height.
const GAIN_HEIGHT: f32 = 120.0;
/// The pan slider's height.
const PAN_HEIGHT: f32 = 80.0;
/// The radius of iced's default slider handle, a circle (the theme sets no slider style).
const HANDLE_RADIUS: f32 = 7.0;
/// The size of the end labels.
const LABEL_SIZE: f32 = 10.0;
/// The size of the mute and solo buttons' labels.
const BUTTON_TEXT: f32 = 10.0;
/// The size of hover tips' text.
const TIP_SIZE: f32 = 13.0;
/// The channel-count field's width.
const CHANNELS_WIDTH: f32 = 60.0;
/// The space between a strip's parts: number, gain, pan, and buttons.
const PART_SPACING: f32 = 8.0;
/// The space within a part, between a slider and its labels.
const LABEL_SPACING: f32 = 2.0;

/// A mixer dialog control that can hold the keyboard focus. Tab visits them in this order: the
/// "Channels" field, then each channel's mute, solo, gain and pan, from the first channel to the
/// last.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MixerFocus {
    /// The "Channels" field.
    Channels,
    /// A channel's gain slider, by channel index from 0.
    Gain(usize),
    /// A channel's pan slider.
    Pan(usize),
    /// A channel's mute button.
    Mute(usize),
    /// A channel's solo button.
    Solo(usize),
}

impl MixerFocus {
    /// The controls on one strip.
    const PER_CHANNEL: usize = 4;

    /// Where this control falls in Tab order.
    fn index(self) -> usize {
        let at = |ch: usize, offset: usize| 1 + ch * Self::PER_CHANNEL + offset;
        match self {
            MixerFocus::Channels => 0,
            MixerFocus::Mute(ch) => at(ch, 0),
            MixerFocus::Solo(ch) => at(ch, 1),
            MixerFocus::Gain(ch) => at(ch, 2),
            MixerFocus::Pan(ch) => at(ch, 3),
        }
    }

    /// The control at `index` in Tab order.
    fn at(index: usize) -> Self {
        let Some(i) = index.checked_sub(1) else {
            return MixerFocus::Channels;
        };
        let ch = i / Self::PER_CHANNEL;
        match i % Self::PER_CHANNEL {
            0 => MixerFocus::Mute(ch),
            1 => MixerFocus::Solo(ch),
            2 => MixerFocus::Gain(ch),
            _ => MixerFocus::Pan(ch),
        }
    }

    /// The control after `from` (`forward`) or before it, in a mixer of `channels` channels,
    /// wrapping at the ends. From no control, forward is the "Channels" field and back the last
    /// pan slider.
    pub(crate) fn moved(from: Option<Self>, channels: usize, forward: bool) -> Self {
        let count = 1 + channels * Self::PER_CHANNEL;
        let next = match (from, forward) {
            (None, true) => 0,
            (None, false) => count - 1,
            (Some(focus), true) => (focus.index().min(count - 1) + 1) % count,
            (Some(focus), false) => (focus.index().min(count - 1) + count - 1) % count,
        };
        Self::at(next)
    }
}

/// The mode a mute or solo button for `target` sets when the channel is in `current`: `target`,
/// or back to playing if it is already `target`.
pub(crate) fn toggled(current: Mode, target: Mode) -> Mode {
    if current == target {
        Mode::Play
    } else {
        target
    }
}

/// One horizontal tic mark across a vertical slider, at `at`, a fraction of the way from the
/// slider's minimum to its maximum; drawn under the slider, so the handle covers it.
struct Tic {
    /// Where the mark is, from the minimum (0) to the maximum (1).
    at: f32,
}

/// The y, from the top, of a vertical slider's handle centre when the slider is `height` tall
/// and its value `fraction` of the way from its minimum (at the bottom) to its maximum.
fn tic_y(height: f32, fraction: f32) -> f32 {
    let handle = 2.0 * HANDLE_RADIUS;
    handle / 2.0 + (height - handle).max(0.0) * (1.0 - fraction.clamp(0.0, 1.0))
}

impl Program<Message> for Tic {
    type State = ();

    fn draw(
        &self,
        _state: &(),
        renderer: &Renderer,
        theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let t = tokens(tone_of(theme));
        let mut frame = Frame::new(renderer, bounds.size());
        let y = tic_y(bounds.height, self.at).round() + 0.5;
        let left = (bounds.width - TIC_LENGTH) / 2.0;
        frame.stroke(
            &Path::line(Point::new(left, y), Point::new(left + TIC_LENGTH, y)),
            Stroke::default()
                .with_width(1.0)
                .with_color(Color { a: 0.4, ..t.text }),
        );
        vec![frame.into_geometry()]
    }
}

/// `control` with a hover tip showing `tip` at the cursor.
fn tipped<'a>(control: Element<'a, Message>, tip: String) -> Element<'a, Message> {
    tooltip(
        control,
        container(text(tip).size(TIP_SIZE))
            .padding([4, 8])
            .style(theme::popup),
        tooltip::Position::FollowCursor,
    )
    .into()
}

/// A pan value as shown: "C" at the centre, else the side and percent, such as "L 35".
fn pan_label(pan: f32) -> String {
    let percent = (pan.abs() * 100.0).round();
    if percent == 0.0 {
        "C".to_owned()
    } else if pan < 0.0 {
        format!("L {percent}")
    } else {
        format!("R {percent}")
    }
}

/// A small end label for a slider.
fn label<'a>(s: String) -> Element<'a, Message> {
    text(s).size(LABEL_SIZE).into()
}

/// A full-width mute or solo button labelled `name`, lit when `selected`.
fn mode_button<'a>(name: &'static str, message: Message, selected: bool) -> Element<'a, Message> {
    button(text(name).size(BUTTON_TEXT).center().width(Fill))
        .on_press(message)
        .padding([4, 2])
        .width(Fill)
        .style(theme::accent_style(
            selected,
            Place::Alone,
            true,
            Accent::Audio,
        ))
        .into()
}

/// One of the channel field's up and down buttons, sending `message` if there is one.
fn spin<'a>(glyph: &'a str, message: Option<Message>) -> Element<'a, Message> {
    button(text(glyph).size(8).center())
        .on_press_maybe(message)
        .padding([1, 6])
        .style(theme::flat)
        .into()
}

impl App {
    /// Mixer `id`'s controls: a "Channels" field above its `channels` strips, which scroll
    /// horizontally.
    pub(crate) fn mixer_panel(&self, id: NodeId, channels: usize) -> Element<'_, Message> {
        let typed = match &self.node_text {
            Some((node, typed)) if *node == id => typed.clone(),
            _ => format!("{channels}"),
        };
        let range = self.channel_range(id);
        let up = range.as_ref().is_some_and(|r| channels < *r.end());
        let down = range.as_ref().is_some_and(|r| channels > *r.start());
        let field = row![
            text("Channels").size(14),
            ringed(
                focus_target(
                    row![
                        text_input("1", &typed)
                            .id(CHANNELS_INPUT)
                            .on_input(move |t| Message::NodeText(id, t))
                            .on_submit(Message::CommitNodeText)
                            .width(Length::Fixed(CHANNELS_WIDTH)),
                        column![
                            spin("▲", up.then_some(Message::StepMixerChannels(id, 1))),
                            spin("▼", down.then_some(Message::StepMixerChannels(id, -1))),
                        ],
                    ]
                    .align_y(Alignment::Center),
                    Message::Dialog(DialogMessage::SetFocus(Some(DialogFocus::Mixer(
                        MixerFocus::Channels,
                    )))),
                    true,
                ),
                self.mixer_focus() == Some(MixerFocus::Channels),
            ),
        ]
        .spacing(10)
        .align_y(Alignment::Center);
        let strips = row((0..channels).map(|ch| self.strip(id, ch))).spacing(4);
        column![
            field,
            // Embedded, so the scrollbar takes its own row below the strips instead of
            // covering their lowest controls.
            scrollable(strips).direction(scrollable::Direction::Horizontal(
                scrollable::Scrollbar::default().spacing(STRIP_SCROLLBAR_GAP),
            )),
        ]
        .spacing(10)
        .into()
    }

    /// One mixer channel: its number, gain, pan, mute and solo.
    fn strip(&self, id: NodeId, ch: usize) -> Element<'_, Message> {
        let gain_name = parameter_name(ch, Control::Gain);
        let pan_name = parameter_name(ch, Control::Pan);
        let mode_name = parameter_name(ch, Control::Mode);
        let gain = self.parameter_value(id, &gain_name);
        let pan = self.parameter_value(id, &pan_name);
        let mode = Mode::from_value(self.parameter_value(id, &mode_name));
        let set_mode = |mode: Mode| {
            Message::Dialog(DialogMessage::ToggleMode {
                node: id,
                channel: ch,
                mode,
            })
        };
        // A press on a control focuses it; sliders leave the arrow keys to the focused control.
        let target = |control: Element<'static, Message>, focus: MixerFocus| {
            ringed(
                focus_target(
                    control,
                    Message::Dialog(DialogMessage::SetFocus(Some(DialogFocus::Mixer(focus)))),
                    false,
                ),
                self.mixer_focus() == Some(focus),
            )
        };
        let unity = (0.0 - SILENCE_DB) / (MAX_GAIN_DB - SILENCE_DB);
        let gain_slider = stack![
            canvas(Tic { at: unity })
                .width(Length::Fixed(SLIDER_WIDTH))
                .height(Length::Fixed(GAIN_HEIGHT)),
            vertical_slider(SILENCE_DB..=MAX_GAIN_DB, gain, move |value| {
                Message::Dialog(DialogMessage::Slide {
                    node: id,
                    name: gain_name.clone(),
                    value,
                })
            })
            .step(GAIN_STEP_DB)
            .on_release(Message::Dialog(DialogMessage::Release))
            .width(SLIDER_WIDTH)
            .height(Length::Fixed(GAIN_HEIGHT)),
        ];
        // The slider runs from right (bottom) to left (top), so it shows the negated pan.
        let pan_slider = stack![
            canvas(Tic { at: 0.5 })
                .width(Length::Fixed(SLIDER_WIDTH))
                .height(Length::Fixed(PAN_HEIGHT)),
            vertical_slider(-1.0..=1.0, -pan, move |value: f32| {
                Message::Dialog(DialogMessage::Slide {
                    node: id,
                    name: pan_name.clone(),
                    value: -value,
                })
            })
            .step(PAN_STEP)
            .on_release(Message::Dialog(DialogMessage::Release))
            .width(SLIDER_WIDTH)
            .height(Length::Fixed(PAN_HEIGHT)),
        ];
        let gain_part = column![
            label(format!("{MAX_GAIN_DB:+}dB")),
            target(
                tipped(gain_slider.into(), format!("{gain:.0} dB")),
                MixerFocus::Gain(ch),
            ),
            label(format!("{SILENCE_DB:+}dB")),
        ]
        .spacing(LABEL_SPACING)
        .align_x(Alignment::Center);
        let pan_part = column![
            label("L".to_owned()),
            target(
                tipped(pan_slider.into(), pan_label(pan)),
                MixerFocus::Pan(ch)
            ),
            label("R".to_owned()),
        ]
        .spacing(LABEL_SPACING)
        .align_x(Alignment::Center);
        let buttons = column![
            target(
                mode_button("Mute", set_mode(Mode::Mute), mode == Mode::Mute),
                MixerFocus::Mute(ch),
            ),
            target(
                mode_button("Solo", set_mode(Mode::Solo), mode == Mode::Solo),
                MixerFocus::Solo(ch),
            ),
        ]
        .spacing(LABEL_SPACING);
        column![text(ch + 1).size(13), buttons, gain_part, pan_part]
            .spacing(PART_SPACING)
            .align_x(Alignment::Center)
            .width(Length::Fixed(STRIP_WIDTH))
            .into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tic_y_matches_handle_centre() {
        assert_eq!(tic_y(120.0, 1.0), 7.0);
        assert_eq!(tic_y(120.0, 0.0), 113.0);
        assert_eq!(tic_y(80.0, 0.5), 40.0);
    }

    #[test]
    fn test_tic_y_clamps_fraction() {
        assert_eq!(tic_y(100.0, 2.0), 7.0);
        assert_eq!(tic_y(100.0, -1.0), 93.0);
    }

    #[test]
    fn test_pan_label_names_side_and_percent() {
        assert_eq!(pan_label(0.0), "C");
        assert_eq!(pan_label(0.001), "C");
        assert_eq!(pan_label(-0.35), "L 35");
        assert_eq!(pan_label(0.2), "R 20");
    }

    #[test]
    fn test_moved_goes_down_each_strip_then_across() {
        use MixerFocus::{Channels, Gain, Mute, Pan, Solo};
        let mut order = vec![];
        let mut focus = None;
        for _ in 0..10 {
            let next = MixerFocus::moved(focus, 2, true);
            order.push(next);
            focus = Some(next);
        }
        assert_eq!(
            order,
            [
                Channels,
                Mute(0),
                Solo(0),
                Gain(0),
                Pan(0),
                Mute(1),
                Solo(1),
                Gain(1),
                Pan(1),
                Channels
            ]
        );
    }

    #[test]
    fn test_moved_backwards_and_from_a_removed_channel() {
        use MixerFocus::{Channels, Gain, Mute, Pan};
        assert_eq!(MixerFocus::moved(None, 2, false), Pan(1));
        assert_eq!(MixerFocus::moved(Some(Channels), 2, false), Pan(1));
        assert_eq!(MixerFocus::moved(Some(Mute(1)), 2, false), Pan(0));
        // Focus on a channel since removed counts from the last control.
        assert_eq!(MixerFocus::moved(Some(Gain(5)), 1, true), Channels);
        assert_eq!(MixerFocus::moved(Some(Gain(5)), 1, false), Gain(0));
    }

    #[test]
    fn test_toggled_sets_or_clears_mode() {
        assert_eq!(toggled(Mode::Play, Mode::Mute), Mode::Mute);
        assert_eq!(toggled(Mode::Mute, Mode::Mute), Mode::Play);
        assert_eq!(toggled(Mode::Mute, Mode::Solo), Mode::Solo);
    }
}
