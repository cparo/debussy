//! A grid of strips, row by row, so a dialog's strips line up in columns and at the bottom of
//! each row (`docs/architecture/node-dialogs.md` "Node dialogs", ADR-0175). iced's `row` and
//! `column` cannot do this: a `Fill`-height child of a `Shrink`-height row is laid out against no
//! height at all, and rows know nothing of each other's widths.

use iced::advanced::widget::{Operation, Tree, Widget};
use iced::advanced::{Clipboard, Layout, Shell, layout, overlay, renderer};
use iced::{Element, Event, Length, Rectangle, Renderer, Size, Theme, Vector, mouse};

use crate::app::Message;

/// `rows` top to bottom, each's children left to right, `spacing` apart both ways. Column *k* is
/// as wide as the widest *k*-th child of any row; each row is as tall as its tallest child. A
/// child that shrinks to its content, such as a `container`, grows to its cell, its content
/// staying at the top left. With one row, each child keeps its own width.
pub(crate) fn strip_grid<'a>(
    rows: Vec<Vec<Element<'a, Message>>>,
    spacing: f32,
) -> Element<'a, Message> {
    let lengths = rows.iter().map(Vec::len).collect();
    Element::new(StripGrid {
        children: rows.into_iter().flatten().collect(),
        lengths,
        spacing,
    })
}

/// The column widths and row heights of a grid whose children, row by row, have the `natural`
/// sizes: column *k* the widest *k*-th child of any row, each row its tallest child.
pub(crate) fn grid_sizes(natural: &[Vec<Size>]) -> (Vec<f32>, Vec<f32>) {
    let mut columns: Vec<f32> = Vec::new();
    for row in natural {
        for (k, size) in row.iter().enumerate() {
            match columns.get_mut(k) {
                Some(width) => *width = width.max(size.width),
                None => columns.push(size.width),
            }
        }
    }
    let rows = natural
        .iter()
        .map(|row| row.iter().fold(0.0_f32, |h, s| h.max(s.height)))
        .collect();
    (columns, rows)
}

/// See [`strip_grid`].
struct StripGrid<'a> {
    /// The children, row by row, each row left to right.
    children: Vec<Element<'a, Message>>,
    /// How many children each row has, top to bottom.
    lengths: Vec<usize>,
    /// The gap between neighbours, across and down.
    spacing: f32,
}

impl Widget<Message, Theme, Renderer> for StripGrid<'_> {
    fn size(&self) -> Size<Length> {
        Size::new(Length::Shrink, Length::Shrink)
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let max = limits.max();
        let shrink = Size::new(true, true);
        // First, each child at its own size, to find the columns and rows.
        let natural = layout::Limits::with_compression(Size::ZERO, max, shrink);
        let sizes: Vec<Size> = self
            .children
            .iter_mut()
            .zip(&mut tree.children)
            .map(|(child, state)| {
                child
                    .as_widget_mut()
                    .layout(state, renderer, &natural)
                    .size()
            })
            .collect();
        let mut rest = sizes.as_slice();
        let by_row: Vec<Vec<Size>> = self
            .lengths
            .iter()
            .map(|&n| {
                let (row, tail) = rest.split_at(n.min(rest.len()));
                rest = tail;
                row.to_vec()
            })
            .collect();
        let (columns, heights) = grid_sizes(&by_row);
        // Then each in its cell, row by row.
        let mut children = self.children.iter_mut().zip(&mut tree.children);
        let mut nodes = Vec::with_capacity(sizes.len());
        let (mut y, mut width) = (0.0_f32, 0.0_f32);
        for (&n, &height) in self.lengths.iter().zip(&heights) {
            let mut x = 0.0;
            for (&column, (child, state)) in columns.iter().zip(children.by_ref().take(n)) {
                let cell = Size::new(column, height);
                let limits = layout::Limits::with_compression(cell, cell, shrink);
                nodes.push(
                    child
                        .as_widget_mut()
                        .layout(state, renderer, &limits)
                        .move_to((x, y)),
                );
                x += column + self.spacing;
            }
            width = width.max(x - self.spacing);
            y += height + self.spacing;
        }
        let height = (y - self.spacing).max(0.0);
        layout::Node::with_children(Size::new(width.max(0.0), height), nodes)
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        for ((child, state), layout) in self
            .children
            .iter()
            .zip(&tree.children)
            .zip(layout.children())
            .filter(|(_, layout)| layout.bounds().intersects(viewport))
        {
            child
                .as_widget()
                .draw(state, renderer, theme, style, layout, cursor, viewport);
        }
    }

    fn children(&self) -> Vec<Tree> {
        self.children.iter().map(Tree::new).collect()
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(&self.children);
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        operation.container(None, layout.bounds());
        operation.traverse(&mut |operation| {
            for ((child, state), layout) in self
                .children
                .iter_mut()
                .zip(&mut tree.children)
                .zip(layout.children())
            {
                child
                    .as_widget_mut()
                    .operate(state, layout, renderer, operation);
            }
        });
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        for ((child, state), layout) in self
            .children
            .iter_mut()
            .zip(&mut tree.children)
            .zip(layout.children())
        {
            child.as_widget_mut().update(
                state, event, layout, cursor, renderer, clipboard, shell, viewport,
            );
        }
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.children
            .iter()
            .zip(&tree.children)
            .zip(layout.children())
            .map(|((child, state), layout)| {
                child
                    .as_widget()
                    .mouse_interaction(state, layout, cursor, viewport, renderer)
            })
            .max()
            .unwrap_or_default()
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        overlay::from_children(
            &mut self.children,
            tree,
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sizes(row: &[(f32, f32)]) -> Vec<Size> {
        row.iter().map(|&(w, h)| Size::new(w, h)).collect()
    }

    #[test]
    fn test_grid_sizes_column_is_the_wider_of_two_rows() {
        let (columns, rows) = grid_sizes(&[
            sizes(&[(100.0, 50.0), (80.0, 70.0)]),
            sizes(&[(90.0, 40.0), (120.0, 30.0)]),
        ]);
        assert_eq!(columns, vec![100.0, 120.0]);
        assert_eq!(rows, vec![70.0, 40.0]);
    }

    #[test]
    fn test_grid_sizes_ragged_rows() {
        let (columns, rows) = grid_sizes(&[
            sizes(&[(10.0, 5.0)]),
            sizes(&[(20.0, 6.0), (30.0, 7.0), (40.0, 8.0)]),
        ]);
        assert_eq!(columns, vec![20.0, 30.0, 40.0]);
        assert_eq!(rows, vec![5.0, 8.0]);
    }

    #[test]
    fn test_grid_sizes_one_row_keeps_each_width() {
        let (columns, rows) = grid_sizes(&[sizes(&[(10.0, 5.0), (30.0, 9.0), (20.0, 7.0)])]);
        assert_eq!(columns, vec![10.0, 30.0, 20.0]);
        assert_eq!(rows, vec![9.0]);
    }

    #[test]
    fn test_grid_sizes_empty() {
        assert_eq!(grid_sizes(&[]), (vec![], vec![]));
        assert_eq!(grid_sizes(&[vec![]]), (vec![], vec![0.0]));
    }
}
