//! Drawing the window: the top bar; the track pane, divider, port gutter and patch board
//! scrolling together; the bottom bar; and any open menu floated over them
//! (`docs/architecture/user-interface.md` "Layout").

use audio_buffers::Layout;
use audio_effects::{Control, MAX_GAIN_DB, Mode, SILENCE_DB};
use iced::widget::{
    Space, button, canvas, column, container, mouse_area, opaque, pin, responsive, row, scrollable,
    slider, stack, text, text_input, vertical_slider,
};
use iced::{Alignment, Element, Fill, Length, Size};
use project_model::{Endpoint, NodeKind, TrackId};

use crate::app::{App, Message, NewNode};
use crate::menu::{Entry, MENU_WIDTH, Menu, burger, dropdown, gap};
use crate::patch_board::{BoardView, Divider, GUTTER_WIDTH, GutterView, Selection};
use crate::theme::{self, Tone};
use crate::transport::PositionBar;
use crate::waveform::{Display, LaneView};

/// The height of one channel's lane, in logical pixels.
pub(crate) const LANE_HEIGHT: f32 = 128.0;
/// The height of a track's header.
pub(crate) const HEADER_HEIGHT: f32 = 26.0;
/// The height of the top and bottom bars.
const BAR_HEIGHT: f32 = 56.0;
/// The divider's width.
const DIVIDER_WIDTH: f32 = 6.0;
/// The space below the tracks holding the "+" button.
const ADD_AREA: f32 = 76.0;
/// The size of the "+" buttons.
const ADD_BUTTON: f32 = 44.0;
/// The gap between the window's main parts.
const SPACING: f32 = 2.0;
/// The id of the track name being edited, to focus it.
pub(crate) const RENAME_INPUT: &str = "rename-track";

/// A button in a bar: `label`, sending `message` if there is one.
fn tool<'a>(label: &'a str, message: Option<Message>, selected: bool) -> Element<'a, Message> {
    button(text(label).size(13).center())
        .on_press_maybe(message)
        .padding([8, 12])
        .style(theme::tool(selected))
        .into()
}

/// Buttons side by side, as one group.
fn group<'a>(buttons: Vec<Element<'a, Message>>) -> Element<'a, Message> {
    row(buttons).spacing(2).into()
}

