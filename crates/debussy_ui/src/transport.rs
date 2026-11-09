//! Time on screen: zoom levels in steps of √2, keeping a point in place while zooming, and the
//! position bar that shows and seeks the playhead (`docs/architecture/user-interface.md`
//! "Transport and seeking").

use iced::widget::canvas::{self, Event, Frame, Geometry, Program};
use iced::{Point, Rectangle, Renderer, Size, Theme, mouse};

use crate::app::Message;
use crate::theme::{CURSOR, tokens, tone_of};
use crate::waveform::MIN_SAMPLES_PER_COLUMN;

/// The farthest zoom out: frames per lane column.
pub const MAX_SAMPLES_PER_COLUMN: u64 = 1 << 24;

/// The coarsest zoom level, at [`MAX_SAMPLES_PER_COLUMN`]; level 0 is
/// [`MIN_SAMPLES_PER_COLUMN`] and each level is √2 times the one below.
pub(crate) const MAX_ZOOM: u32 = 2 * (MAX_SAMPLES_PER_COLUMN / MIN_SAMPLES_PER_COLUMN).ilog2();

/// The zoom level a project opens at: 1024 frames per column.
pub(crate) const DEFAULT_ZOOM: u32 = 18;

/// Frames per lane column at zoom `level` (clamped to [`MAX_ZOOM`]): even levels are exact
/// powers of two, odd ones √2 times the level below, rounded.
pub(crate) fn samples_per_column(level: u32) -> u64 {
    let level = level.min(MAX_ZOOM);
    let base = MIN_SAMPLES_PER_COLUMN << (level / 2);
    if level.is_multiple_of(2) {
        base
    } else {
        (base as f64 * std::f64::consts::SQRT_2).round() as u64
    }
}

/// The coarsest zoom level with fewer than `spc` frames per column, if any.
pub(crate) fn level_below(spc: u64) -> Option<u32> {
    (0..=MAX_ZOOM)
        .rev()
        .find(|&level| samples_per_column(level) < spc)
}

/// The first frame shown after zooming from `old` to `new` frames per column, so that timeline
/// frame `anchor` stays at the same place on screen.
pub(crate) fn zoomed_start(start: u64, old: u64, new: u64, anchor: u64) -> u64 {
    let offset = anchor.saturating_sub(start) as f64 * new as f64 / old.max(1) as f64;
    anchor.saturating_sub(offset.round() as u64)
}

/// What the position bar shows: the playhead and the visible window, all in frames, over a
/// timeline of `length` frames.
pub struct PositionBar {
    /// The playhead.
    pub playhead: u64,
    /// The first frame shown in the lanes.
    pub start: u64,
    /// The frames shown in the lanes.
    pub span: u64,
    /// The timeline's length: the end of the last clip, or more to fit the window and playhead.
    pub length: u64,
}

impl PositionBar {
    fn fraction(&self, frame: u64) -> f32 {
        (frame as f64 / self.length.max(1) as f64).clamp(0.0, 1.0) as f32
    }

    fn frame_at(&self, bounds: Rectangle, x: f32) -> u64 {
        let fraction = f64::from((x / bounds.width.max(1.0)).clamp(0.0, 1.0));
        (fraction * self.length as f64) as u64
    }
}

/// Whether the position bar is being dragged.
#[derive(Default)]
pub struct Dragging(bool);

/// The channel's height within the bar.
const CHANNEL: f32 = 6.0;

impl Program<Message> for PositionBar {
    type State = Dragging;

    fn update(
        &self,
        dragging: &mut Dragging,
        event: &Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let at = cursor.position_in(bounds)?;
                dragging.0 = true;
                Some(
                    canvas::Action::publish(Message::Seek(self.frame_at(bounds, at.x)))
                        .and_capture(),
                )
            }
            Event::Mouse(mouse::Event::CursorMoved { position }) if dragging.0 => {
                let x = position.x - bounds.x;
                Some(canvas::Action::publish(Message::Seek(self.frame_at(bounds, x))).and_capture())
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) if dragging.0 => {
                dragging.0 = false;
                Some(canvas::Action::capture())
            }
            _ => None,
        }
    }

    fn draw(
        &self,
        _state: &Dragging,
        renderer: &Renderer,
        theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let t = tokens(tone_of(theme));
        let mut frame = Frame::new(renderer, bounds.size());
        let top = (bounds.height - CHANNEL) / 2.0;
        frame.fill_rectangle(
            Point::new(0.0, top),
            Size::new(bounds.width, CHANNEL),
            t.channel,
        );
        let played = self.fraction(self.playhead) * bounds.width;
        frame.fill_rectangle(Point::new(0.0, top), Size::new(played, CHANNEL), CURSOR);
        // Ticks at the visible window's edges.
        for edge in [self.start, self.start.saturating_add(self.span)] {
            let x = self.fraction(edge) * bounds.width;
            frame.fill_rectangle(
                Point::new((x - 1.0).clamp(0.0, bounds.width - 2.0), 0.0),
                Size::new(2.0, bounds.height),
                t.text,
            );
        }
        vec![frame.into_geometry()]
    }

    fn mouse_interaction(
        &self,
        dragging: &Dragging,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if dragging.0 {
            mouse::Interaction::Grabbing
        } else if cursor.is_over(bounds) {
            mouse::Interaction::Pointer
        } else {
            mouse::Interaction::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_samples_per_column_doubles_every_two_levels() {
        assert_eq!(samples_per_column(0), MIN_SAMPLES_PER_COLUMN);
        assert_eq!(samples_per_column(1), 3);
        assert_eq!(samples_per_column(DEFAULT_ZOOM), 1024);
        assert_eq!(samples_per_column(MAX_ZOOM), MAX_SAMPLES_PER_COLUMN);
        assert_eq!(samples_per_column(MAX_ZOOM + 5), MAX_SAMPLES_PER_COLUMN);
        assert_eq!(MAX_ZOOM, 46);
        for level in 0..MAX_ZOOM {
            assert!(samples_per_column(level + 1) > samples_per_column(level));
        }
        for level in (0..MAX_ZOOM - 1).step_by(2) {
            assert_eq!(samples_per_column(level + 2), 2 * samples_per_column(level));
            // Rounding at the finest levels (3 for 2√2) puts them further from √2.
            let between = samples_per_column(level + 1) as f64 / samples_per_column(level) as f64;
            let tolerance = if level < 8 { 0.1 } else { 0.01 };
            assert!((between - std::f64::consts::SQRT_2).abs() < tolerance);
        }
    }

    #[test]
    fn test_level_below_finds_the_coarsest_finer_level() {
        assert_eq!(level_below(1025), Some(DEFAULT_ZOOM));
        assert_eq!(level_below(1024), Some(DEFAULT_ZOOM - 1));
        assert_eq!(level_below(MIN_SAMPLES_PER_COLUMN), None);
        assert_eq!(level_below(u64::MAX), Some(MAX_ZOOM));
    }

    #[test]
    fn test_zoomed_start_keeps_the_anchor_in_place() {
        // The anchor 1000 frames into the view at 10 per column is column 100; at 20 per
        // column it stays at column 100, 2000 frames in.
        assert_eq!(zoomed_start(5000, 10, 20, 6000), 4000);
        assert_eq!(zoomed_start(5000, 20, 10, 7000), 6000);
        // Never before the timeline's start.
        assert_eq!(zoomed_start(100, 10, 1000, 200), 0);
    }
}
