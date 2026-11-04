//! A dropdown: an anchor (usually a button) plus a menu that floats over the rest of
//! the window while `expanded`. The menu can hold any widgets, e.g. a checkbox list for
//! multi-select pickers. Clicking outside the menu or pressing Escape sends `on_dismiss`.

use iced::advanced::layout::{self, Layout};
use iced::advanced::overlay;
use iced::advanced::renderer;
use iced::advanced::widget::{self, Tree, Widget};
use iced::advanced::{mouse, Clipboard, Shell};
use iced::{keyboard, Element, Event, Length, Point, Rectangle, Renderer, Size, Theme, Vector};

pub struct DropDown<'a, Message> {
    anchor: Element<'a, Message>,
    menu: Element<'a, Message>,
    expanded: bool,
    on_dismiss: Message,
}

pub fn drop_down<'a, Message: Clone + 'a>(
    anchor: impl Into<Element<'a, Message>>,
    menu: impl Into<Element<'a, Message>>,
    expanded: bool,
    on_dismiss: Message,
) -> DropDown<'a, Message> {
    DropDown { anchor: anchor.into(), menu: menu.into(), expanded, on_dismiss }
}

impl<'a, Message: Clone + 'a> Widget<Message, Theme, Renderer> for DropDown<'a, Message> {
    fn size(&self) -> Size<Length> {
        self.anchor.as_widget().size()
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.anchor), Tree::new(&self.menu)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(&[self.anchor.as_widget(), self.menu.as_widget()]);
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, limits: &layout::Limits) -> layout::Node {
        self.anchor.as_widget_mut().layout(&mut tree.children[0], renderer, limits)
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
        self.anchor.as_widget().draw(&tree.children[0], renderer, theme, style, layout, cursor, viewport);
    }

    fn operate(&mut self, tree: &mut Tree, layout: Layout<'_>, renderer: &Renderer, operation: &mut dyn widget::Operation) {
        self.anchor.as_widget_mut().operate(&mut tree.children[0], layout, renderer, operation);
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
        self.anchor.as_widget_mut().update(&mut tree.children[0], event, layout, cursor, renderer, clipboard, shell, viewport);
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.anchor.as_widget().mouse_interaction(&tree.children[0], layout, cursor, viewport, renderer)
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        let mut children = tree.children.iter_mut();
        let anchor_tree = children.next()?;
        if !self.expanded {
            return self.anchor.as_widget_mut().overlay(anchor_tree, layout, renderer, viewport, translation);
        }
        let bounds = layout.bounds() + translation;
        Some(overlay::Element::new(Box::new(Menu {
            tree: children.next()?,
            menu: &mut self.menu,
            anchor: bounds,
            on_dismiss: self.on_dismiss.clone(),
        })))
    }
}

impl<'a, Message: Clone + 'a> From<DropDown<'a, Message>> for Element<'a, Message> {
    fn from(d: DropDown<'a, Message>) -> Self {
        Element::new(d)
    }
}

struct Menu<'a, 'b, Message> {
    tree: &'b mut Tree,
    menu: &'b mut Element<'a, Message>,
    anchor: Rectangle,
    on_dismiss: Message,
}

impl<Message: Clone> overlay::Overlay<Message, Theme, Renderer> for Menu<'_, '_, Message> {
    fn layout(&mut self, renderer: &Renderer, bounds: Size) -> layout::Node {
        let gap = 2.0;
        let below = bounds.height - (self.anchor.y + self.anchor.height) - gap - 6.0;
        let above = self.anchor.y - gap - 6.0;
        let max_height = below.max(above).max(60.0);
        let width = self.anchor.width.max(160.0);
        let limits = layout::Limits::new(Size::new(width, 0.0), Size::new(width, max_height));
        let node = self.menu.as_widget_mut().layout(self.tree, renderer, &limits);
        let size = node.size();
        // Open downwards unless there is clearly more room above.
        let y = if size.height <= below || below >= above {
            self.anchor.y + self.anchor.height + gap
        } else {
            self.anchor.y - size.height - gap
        };
        let x = self.anchor.x.min(bounds.width - size.width).max(0.0);
        node.move_to(Point::new(x, y))
    }

    fn draw(&self, renderer: &mut Renderer, theme: &Theme, style: &renderer::Style, layout: Layout<'_>, cursor: mouse::Cursor) {
        self.menu.as_widget().draw(self.tree, renderer, theme, style, layout, cursor, &layout.bounds());
    }

    fn operate(&mut self, layout: Layout<'_>, renderer: &Renderer, operation: &mut dyn widget::Operation) {
        self.menu.as_widget_mut().operate(self.tree, layout, renderer, operation);
    }

    fn update(
        &mut self,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
    ) {
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(_)) if !cursor.is_over(layout.bounds()) => {
                shell.publish(self.on_dismiss.clone());
                // A click on the anchor only closes the menu (it must not reopen it);
                // clicks elsewhere still reach their widget.
                if cursor.is_over(self.anchor) {
                    shell.capture_event();
                }
                return;
            }
            Event::Keyboard(keyboard::Event::KeyPressed { key: keyboard::Key::Named(keyboard::key::Named::Escape), .. }) => {
                shell.publish(self.on_dismiss.clone());
                shell.capture_event();
                return;
            }
            _ => {}
        }
        self.menu.as_widget_mut().update(self.tree, event, layout, cursor, renderer, clipboard, shell, &layout.bounds());
    }

    fn mouse_interaction(&self, layout: Layout<'_>, cursor: mouse::Cursor, renderer: &Renderer) -> mouse::Interaction {
        self.menu.as_widget().mouse_interaction(self.tree, layout, cursor, &layout.bounds(), renderer)
    }
}
