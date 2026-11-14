//! A row whose children are all as tall as its tallest, so a dialog's strips line up at the
//! bottom (`docs/architecture/node-dialogs.md` "Node dialogs"). iced's `row` cannot do this: a
//! `Fill`-height child of a `Shrink`-height row is laid out against no height at all.

use iced::advanced::widget::{Operation, Tree, Widget};
use iced::advanced::{Clipboard, Layout, Shell, layout, overlay, renderer};
use iced::{Element, Event, Length, Rectangle, Renderer, Size, Theme, Vector, mouse};

use crate::app::Message;

/// `children` left to right, `spacing` apart, each laid out as tall as the tallest. A child
/// whose height shrinks to its content, such as a `container`, grows to that height, its content
/// staying at the top.
pub(crate) fn equal_heights<'a>(
    children: impl IntoIterator<Item = Element<'a, Message>>,
    spacing: f32,
) -> Element<'a, Message> {
    Element::new(EqualHeights {
        children: children.into_iter().collect(),
        spacing,
    })
}

/// See [`equal_heights`].
struct EqualHeights<'a> {
    /// The children, left to right.
    children: Vec<Element<'a, Message>>,
    /// The gap between neighbours.
    spacing: f32,
}

impl Widget<Message, Theme, Renderer> for EqualHeights<'_> {
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
        // First, each child at its own height, to find the tallest.
        let natural = layout::Limits::with_compression(Size::ZERO, max, shrink);
        let tallest = self
            .children
            .iter_mut()
            .zip(&mut tree.children)
            .map(|(child, state)| {
                child
                    .as_widget_mut()
                    .layout(state, renderer, &natural)
                    .size()
                    .height
            })
            .fold(0.0_f32, f32::max);
        // Then each at that height, left to right.
        let fixed = layout::Limits::with_compression(
            Size::new(0.0, tallest),
            Size::new(max.width, tallest),
            shrink,
        );
        let mut x = 0.0;
        let nodes: Vec<layout::Node> = self
            .children
            .iter_mut()
            .zip(&mut tree.children)
            .map(|(child, state)| {
                let node = child
                    .as_widget_mut()
                    .layout(state, renderer, &fixed)
                    .move_to((x, 0.0));
                x += node.size().width + self.spacing;
                node
            })
            .collect();
        let width = (x - self.spacing).max(0.0);
        layout::Node::with_children(Size::new(width, tallest), nodes)
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
