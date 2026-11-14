//! An automation curve lane's canvas: the curve drawn as vector strokes in the control colour,
//! its anchors and the selected point's handles, and the presses and drags that edit it
//! (`docs/architecture/user-interface.md` "Layout", ADR-0126). The playhead and grid are a
//! [`CursorView`](crate::waveform::CursorView) stacked over it, as on audio lanes.

use iced::widget::canvas::{self, Event, Frame, Geometry, Path, Program, Stroke};
use iced::{Point, Rectangle, Renderer, Size, Theme, Vector, keyboard, mouse};
use modulation::{EMPTY_VALUE, Point as CurvePoint};
use project_model::CurveId;

use crate::app::Message;
use crate::theme::{CONTROL, tokens, tone_of};

/// Samples drawn along each Bézier segment.
const SEGMENT_SAMPLES: usize = 64;
/// An anchor's side, in logical pixels.
const ANCHOR_SIZE: f32 = 8.0;
/// A handle's radius.
const HANDLE_RADIUS: f32 = 2.0;
/// How near a press must be to an anchor or handle to grab it.
const HIT_RADIUS: f32 = 6.0;

/// What a press on a curve lane caught.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Grab {
    /// Point `i`'s anchor.
    Anchor(usize),
    /// Point `i`'s in-handle.
    In(usize),
    /// Point `i`'s out-handle.
    Out(usize),
}

/// The canvas state of a curve lane: whether ⌥ is held, which breaks a handle's link.
#[derive(Default)]
pub struct CurveLaneState {
    alt: bool,
}

/// Draws a curve lane; a press adds a point or grabs one, a drag from a grab moves it, and a
/// sideways scroll pans.
pub struct CurveLaneView<'a> {
    /// The curve.
    pub curve: CurveId,
    /// The points shown: the curve's, or the live shape during a drag.
    pub points: &'a [CurvePoint],
    /// The first timeline frame shown.
    pub start: u64,
    /// The frames shown across the lane's width.
    pub span: u64,
    /// The selected point, whose handles are shown.
    pub selected: Option<usize>,
    /// Whether a drag from this lane is under way, so pointer moves report where it is.
    pub dragging: bool,
}

impl CurveLaneView<'_> {
    /// The x of timeline frame `frame` in a lane `width` wide.
    fn x(&self, frame: f64, width: f32) -> f32 {
        ((frame - self.start as f64) / self.span.max(1) as f64 * f64::from(width)) as f32
    }

    /// The y of value `value` in a lane `height` tall.
    fn y(value: f32, height: f32) -> f32 {
        (1.0 - value) * height
    }

    /// The timeline frame and value at `at`, within a lane of `size`; the value is not clamped.
    fn at(&self, at: Point, size: Size) -> (f64, f32) {
        let fraction = f64::from(at.x / size.width.max(1.0));
        let frame = self.start as f64 + fraction * self.span as f64;
        (frame, 1.0 - at.y / size.height.max(1.0))
    }

    /// Point `i`'s anchor on screen.
    fn anchor(&self, p: &CurvePoint, size: Size) -> Point {
        Point::new(
            self.x(p.frame as f64, size.width),
            Self::y(p.value, size.height),
        )
    }

    /// The handles of point `i` that shape a segment, on screen.
    fn handles(&self, i: usize, size: Size) -> Vec<(Grab, Point)> {
        let Some(p) = self.points.get(i) else {
            return Vec::new();
        };
        let mut handles = Vec::new();
        if i > 0 {
            let at = Point::new(
                self.x(p.frame as f64 + p.in_handle.d_frame, size.width),
                Self::y(p.value + p.in_handle.d_value, size.height),
            );
            handles.push((Grab::In(i), at));
        }
        if i + 1 < self.points.len() {
            let at = Point::new(
                self.x(p.frame as f64 + p.out_handle.d_frame, size.width),
                Self::y(p.value + p.out_handle.d_value, size.height),
            );
            handles.push((Grab::Out(i), at));
        }
        handles
    }

    /// What a press at `at` catches: the selected point's handles first, then the nearest
    /// anchor, each within [`HIT_RADIUS`].
    fn hit(&self, at: Point, size: Size) -> Option<Grab> {
        let near = |p: Point| p.distance(at) <= HIT_RADIUS;
        if let Some(selected) = self.selected
            && let Some((grab, _)) = self
                .handles(selected, size)
                .into_iter()
                .find(|(_, p)| near(*p))
        {
            return Some(grab);
        }
        self.points
            .iter()
            .enumerate()
            .map(|(i, p)| (i, self.anchor(p, size).distance(at)))
            .filter(|(_, d)| *d <= HIT_RADIUS)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| Grab::Anchor(i))
    }

    /// The curve's outline across the lane.
    fn outline(&self, size: Size) -> Path {
        let (w, h) = (size.width, size.height);
        Path::new(|path| {
            let (Some(first), Some(last)) = (self.points.first(), self.points.last()) else {
                let y = Self::y(EMPTY_VALUE, h);
                path.move_to(Point::new(0.0, y));
                path.line_to(Point::new(w, y));
                return;
            };
            let first_at = self.anchor(first, size);
            path.move_to(Point::new(first_at.x.min(0.0), first_at.y));
            path.line_to(first_at);
            let (view_start, view_end) = (self.start as i64, (self.start + self.span) as i64);
            for pair in self.points.windows(2) {
                let (a, b) = (&pair[0], &pair[1]);
                if b.frame < view_start || a.frame > view_end {
                    path.move_to(self.anchor(b, size));
                    continue;
                }
                let control = |frame: f64, value: f32| (frame, f64::from(value));
                let p0 = control(a.frame as f64, a.value);
                let p1 = control(
                    a.frame as f64 + a.out_handle.d_frame,
                    a.value + a.out_handle.d_value,
                );
                let p2 = control(
                    b.frame as f64 + b.in_handle.d_frame,
                    b.value + b.in_handle.d_value,
                );
                let p3 = control(b.frame as f64, b.value);
                for k in 1..=SEGMENT_SAMPLES {
                    let t = k as f64 / SEGMENT_SAMPLES as f64;
                    let u = 1.0 - t;
                    let (c0, c1, c2, c3) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
                    let frame = c0 * p0.0 + c1 * p1.0 + c2 * p2.0 + c3 * p3.0;
                    let value = c0 * p0.1 + c1 * p1.1 + c2 * p2.1 + c3 * p3.1;
                    path.line_to(Point::new(self.x(frame, w), Self::y(value as f32, h)));
                }
            }
            let last_at = self.anchor(last, size);
            path.move_to(last_at);
            path.line_to(Point::new(last_at.x.max(w), last_at.y));
        })
    }
}