impl App {
    /// The whole window.
    pub fn view(&self) -> Element<'_, Message> {
        responsive(move |size| self.window(size)).into()
    }

    /// The track pane's width in a window `width` wide.
    fn track_pane_width(&self, width: f32) -> f32 {
        (width * self.split() - DIVIDER_WIDTH / 2.0).max(0.0)
    }

    fn window(&self, size: Size) -> Element<'_, Message> {
        let base = column![self.top_bar(), self.body(size), self.bottom_bar()]
            .spacing(SPACING)
            .height(Fill);
        let base = container(base)
            .width(Fill)
            .height(Fill)
            .style(|t: &iced::Theme| container::Style {
                background: Some(theme::tokens(theme::tone_of(t)).line.into()),
                ..container::Style::default()
            });
        match self.menu {
            None => base.into(),
            Some(menu) => {
                let (x, y) = self.menu_position(menu, size);
                stack![
                    base,
                    mouse_area(Space::new().width(Fill).height(Fill)).on_press(Message::CloseMenu),
                    pin(self.menu_entries(menu)).x(x).y(y),
                ]
                .into()
            }
        }
    }

    fn top_bar(&self) -> Element<'_, Message> {
        let playing = self.engine().is_playing();
        let display = self.display();
        let logo = container(text("D").size(26))
            .padding([2, 12])
            .style(|t: &iced::Theme| {
                let k = theme::tokens(theme::tone_of(t));
                container::Style {
                    background: Some(k.face.into()),
                    text_color: Some(k.face_text),
                    border: iced::Border {
                        color: k.line,
                        width: 1.0,
                        radius: 6.0.into(),
                    },
                    ..container::Style::default()
                }
            });
        let bar = row![
            logo,
            gap(16.0),
            group(vec![
                tool(
                    "Waveform",
                    Some(Message::SetDisplay(Display::Waveform)),
                    display == Display::Waveform
                ),
                tool(
                    "Spectrogram",
                    Some(Message::SetDisplay(Display::Spectrogram)),
                    display == Display::Spectrogram
                ),
            ]),
            gap(16.0),
            group(vec![
                tool(
                    "Undo",
                    (!playing && self.session().can_undo()).then_some(Message::Undo),
                    false
                ),
                tool(
                    "Redo",
                    (!playing && self.session().can_redo()).then_some(Message::Redo),
                    false
                ),
            ]),
            gap(16.0),
            group(vec![
                tool("Cut", None, false),
                tool("Copy", None, false),
                tool("Paste", None, false),
            ]),
            gap(16.0),
            group(vec![
                tool("−", Some(Message::ZoomOut), false),
                tool("+", Some(Message::ZoomIn), false),
            ]),
            Space::new().width(Fill),
            text(self.project_name.clone()).size(24),
            gap(16.0),
            tool(
                match self.tone() {
                    Tone::Light => "Dark",
                    Tone::Dark => "Light",
                },
                Some(Message::ToggleTone),
                false
            ),
            burger(Menu::App, 26.0),
        ]
        .spacing(4)
        .align_y(Alignment::Center);
        container(bar)
            .padding([0, 12])
            .height(Length::Fixed(BAR_HEIGHT))
            .width(Fill)
            .center_y(Length::Fixed(BAR_HEIGHT))
            .style(theme::chrome)
            .into()
    }

    fn bottom_bar(&self) -> Element<'_, Message> {
        let engine = self.engine();
        let playing = engine.is_playing();
        let mono = engine.mono();
        let seconds = engine.playhead() as f64 / f64::from(self.sample_rate());
        let position = canvas(PositionBar {
            playhead: engine.playhead(),
            start: self.view_start(),
            span: self.view_span(),
            length: self.timeline_length(),
        })
        .width(Fill)
        .height(Length::Fixed(24.0));
        let mut bar = row![
            group(vec![
                tool("Mono", Some(Message::SetMono(true)), mono),
                tool("Stereo", Some(Message::SetMono(false)), !mono),
            ]),
            gap(16.0),
            group(vec![
                if playing {
                    tool("Pause", Some(Message::Pause), false)
                } else {
                    tool("Play", Some(Message::Play), false)
                },
                tool("Stop", Some(Message::Stop), false),
                tool("Record", None, false),
            ]),
            gap(16.0),
            text(format!("{seconds:>8.2} s")).size(14),
            gap(12.0),
            position,
        ]
        .spacing(4)
        .align_y(Alignment::Center);
        if let Some(status) = self.status() {
            bar = bar.push(gap(12.0)).push(text(status.to_owned()).size(13));
        }
        container(bar)
            .padding([0, 12])
            .width(Fill)
            .center_y(Length::Fixed(BAR_HEIGHT))
            .style(theme::chrome)
            .into()
    }

    /// The track pane, divider, gutter and board, scrolling together under one scrollbar.
    fn body(&self, size: Size) -> Element<'_, Message> {
        let viewport = (size.height - 2.0 * BAR_HEIGHT - 2.0 * SPACING).max(0.0);
        let tracks_width = self.track_pane_width(size.width);
        let board_width = (size.width - tracks_width - DIVIDER_WIDTH - GUTTER_WIDTH).max(0.0);
        let board = self.board(Size::new(board_width, viewport));
        let height = viewport
            .max(self.tracks_height() + ADD_AREA)
            .max(board.size.height);
        let gutter: Vec<(crate::patch_board::Pin, f32)> = board
            .pins
            .iter()
            .filter(|(p, _)| matches!(p.endpoint, Endpoint::Track(_)))
            .map(|(p, at)| (*p, at.y))
            .collect();
        let board_size = board.size;
        let content = row![
            container(self.track_pane())
                .width(Length::Fixed(tracks_width))
                .height(Length::Fixed(height))
                .style(theme::pane),
            canvas(Divider {
                window_width: size.width
            })
            .width(Length::Fixed(DIVIDER_WIDTH))
            .height(Length::Fixed(height)),
            canvas(GutterView(gutter))
                .width(Length::Fixed(GUTTER_WIDTH))
                .height(Length::Fixed(height)),
            scrollable(
                canvas(BoardView {
                    board,
                    selection: self.selection,
                    wiring: self.wiring,
                })
                .width(Length::Fixed(board_size.width))
                .height(Length::Fixed(height)),
            )
            .direction(scrollable::Direction::Horizontal(
                scrollable::Scrollbar::default(),
            ))
            .width(Fill),
        ];
        let scrolling = scrollable(content)
            .on_scroll(|viewport| Message::Scrolled(viewport.absolute_offset().y))
            .width(Fill)
            .height(Fill);
        // The node menu's "+" stays at the board's top right, clear of the scrollbar.
        let add = button(text("+").size(24).center())
            .on_press(Message::OpenMenu(Menu::AddNode))
            .width(Length::Fixed(ADD_BUTTON))
            .height(Length::Fixed(ADD_BUTTON))
            .style(theme::tool(false));
        let mut layers = stack![
            scrolling,
            pin(add).x(size.width - ADD_BUTTON - 24.0).y(12.0)
        ];
        if let Some(dialog) = self.node_dialog((tracks_width - 32.0).clamp(0.0, 560.0)) {
            layers = layers.push(pin(opaque(dialog)).x(16.0).y(16.0));
        }
        layers.into()
    }

    /// The selected node's dialog, `width` wide, over the track pane.
    fn node_dialog(&self, width: f32) -> Option<Element<'_, Message>> {
        let Some(Selection::Node(id)) = self.selection() else {
            return None;
        };
        let node = self.session().state().node(id)?;
        let header = row![
            text(crate::patch_board::title(&node.kind)).size(18),
            Space::new().width(Fill),
            tool("Delete node", Some(Message::DeleteNode(id)), false),
            tool("×", Some(Message::Select(None)), false),
        ]
        .spacing(6)
        .align_y(Alignment::Center);
        let body: Element<'_, Message> = match &node.kind {
            NodeKind::Mixer { channels, .. } => {
                let strips =
                    row((0..*channels).map(|ch| self.strip(id, ch, self.channel_name(id, ch))))
                        .spacing(12);
                column![
                    scrollable(strips).direction(scrollable::Direction::Horizontal(
                        scrollable::Scrollbar::default(),
                    )),
                    row![
                        tool("Add channel", Some(Message::AddMixerChannel(id)), false),
                        tool(
                            "Remove channel",
                            self.removable_channel(id)
                                .map(|_| Message::RemoveMixerChannel(id)),
                            false
                        ),
                    ]
                    .spacing(6),
                ]
                .spacing(10)
                .into()
            }
            NodeKind::FeedbackDelay { seconds, .. } => {
                let typed = match &self.delay_text {
                    Some((node, typed)) if *node == id => typed.clone(),
                    _ => format!("{seconds}"),
                };
                row![
                    text("Time (seconds)").size(14),
                    text_input("0.25", &typed)
                        .on_input(move |t| Message::DelayText(id, t))
                        .on_submit(Message::CommitDelay)
                        .width(Length::Fixed(100.0)),
                ]
                .spacing(10)
                .align_y(Alignment::Center)
                .into()
            }
            NodeKind::Bus(_) | NodeKind::Effect { .. } => Space::new().into(),
        };
        Some(
            container(column![header, body].spacing(12))
                .padding(12)
                .width(Length::Fixed(width))
                .style(theme::popup)
                .into(),
        )
    }

    fn track_pane(&self) -> Element<'_, Message> {
        let cursor = self.playhead_fraction();
        let (start, span) = (self.view_start(), self.view_span());
        let mut tracks = column![];
        for (id, lanes) in &self.lanes {
            let Some(track) = self.session().state().track(*id) else {
                continue;
            };
            tracks = tracks.push(self.track_header(*id, &track.name));
            for lane in lanes {
                tracks = tracks.push(
                    canvas(LaneView {
                        lane,
                        generation: self.generation,
                        cursor,
                        start,
                        span,
                    })
                    .width(Fill)
                    .height(Length::Fixed(LANE_HEIGHT)),
                );
            }
        }
        let add = container(
            button(text("+").size(24).center())
                .on_press(Message::OpenMenu(Menu::AddTrack))
                .width(Length::Fixed(44.0))
                .height(Length::Fixed(44.0))
                .style(theme::tool(false)),
        )
        .padding(16);
        column![tracks, add].width(Fill).into()
    }

    fn track_header<'a>(&'a self, id: TrackId, name: &'a str) -> Element<'a, Message> {
        let label: Element<'a, Message> = match &self.renaming {
            Some((renaming, typed)) if *renaming == id => text_input("Track name", typed)
                .id(RENAME_INPUT)
                .on_input(Message::RenameText)
                .on_submit(Message::FinishRename)
                .size(14)
                .padding([1, 4])
                .width(Length::Fixed(260.0))
                .into(),
            _ => text(name.to_owned()).size(15).into(),
        };
        container(
            row![
                label,
                Space::new().width(Fill),
                burger(Menu::Track(id), 18.0)
            ]
            .align_y(Alignment::Center),
        )
        .padding([0, 6])
        .width(Fill)
        .center_y(Length::Fixed(HEADER_HEIGHT))
        .style(theme::chrome)
        .into()
    }

    /// Where `menu` opens, in the window, for a window of `size`.
    fn menu_position(&self, menu: Menu, size: Size) -> (f32, f32) {
        let track_pane = self.track_pane_width(size.width);
        let pane_top = BAR_HEIGHT + SPACING - self.scrolled;
        let (x, y) = match menu {
            Menu::App => (size.width - MENU_WIDTH - 8.0, BAR_HEIGHT),
            Menu::Track(id) => {
                let above: f32 = self
                    .lanes
                    .iter()
                    .take_while(|(t, _)| *t != id)
                    .map(|(_, lanes)| HEADER_HEIGHT + LANE_HEIGHT * lanes.len() as f32)
                    .sum();
                (
                    track_pane - MENU_WIDTH - 8.0,
                    pane_top + above + HEADER_HEIGHT,
                )
            }
            Menu::AddTrack => (16.0, pane_top + self.tracks_height() + 16.0 + 46.0),
            Menu::AddNode => (
                size.width - MENU_WIDTH - 24.0,
                BAR_HEIGHT + SPACING + 12.0 + ADD_BUTTON + 4.0,
            ),
        };
        (
            x.clamp(0.0, (size.width - MENU_WIDTH).max(0.0)),
            y.clamp(0.0, (size.height - 120.0).max(0.0)),
        )
    }

    /// The height of every track's header and lanes.
    fn tracks_height(&self) -> f32 {
        self.lanes
            .iter()
            .map(|(_, lanes)| HEADER_HEIGHT + LANE_HEIGHT * lanes.len() as f32)
            .sum()
    }

    fn menu_entries(&self, menu: Menu) -> Element<'_, Message> {
        let stopped = !self.engine().is_playing();
        let when_stopped = |message: Message| stopped.then_some(message);
        let entries = match menu {
            Menu::App => {
                let (choices, current) = &self.devices;
                let mut entries = vec![
                    Entry::item("New project", None),
                    Entry::item("Open…", None),
                    Entry::item("Save", Some(Message::Save)),
                    Entry::Separator,
                    Entry::item("Import…", when_stopped(Message::Import)),
                    Entry::item("Export…", Some(Message::Export(false))),
                    Entry::item("Export mono…", Some(Message::Export(true))),
                    Entry::Separator,
                    Entry::Heading("Audio device".to_owned()),
                ];
                entries.extend(choices.iter().map(|choice| {
                    let mark = if Some(&choice.id) == current.as_ref() {
                        "✓ "
                    } else {
                        "    "
                    };
                    Entry::item(
                        format!("{mark}{}", choice.name),
                        when_stopped(Message::Device(choice.clone())),
                    )
                }));
                entries.extend([Entry::Separator, Entry::item("Quit", Some(Message::Quit))]);
                entries
            }
            Menu::Track(id) => vec![
                Entry::item("Rename", Some(Message::StartRename(id))),
                Entry::item("Import audio into…", when_stopped(Message::ImportInto(id))),
                Entry::item("Delete", when_stopped(Message::DeleteTrack(id))),
            ],
            Menu::AddTrack => vec![
                Entry::item("Mono track", when_stopped(Message::AddTrack(Layout::Mono))),
                Entry::item(
                    "Stereo track",
                    when_stopped(Message::AddTrack(Layout::Stereo)),
                ),
                Entry::item("Automation curve", None),
            ],
            Menu::AddNode => vec![
                Entry::item("Mixer", Some(Message::AddNode(NewNode::Mixer))),
                Entry::item("Bus", Some(Message::AddNode(NewNode::Bus))),
                Entry::item(
                    "Feedback delay",
                    Some(Message::AddNode(NewNode::FeedbackDelay)),
                ),
            ],
        };
        dropdown(entries)
    }

    /// A mixer channel's name: the track connected to it, if any, else its number.
    fn channel_name(&self, node: project_model::NodeId, channel: usize) -> String {
        let state = self.session().state();
        state
            .connections
            .iter()
            .find(|c| c.to == Endpoint::Node(node) && c.input == channel)
            .and_then(|c| match c.from {
                Endpoint::Track(id) => state.track(id).map(|t| match c.from_side {
                    Some(project_model::Side::Left) => format!("{} L", t.name),
                    Some(project_model::Side::Right) => format!("{} R", t.name),
                    None => t.name.clone(),
                }),
                _ => None,
            })
            .unwrap_or_else(|| format!("{}", channel + 1))
    }

    /// One mixer channel's gain, pan, mute and solo.
    fn strip(&self, id: project_model::NodeId, ch: usize, name: String) -> Element<'_, Message> {
        let gain = self.mixer_value(id, ch, Control::Gain);
        let pan = self.mixer_value(id, ch, Control::Pan);
        let mode = Mode::from_value(self.mixer_value(id, ch, Control::Mode));
        let toggle = |target: Mode| if mode == target { Mode::Play } else { target };
        column![
            text(name).size(13),
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
            .height(Length::Fixed(120.0)),
            text(format!("{gain:.1} dB")).size(12),
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
                tool(
                    "M",
                    Some(Message::SetMode {
                        node: id,
                        channel: ch,
                        mode: toggle(Mode::Mute)
                    }),
                    mode == Mode::Mute
                ),
                tool(
                    "S",
                    Some(Message::SetMode {
                        node: id,
                        channel: ch,
                        mode: toggle(Mode::Solo)
                    }),
                    mode == Mode::Solo
                ),
            ]
            .spacing(4),
        ]
        .spacing(6)
        .width(Length::Fixed(90.0))
        .into()
    }
}
