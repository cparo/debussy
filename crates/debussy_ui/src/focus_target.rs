//! A wrapper that makes a mixer or effect dialog control take the dialog's keyboard focus when
//! pressed, and, for a slider or toggle, keeps the keyboard and wheel from it so the arrow keys reach the focused
//! control rather than the hovered one (`docs/architecture/user-interface.md` "Layout").

use iced::advanced::widget::{Operation, Tree, Widget};
use iced::advanced::{Clipboard, Layout, Shell, layout, overlay, renderer};
use iced::widget::container;
use iced::{Element, Event, Length, Rectangle, Renderer, Size, Theme, Vector, mouse};

use crate::app::Message;
use crate::theme;

/// `control` in a focus ring, drawn when `focused`.
pub(crate) fn ringed<'a>(
    control: impl Into<Element<'a, Message>>,
    focused: bool,
) -> Element<'a, Message> {
    container(control)
        .padding(2)
        .style(theme::focus_ring(focused))
        .into()
}

/// `content`, sending `on_press` when the left button is pressed over it, whether or not
/// `content` takes the press too. Unless `keys`, `content` sees no keyboard or wheel events;
/// they go on to the window instead.
pub(crate) fn focus_target<'a>(
    content: impl Into<Element<'a, Message>>,
    on_press: Message,
    keys: bool,
) -> Element<'a, Message> {
    Element::new(FocusTarget {
        content: content.into(),
        on_press,
        keys,
    })
}

/// See [`focus_target`].
struct FocusTarget<'a> {
    /// The control.
    content: Element<'a, Message>,
    /// Sent when it is pressed.
    on_press: Message,
    /// Whether the control sees keyboard and wheel events.
    keys: bool,
}

impl Widget<Message, Theme, Renderer> for FocusTarget<'_> {
    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }

    fn size_hint(&self) -> Size<Length> {
        self.content.as_widget().size_hint()
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.content
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits)
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
        self.content.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.content));
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        self.content
            .as_widget_mut()
            .operate(&mut tree.children[0], layout, renderer, operation);
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
        let withheld = !self.keys
            && matches!(
                event,
                Event::Keyboard(_) | Event::Mouse(mouse::Event::WheelScrolled { .. })
            );
        if !withheld {
            self.content.as_widget_mut().update(
                &mut tree.children[0],
                event,
                layout,
                cursor,
                renderer,
                clipboard,
                shell,
                viewport,
            );
        }
        if matches!(
            event,
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
        ) && cursor.is_over(layout.bounds())
        {
            shell.publish(self.on_press.clone());
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
        self.content.as_widget().mouse_interaction(
            &tree.children[0],
            layout,
            cursor,
            viewport,
            renderer,
        )
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        self.content.as_widget_mut().overlay(
            &mut tree.children[0],
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}