impl Program<Message> for CurveLaneView<'_> {
    type State = CurveLaneState;

    fn update(
        &self,
        state: &mut CurveLaneState,
        event: &Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        if let Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) = event {
            state.alt = modifiers.alt();
            return None;
        }
        let size = bounds.size();
        match event {
            Event::Mouse(mouse::Event::CursorMoved { .. }) if self.dragging => {
                // Anywhere in the window: a drag past the lane's edge clamps, not stops.
                let at = cursor.position()? - Vector::new(bounds.x, bounds.y);
                let (frame, value) = self.at(at, size);
                let alt = state.alt;
                Some(canvas::Action::publish(Message::CurveMove {
                    frame,
                    value,
                    alt,
                }))
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let at = cursor.position_in(bounds)?;
                let (frame, value) = self.at(at, size);
                let press = Message::CurvePress {
                    curve: self.curve,
                    frame,
                    value,
                    hit: self.hit(at, size),
                    width: size.width,
                };
                Some(canvas::Action::publish(press).and_capture())
            }
            Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                cursor.position_in(bounds)?;
                let (x, y) = match delta {
                    mouse::ScrollDelta::Pixels { x, y } => (*x, *y),
                    // A line is about 1/40 of the lane, as on audio lanes.
                    mouse::ScrollDelta::Lines { x, y } => {
                        (x * bounds.width / 40.0, y * bounds.width / 40.0)
                    }
                };
                (x.abs() > y.abs()).then(|| {
                    canvas::Action::publish(Message::Pan(-x / bounds.width.max(1.0))).and_capture()
                })
            }
            _ => None,
        }
    }

    fn draw(
        &self,
        _state: &CurveLaneState,
        renderer: &Renderer,
        theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let t = tokens(tone_of(theme));
        let size = bounds.size();
        let mut frame = Frame::new(renderer, size);
        frame.fill_rectangle(Point::ORIGIN, size, t.lane);
        frame.stroke(
            &self.outline(size),
            Stroke::default().with_color(CONTROL).with_width(1.5),
        );
        let half = ANCHOR_SIZE / 2.0;
        for (i, p) in self.points.iter().enumerate() {
            let at = self.anchor(p, size);
            if at.x < -half || at.x > size.width + half {
                continue;
            }
            let corner = Point::new(at.x - half, at.y - half);
            let square = Size::new(ANCHOR_SIZE, ANCHOR_SIZE);
            frame.fill_rectangle(corner, square, CONTROL);
            if self.selected == Some(i) {
                frame.stroke(
                    &Path::rectangle(corner, square),
                    Stroke::default().with_color(t.text).with_width(1.5),
                );
            }
        }
        if let Some(selected) = self.selected
            && let Some(p) = self.points.get(selected)
        {
            let anchor = self.anchor(p, size);
            for (_, at) in self.handles(selected, size) {
                frame.stroke(
                    &Path::line(anchor, at),
                    Stroke::default().with_color(CONTROL).with_width(1.0),
                );
                frame.fill(&Path::circle(at, HANDLE_RADIUS), CONTROL);
            }
        }
        vec![frame.into_geometry()]
    }

    fn mouse_interaction(
        &self,
        _state: &CurveLaneState,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        match cursor.position_in(bounds) {
            _ if self.dragging => mouse::Interaction::Grabbing,
            Some(at) if self.hit(at, bounds.size()).is_some() => mouse::Interaction::Grab,
            Some(_) => mouse::Interaction::Crosshair,
            None => mouse::Interaction::default(),
        }
    }
}
