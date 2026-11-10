//! Drawing the window: the top bar; the track pane, divider, port gutter and patch board
//! scrolling together; the bottom bar; and any open menu floated over them
//! (`docs/architecture/user-interface.md` "Layout").

use std::sync::LazyLock;

use audio_buffers::Layout;
use iced::widget::{
    Space, button, canvas, column, container, mouse_area, opaque, pick_list, pin, progress_bar,
    responsive, row, scrollable, slider, stack, svg, text, text_input, tooltip,
};
use iced::{Alignment, Element, Fill, Length, Size};
use project_model::{Endpoint, NodeKind, Side, TrackId};

use crate::app::{App, Message, NewNode};
use crate::click::{COUNT_IN_BARS, format_bpm};
use crate::icon::Icon;
use crate::menu::{Entry, MENU_WIDTH, Menu, burger, dropdown, gap};
use crate::patch_board::{
    BoardView, Divider, GUTTER_WIDTH, GutterView, PIN_RADIUS, Selection, gutter_pin_x,
};
use crate::recording::METER_FLOOR_DB;
use crate::services::{MAX_CLICK_LEVEL_DB, MIN_CLICK_LEVEL_DB};
use crate::theme::{self, Accent, Place, Tone};
use crate::transport::PositionBar;
use crate::waveform::{CursorView, Display, GridView, LaneView};

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
/// The logo's height in the top bar; its width follows from the artwork.
const LOGO_HEIGHT: f32 = 44.0;
/// The logo artwork (`docs/architecture/user-interface.md` "Icons"), parsed once.
static LOGO: LazyLock<svg::Handle> = LazyLock::new(|| {
    svg::Handle::from_memory(include_bytes!("../assets/icons/debussy.svg").as_slice())
});
/// The padding inside a node's dialog.
const DIALOG_PADDING: f32 = 12.0;
/// The id of the track name being edited, to focus it.
pub(crate) const RENAME_INPUT: &str = "rename-track";

/// The size of a toolbar button's icon, and of the project name's text, in logical pixels.
const ICON_SIZE: f32 = 16.0;
/// A toolbar button's text size.
const TOOL_TEXT: f32 = 13.0;
/// The width of the playhead time beside the position bar: room for "00:00:00.000".
const TIME_WIDTH: f32 = 100.0;
/// The tempo field's width in the bottom bar.
const TEMPO_WIDTH: f32 = 64.0;
/// The meter pickers' width each.
const METER_WIDTH: f32 = 56.0;
/// Where the click dialog opens from the window's left edge: about over the click button.
const CLICK_DIALOG_X: f32 = 370.0;
/// The click dialog's width.
const CLICK_DIALOG_WIDTH: f32 = 290.0;
/// The click dialog's height, to open it above the bottom bar.
const CLICK_DIALOG_HEIGHT: f32 = 390.0;
/// The delete confirmation's width.
const CONFIRM_WIDTH: f32 = 280.0;
/// The height of one item in a dropdown menu, to open the device menu above the bottom bar.
const MENU_ITEM_HEIGHT: f32 = 27.0;

/// What a toolbar button shows: a text label or an icon.
#[derive(Clone, Copy)]
pub(crate) enum Face<'a> {
    /// A text label.
    Label(&'a str),
    /// An icon, named in a tooltip.
    Icon(Icon),
}

/// A button in a bar showing `face`, sending `message` if there is one.
pub(crate) fn tool<'a>(
    face: Face<'a>,
    message: Option<Message>,
    selected: bool,
) -> Element<'a, Message> {
    button_at(face, message, selected, Place::Alone)
}

/// A toolbar button styled for `place`.
fn button_at<'a>(
    face: Face<'a>,
    message: Option<Message>,
    selected: bool,
    place: Place,
) -> Element<'a, Message> {
    let enabled = message.is_some();
    // Record is lit red, its dot red until it is (`docs/architecture/user-interface.md` "Icons").
    let accent = match face {
        Face::Icon(Icon::Record) => Accent::Record,
        _ => Accent::Audio,
    };
    let content: Element<'a, Message> = match face {
        Face::Label(label) => text(label).size(TOOL_TEXT).center().into(),
        Face::Icon(Icon::Record) => svg(Icon::Record.handle())
            .width(ICON_SIZE)
            .height(ICON_SIZE)
            .style(theme::record_icon(selected, enabled))
            .into(),
        Face::Icon(icon) => svg(icon.handle())
            .width(ICON_SIZE)
            .height(ICON_SIZE)
            .style(theme::tool_icon(selected, enabled))
            .into(),
    };
    // An icon is padded to the height of a text label, so mixed strips line up.
    let vertical = match face {
        Face::Label(_) => 8.0,
        Face::Icon(_) => 8.0 + (TOOL_TEXT * 1.3 - ICON_SIZE) / 2.0,
    };
    let face_button = button(content)
        .on_press_maybe(message)
        .padding([vertical, 12.0])
        .style(theme::accent_style(
            selected,
            place,
            matches!(face, Face::Label(_)),
            accent,
        ));
    match face {
        Face::Label(_) => face_button.into(),
        Face::Icon(icon) => tooltip(
            face_button,
            container(text(icon.name()).size(TOOL_TEXT))
                .padding([4, 8])
                .style(theme::popup),
            tooltip::Position::FollowCursor,
        )
        .into(),
    }
}

