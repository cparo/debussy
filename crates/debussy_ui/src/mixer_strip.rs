//! A mixer's dialog body: a "Channels" field above narrow numbered strips, each with vertical
//! gain and pan sliders, their values as hover tips, and mute above solo
//! (`docs/architecture/user-interface.md` "Layout").

use audio_effects::{Control, MAX_GAIN_DB, Mode, SILENCE_DB, parameter_name};
use iced::widget::canvas::{Frame, Geometry, Path, Program, Stroke};
use iced::widget::{
    canvas as canvas_widget, column, container, row, scrollable, stack, text, text_input, tooltip,
    vertical_slider,
};
use iced::{Alignment, Color, Element, Length, Point, Rectangle, Renderer, Theme, mouse};
use project_model::NodeId;

use crate::app::{App, Message};
use crate::theme::{self, tokens, tone_of};
use crate::view::{Face, tool};

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
/// The size of hover tips' text.
const TIP_SIZE: f32 = 13.0;
/// The channel-count field's width.
const CHANNELS_WIDTH: f32 = 60.0;

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

impl App {
    /// Mixer `id`'s controls: a "Channels" field above its `channels` strips, which scroll
    /// horizontally.
    pub(crate) fn mixer_panel(&self, id: NodeId, channels: usize) -> Element<'_, Message> {
        let typed = match &self.node_text {
            Some((node, typed)) if *node == id => typed.clone(),
            _ => format!("{channels}"),
        };
        let field = row![
            text("Channels").size(14),
            text_input("1", &typed)
                .on_input(move |t| Message::NodeText(id, t))
                .on_submit(Message::CommitNodeText)
                .width(Length::Fixed(CHANNELS_WIDTH)),
        ]
        .spacing(10)
        .align_y(Alignment::Center);
        let strips = row((0..channels).map(|ch| self.strip(id, ch))).spacing(4);
        column![
            field,
            scrollable(strips).direction(scrollable::Direction::Horizontal(
                scrollable::Scrollbar::default(),
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
        let set_mode = |target: Mode| Message::SetParameter {
            node: id,
            name: mode_name.clone(),
            value: if mode == target { Mode::Play } else { target }.value(),
        };
        let unity = (0.0 - SILENCE_DB) / (MAX_GAIN_DB - SILENCE_DB);
        let gain_slider = stack![
            canvas_widget(Tic { at: unity })
                .width(Length::Fixed(SLIDER_WIDTH))
                .height(Length::Fixed(GAIN_HEIGHT)),
            vertical_slider(SILENCE_DB..=MAX_GAIN_DB, gain, move |value| {
                Message::Slide {
                    node: id,
                    name: gain_name.clone(),
                    value,
                }
            })
            .step(0.1_f32)
            .on_release(Message::ReleaseSnapped { step: 1.0 })
            .width(SLIDER_WIDTH)
            .height(Length::Fixed(GAIN_HEIGHT)),
        ];
        // The slider runs from right (bottom) to left (top), so it shows the negated pan.
        let pan_slider = stack![
            canvas_widget(Tic { at: 0.5 })
                .width(Length::Fixed(SLIDER_WIDTH))
                .height(Length::Fixed(PAN_HEIGHT)),
            vertical_slider(-1.0..=1.0, -pan, move |value: f32| Message::Slide {
                node: id,
                name: pan_name.clone(),
                value: -value,
            })
            .step(0.01_f32)
            .on_release(Message::Release)
            .width(SLIDER_WIDTH)
            .height(Length::Fixed(PAN_HEIGHT)),
        ];
        column![
            text(ch + 1).size(13),
            label(format!("{MAX_GAIN_DB:+}dB")),
            tipped(gain_slider.into(), format!("{gain:.1} dB")),
            label(format!("{SILENCE_DB:+}dB")),
            label("L".to_owned()),
            tipped(pan_slider.into(), pan_label(pan)),
            label("R".to_owned()),
            column![
                tool(
                    Face::Label("M"),
                    Some(set_mode(Mode::Mute)),
                    mode == Mode::Mute
                ),
                tool(
                    Face::Label("S"),
                    Some(set_mode(Mode::Solo)),
                    mode == Mode::Solo
                ),
            ]
            .spacing(4)
            .align_x(Alignment::Center),
        ]
        .spacing(4)
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
}
