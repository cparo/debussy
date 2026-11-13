//! Scrolling the node dialog's focused control into view: Tab may move the keyboard focus to a
//! control scrolled out of sight, such as an equaliser band past the right edge of its strips
//! (`docs/architecture/user-interface.md` "Layout", ADR-0109).
//!
//! The focus ring around the focused control carries the id [`FOCUSED`]. [`reveal`] runs two
//! widget operations: the first finds that ring's bounds, the second scrolls each scrollable
//! holding it, the dialog body and its horizontal strips, by the least that shows it whole.

use iced::advanced::widget::operation::{Outcome, Scrollable};
use iced::advanced::widget::{Id, Operation, operate};
use iced::widget::operation::AbsoluteOffset;
use iced::{Rectangle, Task, Vector};

/// The id of the focused control's focus ring; only one control has it at a time.
pub(crate) const FOCUSED: &str = "dialog-focus";

/// Scrolls the control with the keyboard focus into view, if one is drawn.
pub(crate) fn reveal<T: Send + 'static>() -> Task<T> {
    operate(Locate::default()).then(|target| operate::<()>(Reveal { target }).discard())
}

/// How far to scroll a view showing `length` from `start` so that a target from `target` of
/// `size` shows whole, moving as little as possible: 0 if it already does. A target longer
/// than the view is shown from its start.
fn delta(start: f32, length: f32, target: f32, size: f32) -> f32 {
    if target < start || size > length {
        target - start
    } else if target + size > start + length {
        target + size - (start + length)
    } else {
        0.0
    }
}

/// Finds the bounds of the widget with the id [`FOCUSED`].
#[derive(Default)]
struct Locate {
    /// The bounds found, in the layout's unscrolled coordinates.
    found: Option<Rectangle>,
}

impl Operation<Rectangle> for Locate {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation<Rectangle>)) {
        operate(self);
    }

    fn container(&mut self, id: Option<&Id>, bounds: Rectangle) {
        if id == Some(&Id::new(FOCUSED)) {
            self.found = Some(bounds);
        }
    }

    fn finish(&self) -> Outcome<Rectangle> {
        match self.found {
            Some(bounds) => Outcome::Some(bounds),
            None => Outcome::None,
        }
    }
}

/// Scrolls every scrollable whose content holds `target` until `target` shows whole.
struct Reveal {
    /// The control's bounds, in the layout's unscrolled coordinates.
    target: Rectangle,
}

impl<T> Operation<T> for Reveal {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation<T>)) {
        operate(self);
    }

    fn scrollable(
        &mut self,
        _id: Option<&Id>,
        bounds: Rectangle,
        content_bounds: Rectangle,
        translation: Vector,
        state: &mut dyn Scrollable,
    ) {
        if !content_bounds.contains(self.target.center()) {
            return;
        }
        let x = delta(
            translation.x,
            bounds.width,
            self.target.x - content_bounds.x,
            self.target.width,
        );
        let y = delta(
            translation.y,
            bounds.height,
            self.target.y - content_bounds.y,
            self.target.height,
        );
        if x != 0.0 || y != 0.0 {
            state.scroll_by(AbsoluteOffset { x, y }, bounds, content_bounds);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_delta_scrolls_the_least_that_shows_the_target() {
        // In view: no scroll.
        assert_eq!(delta(100.0, 300.0, 150.0, 50.0), 0.0);
        // Past the end: its end comes to the view's end.
        assert_eq!(delta(100.0, 300.0, 380.0, 50.0), 30.0);
        // Before the start: its start comes to the view's start.
        assert_eq!(delta(100.0, 300.0, 40.0, 50.0), -60.0);
        // Longer than the view: shown from its start.
        assert_eq!(delta(100.0, 300.0, 150.0, 400.0), 50.0);
    }

    #[test]
    fn test_reveal_scrolls_only_scrollables_holding_the_target() {
        struct Fake(Vec<AbsoluteOffset>);
        impl Scrollable for Fake {
            fn snap_to(&mut self, _: iced::widget::operation::RelativeOffset<Option<f32>>) {}
            fn scroll_to(&mut self, _: AbsoluteOffset<Option<f32>>) {}
            fn scroll_by(&mut self, offset: AbsoluteOffset, _: Rectangle, _: Rectangle) {
                self.0.push(offset);
            }
        }
        let target = Rectangle::new((900.0, 40.0).into(), (100.0, 30.0).into());
        let mut reveal = Reveal { target };
        let mut state = Fake(Vec::new());
        // A horizontal strip view 400 wide at x 0, its content 1200 wide, scrolled to 100.
        let bounds = Rectangle::new((0.0, 0.0).into(), (400.0, 200.0).into());
        let content = Rectangle::new((0.0, 0.0).into(), (1200.0, 200.0).into());
        Operation::<()>::scrollable(
            &mut reveal,
            None,
            bounds,
            content,
            Vector::new(100.0, 0.0),
            &mut state,
        );
        assert_eq!(state.0.len(), 1);
        assert_eq!((state.0[0].x, state.0[0].y), (500.0, 0.0));
        // A scrollable elsewhere is left alone.
        let elsewhere = Rectangle::new((0.0, 500.0).into(), (1200.0, 200.0).into());
        Operation::<()>::scrollable(
            &mut reveal,
            None,
            bounds,
            elsewhere,
            Vector::ZERO,
            &mut state,
        );
        assert_eq!(state.0.len(), 1);
    }
}
