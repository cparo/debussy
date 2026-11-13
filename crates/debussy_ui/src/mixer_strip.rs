//! A mixer's dialog body: a "Channels" field above narrow numbered strips, each with mute above
//! solo, then vertical gain and pan sliders with their values as hover tips; and the keyboard
//! focus that Tab and presses move through them (`docs/architecture/user-interface.md` "Layout").

use audio_effects::{Control, Mode, parameter_name};
use iced::widget::canvas::{Frame, Geometry, Path, Program, Stroke};
use iced::widget::{
    button, canvas, column, container, row, scrollable, stack, text, text_input, tooltip,
    vertical_slider,
};
use iced::{Alignment, Color, Element, Fill, Length, Point, Rectangle, Renderer, Theme, mouse};
use signal_graph::ParameterInfo;

use crate::app::{App, Message};
use crate::effect_panel::{STRIP_SCROLLBAR_GAP, format_value};
use crate::focus_target::{focus_target, ringed};
use crate::node_dialog::{DialogFocus, DialogMessage};
use crate::node_parameters::NodeParameters;
use crate::theme::{self, Accent, Place, tokens, tone_of};

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
/// last; the dialog's "Delete" button follows.
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

    /// The control and channel of a gain or pan slider; `None` for any other control. Pan is
    /// drawn reversed, left at the top, so an up arrow moves it toward lower values.
    pub(crate) fn slider(self) -> Option<(Control, usize)> {
        match self {
            MixerFocus::Gain(ch) => Some((Control::Gain, ch)),
            MixerFocus::Pan(ch) => Some((Control::Pan, ch)),
            MixerFocus::Channels | MixerFocus::Mute(_) | MixerFocus::Solo(_) => None,
        }
    }

    /// The controls of a mixer of `channels` channels, in Tab order.
    pub(crate) fn order(channels: usize) -> impl Iterator<Item = Self> {
        (0..1 + channels * Self::PER_CHANNEL).map(Self::at)
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

/// How far `value` is from `min` (0) to `max` (1); 0.5 for an empty range.
fn fraction(min: f32, max: f32, value: f32) -> f32 {
    if max > min {
        (value - min) / (max - min)
    } else {
        0.5
    }
}

/// A vertical slider `height` tall for parameter `info` of `parameters`, over a tic mark at its
/// default, from its range and declared step. `reversed` draws the maximum at the bottom.
fn fader<'a>(
    parameters: &NodeParameters<'a>,
    info: &'a ParameterInfo,
    height: f32,
    reversed: bool,
) -> Element<'a, Message> {
    let id = parameters.node();
    // A reversed slider runs over the negated range and shows the negated value.
    let sign = if reversed { -1.0 } else { 1.0 };
    let (min, max) = if reversed {
        (-info.max, -info.min)
    } else {
        (info.min, info.max)
    };
    let value = sign * parameters.value(&info.name);
    let at = fraction(min, max, sign * info.default);
    stack![
        canvas(Tic { at })
            .width(Length::Fixed(SLIDER_WIDTH))
            .height(Length::Fixed(height)),
        vertical_slider(min..=max, value, move |value: f32| {
            Message::Dialog(DialogMessage::Slide {
                node: id,
                name: info.name.clone(),
                value: sign * value,
            })
        })
        .step(info.step.unwrap_or((max - min) / 1000.0))
        .on_release(Message::Dialog(DialogMessage::Release))
        .width(SLIDER_WIDTH)
        .height(Length::Fixed(height)),
    ]
    .into()
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
    /// The controls of the mixer whose parameters are `parameters`: a "Channels" field above its
    /// `channels` strips, which scroll horizontally.
    pub(crate) fn mixer_panel<'a>(
        &'a self,
        parameters: &NodeParameters<'a>,
        channels: usize,
    ) -> Element<'a, Message> {
        let id = parameters.node();
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
        let strips = row((0..channels).map(|ch| self.strip(parameters, ch))).spacing(4);
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

    /// One mixer channel of `parameters`: its number, gain, pan, mute and solo. Gain and pan take
    /// their range, step, tic and value text from their parameters' metadata.
    fn strip<'a>(&'a self, parameters: &NodeParameters<'a>, ch: usize) -> Element<'a, Message> {
        let id = parameters.node();
        let gain_name = parameter_name(ch, Control::Gain);
        let pan_name = parameter_name(ch, Control::Pan);
        let mode = Mode::from_value(parameters.value(&parameter_name(ch, Control::Mode)));
        let set_mode = |mode: Mode| {
            Message::Dialog(DialogMessage::ToggleMode {
                node: id,
                channel: ch,
                mode,
            })
        };
        // A press on a control focuses it; sliders leave the arrow keys to the focused control.
        let target = |control: Element<'a, Message>, focus: MixerFocus| {
            ringed(
                focus_target(
                    control,
                    Message::Dialog(DialogMessage::SetFocus(Some(DialogFocus::Mixer(focus)))),
                    false,
                ),
                self.mixer_focus() == Some(focus),
            )
        };
        let gain_part: Element<'a, Message> = match parameters.info(&gain_name) {
            Some(info) => column![
                label(format!("{:+}{}", info.max, info.unit)),
                target(
                    tipped(
                        fader(parameters, info, GAIN_HEIGHT, false),
                        format_value(info, parameters.value(&gain_name)),
                    ),
                    MixerFocus::Gain(ch),
                ),
                label(format!("{:+}{}", info.min, info.unit)),
            ]
            .spacing(LABEL_SPACING)
            .align_x(Alignment::Center)
            .into(),
            None => column![].into(),
        };
        // Pan is drawn reversed, left at the top (ADR-0089).
        let pan_part: Element<'a, Message> = match parameters.info(&pan_name) {
            Some(info) => column![
                label("L".to_owned()),
                target(
                    tipped(
                        fader(parameters, info, PAN_HEIGHT, true),
                        pan_label(parameters.value(&pan_name)),
                    ),
                    MixerFocus::Pan(ch),
                ),
                label("R".to_owned()),
            ]
            .spacing(LABEL_SPACING)
            .align_x(Alignment::Center)
            .into(),
            None => column![].into(),
        };
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
    fn test_order_goes_down_each_strip_then_across() {
        use MixerFocus::{Channels, Gain, Mute, Pan, Solo};
        let order: Vec<MixerFocus> = MixerFocus::order(2).collect();
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
            ]
        );
        assert_eq!(MixerFocus::order(0).collect::<Vec<_>>(), [Channels]);
    }

    #[test]
    fn test_toggled_sets_or_clears_mode() {
        assert_eq!(toggled(Mode::Play, Mode::Mute), Mode::Mute);
        assert_eq!(toggled(Mode::Mute, Mode::Mute), Mode::Play);
        assert_eq!(toggled(Mode::Mute, Mode::Solo), Mode::Solo);
    }
}
