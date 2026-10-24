//! A widget laid out at least some width wide, still growing to its content: the mockup's
//! `.sel { min-width: 170px }` for a `Shrink` pick list, which iced sizes to its longest option.
//! The same wrapper can also keep some of its parent's width free, for the siblings laid out after
//! it (a row's name beside its control).

use iced::advanced::layout::{self, Layout};
use iced::advanced::widget::{tree, Operation, Tree};
use iced::advanced::{mouse, overlay, renderer, Clipboard, Shell, Widget};
use iced::{Element, Event, Length, Rectangle, Renderer, Size, Theme, Vector};

use crate::ui::app::Message;

/// `content` with a minimum width and some width kept free; everything else is the content's own.
pub struct Widths<'a> {
    content: Element<'a, Message>,
    min: f32,
    room: f32,
}

/// `content` at least `min` wide, when its parent has that room.
pub fn min_width<'a>(min: f32, content: impl Into<Element<'a, Message>>) -> Widths<'a> {
    Widths {
        content: content.into(),
        min,
        room: 0.0,
    }
}

/// `content` laid out `room` narrower than its parent allows. In a `row!` the children that are not
/// `Fill` are laid out first, each against the whole remaining width; this one leaves `room` of it
/// to the `Fill` children laid out after it.
pub fn leave_room<'a>(room: f32, content: impl Into<Element<'a, Message>>) -> Widths<'a> {
    Widths {
        content: content.into(),
        min: 0.0,
        room,
    }
}

impl Widget<Message, Theme, Renderer> for Widths<'_> {
    fn tag(&self) -> tree::Tag {
        self.content.as_widget().tag()
    }

    fn state(&self) -> tree::State {
        self.content.as_widget().state()
    }

    fn children(&self) -> Vec<Tree> {
        self.content.as_widget().children()
    }

    fn diff(&self, tree: &mut Tree) {
        self.content.as_widget().diff(tree);
    }

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
        // `min_width` stops at the maximum: a parent narrower than `min` still wins.
        let limits = limits
            .max_width((limits.max().width - self.room).max(0.0))
            .min_width(self.min);
        self.content.as_widget_mut().layout(tree, renderer, &limits)
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
            .operate(tree, layout, renderer, operation);
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
        self.content.as_widget_mut().update(
            tree, event, layout, cursor, renderer, clipboard, shell, viewport,
        );
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.content
            .as_widget()
            .mouse_interaction(tree, layout, cursor, viewport, renderer)
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
        self.content
            .as_widget()
            .draw(tree, renderer, theme, style, layout, cursor, viewport);
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        self.content
            .as_widget_mut()
            .overlay(tree, layout, renderer, viewport, translation)
    }
}

impl<'a> From<Widths<'a>> for Element<'a, Message> {
    fn from(widget: Widths<'a>) -> Self {
        Element::new(widget)
    }
}
