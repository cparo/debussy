//! Drawing the interface: the transport bar, the track lanes and the mixer
//! (`docs/architecture/user-interface.md`).

use audio_effects::{Control, MAX_GAIN_DB, Mode, SILENCE_DB};
use iced::widget::{
    Space, button, column, container, row, scrollable, slider, text, vertical_slider,
};
use iced::{Element, Fill, Length};
use project_model::{Endpoint, NodeKind};

use crate::app::{App, Message};
use crate::waveform::{LaneView, canvas};

/// The height of a track lane, in logical pixels.
const LANE_HEIGHT: f32 = 64.0;

impl App {
    /// The whole window.
    pub fn view(&self) -> Element<'_, Message> {
        let mut body = column![self.transport(), self.tracks(), self.mixer()]
            .spacing(12)
            .padding(12);
        if let Some(status) = self.status() {
            body = body.push(text(status.to_owned()));
        }
        container(body).width(Fill).height(Fill).into()
    }

    fn transport(&self) -> Element<'_, Message> {
        let playing = self.engine().is_playing();
        let seconds = self.engine().playhead() as f64 / f64::from(self.sample_rate());
        let mono = if self.engine().mono() {
            "Mono ✓"
        } else {
            "Mono"
        };
        row![
            button("⏮").on_press(Message::Rewind),
            if playing {
                button("⏸").on_press(Message::Pause)
            } else {
                button("▶").on_press(Message::Play)
            },
            button("⏹").on_press(Message::Stop),
            text(format!("{:>8.2} s", seconds)),
            Space::new().width(Length::Fixed(24.0)),
            button(mono).on_press(Message::ToggleMono),
            button("−").on_press(Message::ZoomOut),
            button("+").on_press(Message::ZoomIn),
            button("◀").on_press(Message::Scroll(-(crate::waveform::LANE_COLUMNS as i64) / 4)),
            button("▶▶").on_press(Message::Scroll(crate::waveform::LANE_COLUMNS as i64 / 4)),
            Space::new().width(Length::Fixed(24.0)),
            button("Undo")
                .on_press_maybe((!playing && self.session().can_undo()).then_some(Message::Undo)),
            button("Redo")
                .on_press_maybe((!playing && self.session().can_redo()).then_some(Message::Redo)),
            button("Save").on_press(Message::Save),
        ]
        .spacing(8)
        .into()
    }

    fn tracks(&self) -> Element<'_, Message> {
        let playhead = self.playhead_fraction();
        let rows = self.lanes.iter().filter_map(|(id, lane)| {
            let track = self.session().state().track(*id)?;
            let lane = canvas(LaneView { lane, playhead })
                .width(Fill)
                .height(Length::Fixed(LANE_HEIGHT));
            Some(
                row![text(track.name.clone()).width(Length::Fixed(140.0)), lane]
                    .spacing(8)
                    .into(),
            )
        });
        scrollable(column(rows).spacing(4)).height(Fill).into()
    }

    fn mixer(&self) -> Element<'_, Message> {
        let state = self.session().state();
        let mut strips = row![].spacing(12);
        for node in &state.nodes {
            let NodeKind::Mixer { channels, .. } = &node.kind else {
                continue;
            };
            for channel in 0..*channels {
                // Named after the track connected to the channel, if any.
                let name = state
                    .connections
                    .iter()
                    .find(|c| c.to == Endpoint::Node(node.id) && c.input == channel)
                    .and_then(|c| match c.from {
                        Endpoint::Track(id) => state.track(id).map(|t| t.name.clone()),
                        _ => None,
                    })
                    .unwrap_or_else(|| format!("{}", channel + 1));
                let (id, ch) = (node.id, channel);
                let gain = self.mixer_value(id, ch, Control::Gain);
                let pan = self.mixer_value(id, ch, Control::Pan);
                let mode = Mode::from_value(self.mixer_value(id, ch, Control::Mode));
                let toggle = |target: Mode| if mode == target { Mode::Play } else { target };
                strips = strips.push(
                    column![
                        text(name),
                        vertical_slider(SILENCE_DB..=MAX_GAIN_DB, gain, move |value| {
                            Message::Slide {
                                node: id,
                                channel: ch,
                                control: Control::Gain,
                                value,
                            }
                        })
                        .step(0.1_f32)
                        .on_release(Message::Release)
                        .height(Length::Fixed(140.0)),
                        text(format!("{gain:.1} dB")),
                        slider(-1.0..=1.0, pan, move |value| Message::Slide {
                            node: id,
                            channel: ch,
                            control: Control::Pan,
                            value,
                        })
                        .step(0.01_f32)
                        .on_release(Message::Release)
                        .width(Length::Fixed(80.0)),
                        row![
                            button(if mode == Mode::Mute { "M ✓" } else { "M" }).on_press(
                                Message::SetMode {
                                    node: id,
                                    channel: ch,
                                    mode: toggle(Mode::Mute)
                                }
                            ),
                            button(if mode == Mode::Solo { "S ✓" } else { "S" }).on_press(
                                Message::SetMode {
                                    node: id,
                                    channel: ch,
                                    mode: toggle(Mode::Solo)
                                }
                            ),
                        ]
                        .spacing(4),
                    ]
                    .spacing(6)
                    .width(Length::Fixed(90.0)),
                );
            }
        }
        scrollable(strips)
            .direction(iced::widget::scrollable::Direction::Horizontal(
                iced::widget::scrollable::Scrollbar::default(),
            ))
            .into()
    }
}