/// A small toggle for a track header, labelled `label`, lit with `accent` when `selected`.
fn chip<'a>(
    label: &'a str,
    message: Option<Message>,
    selected: bool,
    accent: Accent,
) -> Element<'a, Message> {
    button(text(label).size(12).center())
        .on_press_maybe(message)
        .padding([1, 6])
        .style(theme::accent_style(selected, Place::Alone, true, accent))
        .into()
}

/// An armed track's input meter: one bar per channel, left above right, from
/// [`METER_FLOOR_DB`] to 0 dBFS, red from −1 dBFS.
fn input_meter<'a>(levels: &[f32]) -> Element<'a, Message> {
    let girth = if levels.len() > 1 { 4.0 } else { 8.0 };
    let bars = levels.iter().map(|&level| {
        let clipping = level >= -1.0;
        progress_bar(METER_FLOOR_DB..=0.0, level)
            .length(Length::Fixed(64.0))
            .girth(Length::Fixed(girth))
            .style(move |theme: &iced::Theme| {
                if clipping {
                    iced::widget::progress_bar::danger(theme)
                } else {
                    iced::widget::progress_bar::success(theme)
                }
            })
            .into()
    });
    column(bars).spacing(1).into()
}

/// Buttons side by side as one strip, each a `(face, message, selected)` as for [`tool`].
pub(crate) fn group<'a>(buttons: Vec<(Face<'a>, Option<Message>, bool)>) -> Element<'a, Message> {
    if let [(face, message, selected)] = buttons.as_slice() {
        return tool(*face, message.clone(), *selected);
    }
    let last = buttons.len().saturating_sub(1);
    let faces = buttons
        .into_iter()
        .enumerate()
        .map(|(i, (face, message, selected))| {
            let place = match i {
                0 => Place::First,
                i if i == last => Place::Last,
                _ => Place::Middle,
            };
            button_at(face, message, selected, place)
        });
    container(row(faces).spacing(1))
        .padding(1)
        .style(theme::strip)
        .into()
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
        let selected = self.selected_range().is_some();
        let display = self.display();
        let logo = svg(LOGO.clone())
            .width(Length::Shrink)
            .height(LOGO_HEIGHT)
            .style(theme::logo);
        let bar = row![
            logo,
            gap(16.0),
            group(vec![
                (
                    Face::Icon(Icon::Waveform),
                    Some(Message::SetDisplay(Display::Waveform)),
                    display == Display::Waveform
                ),
                (
                    Face::Icon(Icon::Spectrogram),
                    Some(Message::SetDisplay(Display::Spectrogram)),
                    display == Display::Spectrogram
                ),
            ]),
            gap(4.0),
            tool(
                Face::Icon(Icon::Grid),
                Some(Message::SetGrid(!self.grid())),
                self.grid()
            ),
            gap(16.0),
            group(vec![
                (
                    Face::Icon(Icon::Undo),
                    (!playing && self.session().can_undo()).then_some(Message::Undo),
                    false
                ),
                (
                    Face::Icon(Icon::Redo),
                    (!playing && self.session().can_redo()).then_some(Message::Redo),
                    false
                ),
            ]),
            gap(16.0),
            group(vec![
                (
                    Face::Icon(Icon::Cut),
                    (!playing && selected).then_some(Message::Cut),
                    false
                ),
                (
                    Face::Icon(Icon::Copy),
                    selected.then_some(Message::Copy),
                    false
                ),
                (
                    Face::Icon(Icon::Paste),
                    (!playing && self.clipboard().is_some() && self.paste_target().is_some())
                        .then_some(Message::Paste),
                    false
                ),
            ]),
            gap(16.0),
            group(vec![
                (
                    Face::Label("−"),
                    self.can_zoom_out().then_some(Message::ZoomOut),
                    false
                ),
                (Face::Label("+"), Some(Message::ZoomIn), false),
            ]),
            Space::new().width(Fill),
            text(self.project_name.clone()).size(ICON_SIZE),
            gap(16.0),
            self.tone_toggle(),
            burger(Menu::App, ICON_SIZE, 8.0),
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

    /// The light/dark toggle: the icon of the tone it switches to, in the accent, with no button
    /// face or outline.
    fn tone_toggle(&self) -> Element<'_, Message> {
        let icon = match self.tone() {
            Tone::Light => Icon::Dark,
            Tone::Dark => Icon::Light,
        };
        let face = button(
            svg(icon.handle())
                .width(ICON_SIZE)
                .height(ICON_SIZE)
                .style(theme::accent_icon),
        )
        .on_press(Message::ToggleTone)
        .padding(8)
        .style(theme::flat);
        tooltip(
            face,
            container(text(icon.name()).size(TOOL_TEXT))
                .padding([4, 8])
                .style(theme::popup),
            tooltip::Position::FollowCursor,
        )
        .into()
    }

    fn bottom_bar(&self) -> Element<'_, Message> {
        let engine = self.engine();
        let playing = engine.is_playing();
        let mono = engine.mono();
        let position = canvas(PositionBar {
            playhead: engine.playhead(),
            start: self.view_start(),
            span: self.view_span(),
            length: self.timeline_length(),
        })
        .width(Fill)
        .height(Length::Fixed(24.0));
        let mut bar = row![
            // Changing device is refused while playing, with a notice.
            tool(
                Face::Icon(Icon::Device),
                Some(Message::OpenMenu(Menu::Device)),
                self.menu == Some(Menu::Device)
            ),
            gap(16.0),
            group(vec![
                (Face::Label("Mono"), Some(Message::SetMono(true)), mono),
                (Face::Label("Stereo"), Some(Message::SetMono(false)), !mono),
            ]),
            gap(16.0),
            group(vec![
                if playing {
                    (Face::Icon(Icon::Pause), Some(Message::Pause), false)
                } else {
                    (Face::Icon(Icon::Play), Some(Message::Play), false)
                },
                (Face::Icon(Icon::Stop), Some(Message::Stop), false),
                (
                    Face::Icon(Icon::Record),
                    self.can_record().then_some(Message::Record),
                    self.is_recording()
                ),
            ]),
            gap(16.0),
            tool(
                Face::Icon(Icon::Click),
                Some(Message::OpenMenu(Menu::Click)),
                self.engine().click_on()
            ),
            gap(16.0),
            position,
            gap(12.0),
            // A fixed width, so the position bar beside it does not move as the digits change.
            text(self.playhead_time())
                .size(14)
                .width(Length::Fixed(TIME_WIDTH)),
        ]
        .spacing(4)
        .align_y(Alignment::Center);
        let status = if self.is_counting_in() {
            Some("Count-in")
        } else {
            self.status()
        };
        if let Some(status) = status {
            bar = bar.push(gap(12.0)).push(text(status.to_owned()).size(13));
        }
        container(bar)
            .padding([0, 12])
            .width(Fill)
            .center_y(Length::Fixed(BAR_HEIGHT))
            .style(theme::chrome)
            .into()
    }

    /// The click dialog: the click on or off, the tempo and meter, the sound and level, and the
    /// count-in before recording. Its controls leave it open; a click elsewhere closes it.
    fn click_dialog(&self) -> Element<'_, Message> {
        let playing = self.engine().is_playing();
        let click_on = self.engine().click_on();
        let tempo = self.session().state().tempo;
        let preferences = self.click.preferences;
        let tempo_text = self
            .click
            .tempo_text
            .clone()
            .unwrap_or_else(|| format_bpm(tempo.bpm));
        let beats: Vec<u8> = (1..=project_model::tempo::MAX_BEATS_PER_BAR).collect();
        let label = |s: String| text(s).size(TOOL_TEXT);
        let line = |name: &'static str, control: Element<'static, Message>| {
            row![label(name.to_owned()), Space::new().width(Fill), control]
                .spacing(6)
                .align_y(Alignment::Center)
        };
        let sounds = engine_bridge::ClickSound::ALL
            .into_iter()
            .map(|sound| {
                (
                    Face::Label(sound.name()),
                    Some(Message::SetClickSound(sound)),
                    sound == preferences.sound,
                )
            })
            .collect();
        let count_ins = COUNT_IN_BARS
            .into_iter()
            .map(|bars| {
                let name = match bars {
                    0 => "Off",
                    1 => "1 bar",
                    _ => "2 bars",
                };
                (
                    Face::Label(name),
                    Some(Message::SetCountIn(bars)),
                    bars == self.click.count_in_bars,
                )
            })
            .collect();
        // Changing the tempo or meter is refused while playing, with a notice.
        let tempo_row = row![
            text_input("BPM", &tempo_text)
                .on_input_maybe((!playing).then_some(Message::TempoText))
                .on_submit(Message::CommitTempo)
                .size(TOOL_TEXT)
                .width(Length::Fixed(TEMPO_WIDTH)),
            label("BPM".to_owned()),
        ]
        .spacing(6)
        .align_y(Alignment::Center);
        let meter_row = row![
            pick_list(beats, Some(tempo.beats_per_bar), Message::SetBeatsPerBar)
                .text_size(TOOL_TEXT)
                .width(Length::Fixed(METER_WIDTH)),
            label("/".to_owned()),
            pick_list(
                project_model::tempo::BEAT_UNITS.to_vec(),
                Some(tempo.beat_unit),
                Message::SetBeatUnit
            )
            .text_size(TOOL_TEXT)
            .width(Length::Fixed(METER_WIDTH)),
        ]
        .spacing(6)
        .align_y(Alignment::Center);
        let body = column![
            line(
                "Click",
                group(vec![
                    (
                        Face::Label("Off"),
                        Some(Message::SetClick(false)),
                        !click_on
                    ),
                    (Face::Label("On"), Some(Message::SetClick(true)), click_on),
                ])
            ),
            line("Tempo", tempo_row.into()),
            line("Meter", meter_row.into()),
            column![label("Sound".to_owned()), group(sounds)].spacing(6),
            column![
                label(format!("Level {:+.1} dB", preferences.level_db)),
                slider(
                    MIN_CLICK_LEVEL_DB..=MAX_CLICK_LEVEL_DB,
                    preferences.level_db,
                    Message::ClickLevel
                )
                .step(0.5_f32)
                .on_release(Message::ReleaseClickLevel),
            ]
            .spacing(6),
            column![
                label("Count-in before recording".to_owned()),
                group(count_ins)
            ]
            .spacing(6),
        ]
        .spacing(14);
        // Opaque, so a press on the dialog's background does not close it.
        opaque(
            container(body)
                .padding(12)
                .width(Length::Fixed(CLICK_DIALOG_WIDTH))
                .style(theme::popup),
        )
    }

    /// The device menu: the devices present, the one in use ticked.
    fn device_menu(&self) -> Vec<Entry> {
        let (choices, current) = &self.devices;
        if choices.is_empty() {
            return vec![Entry::item("No audio device", None)];
        }
        choices
            .iter()
            .map(|choice| {
                let tick = if Some(&choice.id) == current.as_ref() {
                    "✓ "
                } else {
                    "    "
                };
                Entry::item(
                    format!("{tick}{}", choice.name),
                    Some(Message::Device(choice.clone())),
                )
            })
            .collect()
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
        // Each gutter pin's name shows on hover; the tip's anchor captures nothing, so a press
        // still reaches the gutter and starts a wire.
        let mut gutter_layers = stack![
            canvas(GutterView(gutter.clone()))
                .width(Length::Fixed(GUTTER_WIDTH))
                .height(Length::Fixed(height))
        ];
        for (p, y) in &gutter {
            let Endpoint::Track(track) = p.endpoint else {
                continue;
            };
            let Some(name) = self.port_name(track, p.side) else {
                continue;
            };
            let anchor = tooltip(
                Space::new()
                    .width(Length::Fixed(2.0 * PIN_RADIUS))
                    .height(Length::Fixed(2.0 * PIN_RADIUS)),
                container(text(name).size(TOOL_TEXT))
                    .padding([4, 8])
                    .style(theme::popup),
                tooltip::Position::Right,
            );
            gutter_layers =
                gutter_layers.push(pin(anchor).x(gutter_pin_x() - PIN_RADIUS).y(y - PIN_RADIUS));
        }
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
            gutter_layers,
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
        // The pane colour behind the row hides the seam where the gutter and board meet at a
        // fractional x.
        let scrolling = scrollable(container(content).style(theme::pane))
            .on_scroll(|viewport| Message::Scrolled(viewport.absolute_offset().y))
            .width(Fill)
            .height(Fill);
        // The node menu's "+" stays at the board's top right, clear of the scrollbar.
        let add = button(text("+").size(24).center())
            .on_press(Message::OpenMenu(Menu::AddNode))
            .width(Length::Fixed(ADD_BUTTON))
            .height(Length::Fixed(ADD_BUTTON))
            .style(theme::tool(false, Place::Alone));
        let mut layers = stack![
            scrolling,
            pin(add).x(size.width - ADD_BUTTON - 24.0).y(12.0)
        ];
        if let Some(dialog) = self.node_dialog(Size::new(tracks_width, viewport)) {
            layers = layers.push(pin(opaque(dialog)));
        }
        layers.into()
    }

    /// The selected node's dialog, `size` big, covering the track pane; its body scrolls under a
    /// fixed header.
    fn node_dialog(&self, size: Size) -> Option<Element<'_, Message>> {
        let Some(Selection::Node(id)) = self.selection() else {
            return None;
        };
        let node = self.session().state().node(id)?;
        let header = row![
            text(crate::patch_board::title(&node.kind)).size(18),
            Space::new().width(Fill),
            button(text("×").size(22))
                .on_press(Message::Select(None))
                .padding([0, 4])
                .style(theme::bare),
        ]
        .align_y(Alignment::Center);
        let footer = row![
            Space::new().width(Fill),
            tool(
                Face::Label("Delete"),
                Some(Message::AskDeleteNode(id)),
                false
            ),
        ];
        let body: Element<'_, Message> = match &node.kind {
            NodeKind::Mixer { channels, .. } => self.mixer_panel(id, *channels),
            NodeKind::FeedbackDelay { seconds, .. } => {
                let typed = match &self.node_text {
                    Some((node, typed)) if *node == id => typed.clone(),
                    _ => format!("{seconds}"),
                };
                row![
                    text("Time (seconds)").size(14),
                    text_input("0.25", &typed)
                        .on_input(move |t| Message::NodeText(id, t))
                        .on_submit(Message::CommitNodeText)
                        .width(Length::Fixed(100.0)),
                ]
                .spacing(10)
                .align_y(Alignment::Center)
                .into()
            }
            NodeKind::Effect { kind, .. } => {
                self.effect_panel(id, kind, size.width - 2.0 * DIALOG_PADDING)
            }
            NodeKind::Bus(_) => Space::new().into(),
        };
        let dialog = container(column![header, scrollable(body).height(Fill), footer].spacing(12))
            .padding(DIALOG_PADDING)
            .width(Length::Fixed(size.width))
            .height(Length::Fixed(size.height))
            .style(theme::panel);
        // A press on the dialog's background, not on a control, takes the keyboard focus away.
        let dialog = mouse_area(dialog).on_press(Message::SetMixerFocus(None));
        if self.confirming_delete != Some(id) {
            return Some(dialog.into());
        }
        let title = crate::patch_board::title(&node.kind);
        let confirm = container(
            column![
                text(format!("Delete the {title} node?")).size(14),
                row![
                    Space::new().width(Fill),
                    // Cancel is the default: lit, and what Enter or Space does.
                    tool(Face::Label("Cancel"), Some(Message::CancelDelete), true),
                    tool(Face::Label("Delete"), Some(Message::DeleteNode(id)), false),
                ]
                .spacing(6),
            ]
            .spacing(14),
        )
        .padding(16)
        .width(Length::Fixed(CONFIRM_WIDTH))
        .style(theme::popup);
        Some(
            stack![
                dialog,
                // A press outside the confirmation cancels it.
                mouse_area(container(opaque(confirm)).center(Fill).style(theme::scrim))
                    .on_press(Message::CancelDelete),
            ]
            .into(),
        )
    }

    /// A track port's name: the track's, with " L" or " R" for one side of a stereo port.
    fn port_name(&self, track: TrackId, side: Option<Side>) -> Option<String> {
        let name = &self.session().state().track(track)?.name;
        Some(match side {
            Some(Side::Left) => format!("{name} L"),
            Some(Side::Right) => format!("{name} R"),
            None => name.clone(),
        })
    }

    fn track_pane(&self) -> Element<'_, Message> {
        let cursor = self.playhead_fraction();
        let (start, span) = (self.view_start(), self.view_span());
        let selected = self.selected_range();
        let dragging = self.is_dragging();
        let grid = self.grid().then(|| GridView {
            tempo: self.session().state().tempo,
            rate: self.sample_rate(),
            start,
            span,
            scale: self.scale_factor,
        });
        let mut tracks = column![];
        for (id, lanes) in &self.lanes {
            let Some(track) = self.session().state().track(*id) else {
                continue;
            };
            tracks = tracks.push(self.track_header(*id, &track.name));
            let band = selected
                .as_ref()
                .filter(|(ids, _)| ids.contains(id))
                .and_then(|(_, range)| band(range.start(), range.end(), start, span));
            for lane in lanes {
                let view = LaneView {
                    lane,
                    track: *id,
                    dragging,
                    generation: self.generation,
                    start,
                    span,
                    scale: self.scale_factor,
                };
                // The cursor is stacked above the lane, in a layer of its own, to draw over
                // the lane's image.
                tracks = tracks.push(
                    stack![
                        canvas(view).width(Fill).height(Fill),
                        canvas(CursorView {
                            at: cursor,
                            band,
                            grid
                        })
                        .width(Fill)
                        .height(Fill),
                    ]
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
                .style(theme::tool(false, Place::Alone)),
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
        let playing = self.engine().is_playing();
        let unless_playing = |message: Message| (!playing).then_some(message);
        let arming = self.arming(id);
        let mut controls = row![chip(
            "Arm",
            unless_playing(Message::Arm(id)),
            arming.is_some(),
            Accent::Record
        )]
        .spacing(4)
        .align_y(Alignment::Center);
        if let Some((monitor, input)) = arming {
            controls = controls.push(chip(
                "Monitor",
                unless_playing(Message::Monitor(id)),
                monitor,
                Accent::Audio,
            ));
            let choices = self.input_choices(id);
            let selected = choices.contains(&input).then_some(input);
            controls = controls.push(
                pick_list(choices, selected, move |choice| Message::Input(id, choice))
                    .placeholder("No input")
                    .text_size(12)
                    .padding([1, 6]),
            );
            controls = controls.push(input_meter(self.input_levels(id)));
        }
        container(
            row![
                label,
                Space::new().width(Fill),
                controls,
                gap(6.0),
                burger(Menu::Track(id), 14.0, 4.0)
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
            Menu::Click => (
                CLICK_DIALOG_X,
                size.height - BAR_HEIGHT - CLICK_DIALOG_HEIGHT,
            ),
            Menu::Device => {
                let items = self.devices.0.len().max(1) as f32;
                (
                    12.0,
                    size.height - BAR_HEIGHT - items * MENU_ITEM_HEIGHT - 12.0,
                )
            }
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
                vec![
                    Entry::item("New project", None),
                    Entry::item("Open…", None),
                    Entry::Separator,
                    Entry::item("Import…", when_stopped(Message::Import)),
                    Entry::item("Export…", Some(Message::Export(false))),
                    Entry::item("Export mono…", Some(Message::Export(true))),
                    Entry::Separator,
                    Entry::item("Quit", Some(Message::Quit)),
                ]
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
            Menu::Click => return self.click_dialog(),
            Menu::Device => self.device_menu(),
            Menu::AddNode => vec![
                Entry::item("Mixer", Some(Message::AddNode(NewNode::Mixer))),
                Entry::item("Bus", Some(Message::AddNode(NewNode::Bus))),
                Entry::item(
                    "Feedback delay",
                    Some(Message::AddNode(NewNode::FeedbackDelay)),
                ),
                Entry::Separator,
            ]
            .into_iter()
            .chain(audio_effects::EFFECTS.iter().map(|effect| {
                Entry::item(
                    effect.name,
                    Some(Message::AddNode(NewNode::Effect(effect.id))),
                )
            }))
            .collect(),
        };
        dropdown(entries)
    }
}

/// The part of frames `from..to` in the view of `span` frames from `start`, as fractions of the
/// lane's width; `None` if none of it is visible.
fn band(from: u64, to: u64, start: u64, span: u64) -> Option<(f32, f32)> {
    let end = start.saturating_add(span);
    if to <= start || from >= end || span == 0 {
        return None;
    }
    let fraction = |frame: u64| (frame.clamp(start, end) - start) as f32 / span as f32;
    Some((fraction(from), fraction(to)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_band_clips_to_the_view() {
        assert_eq!(band(100, 300, 0, 1_000), Some((0.1, 0.3)));
        assert_eq!(band(0, 2_000, 500, 1_000), Some((0.0, 1.0)));
        assert_eq!(band(0, 500, 500, 1_000), None);
        assert_eq!(band(1_500, 1_600, 500, 1_000), None);
    }
}
