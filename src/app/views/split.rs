//! Resizable split pane widget

use iced::advanced::layout::{self, Layout};
use iced::advanced::overlay;
use iced::advanced::renderer::{self, Quad};
use iced::advanced::widget::{self, Operation, Tree, Widget};
use iced::advanced::{Clipboard, Shell};
use iced::mouse::{self, Cursor};
use iced::{Color, Element, Event, Length, Point, Rectangle, Size, Vector};

/// Split direction
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitDirection {
    /// Left-right split (vertical divider line)
    Horizontal,
    /// Top-bottom split (horizontal divider line)
    Vertical,
}

/// Split handle width in pixels
pub const HANDLE_WIDTH: f32 = 4.0;
/// Extended hit area for easier grabbing
const HANDLE_HIT_AREA: f32 = 8.0;

/// State for tracking drag operations
#[derive(Debug, Default)]
struct SplitState {
    is_dragging: bool,
}

/// Resizable split pane widget
pub struct Split<'a, Message, Theme = iced::Theme, Renderer = iced::Renderer>
where
    Renderer: iced::advanced::Renderer,
{
    first: Element<'a, Message, Theme, Renderer>,
    second: Element<'a, Message, Theme, Renderer>,
    direction: SplitDirection,
    split_at: f32,
    on_resize: Box<dyn Fn(f32) -> Message + 'a>,
    on_resize_end: Option<Box<dyn Fn() -> Message + 'a>>,
    min_first: f32,
    min_second: f32,
    handle_color: Color,
    handle_hover_color: Color,
}

impl<'a, Message, Theme, Renderer> Split<'a, Message, Theme, Renderer>
where
    Renderer: iced::advanced::Renderer,
{
    /// Create a new split pane
    pub fn new(
        first: impl Into<Element<'a, Message, Theme, Renderer>>,
        second: impl Into<Element<'a, Message, Theme, Renderer>>,
        direction: SplitDirection,
        split_at: f32,
        on_resize: impl Fn(f32) -> Message + 'a,
    ) -> Self {
        Self {
            first: first.into(),
            second: second.into(),
            direction,
            split_at,
            on_resize: Box::new(on_resize),
            on_resize_end: None,
            min_first: 50.0,
            min_second: 50.0,
            handle_color: Color::from_rgb(0.3, 0.3, 0.3),
            handle_hover_color: Color::from_rgb(0.5, 0.5, 0.5),
        }
    }

    /// Set callback for when resize ends (for saving state)
    pub fn on_resize_end(mut self, callback: impl Fn() -> Message + 'a) -> Self {
        self.on_resize_end = Some(Box::new(callback));
        self
    }

    /// Set minimum size for the first pane
    pub fn min_first(mut self, min: f32) -> Self {
        self.min_first = min;
        self
    }

    /// Set minimum size for the second pane
    pub fn min_second(mut self, min: f32) -> Self {
        self.min_second = min;
        self
    }

    /// Set handle colors
    pub fn handle_colors(mut self, normal: Color, hover: Color) -> Self {
        self.handle_color = normal;
        self.handle_hover_color = hover;
        self
    }

    /// Check if a point is within the handle hit area
    fn is_in_handle(&self, bounds: Rectangle, position: Point) -> bool {
        let handle_bounds = self.handle_bounds(bounds);
        let extended = Rectangle {
            x: handle_bounds.x - (HANDLE_HIT_AREA - HANDLE_WIDTH) / 2.0,
            y: handle_bounds.y - (HANDLE_HIT_AREA - HANDLE_WIDTH) / 2.0,
            width: match self.direction {
                SplitDirection::Horizontal => HANDLE_HIT_AREA,
                SplitDirection::Vertical => handle_bounds.width,
            },
            height: match self.direction {
                SplitDirection::Horizontal => handle_bounds.height,
                SplitDirection::Vertical => HANDLE_HIT_AREA,
            },
        };
        extended.contains(position)
    }

    /// Get the handle bounds
    fn handle_bounds(&self, bounds: Rectangle) -> Rectangle {
        match self.direction {
            SplitDirection::Horizontal => Rectangle {
                x: bounds.x + self.split_at - HANDLE_WIDTH / 2.0,
                y: bounds.y,
                width: HANDLE_WIDTH,
                height: bounds.height,
            },
            SplitDirection::Vertical => Rectangle {
                x: bounds.x,
                y: bounds.y + self.split_at - HANDLE_WIDTH / 2.0,
                width: bounds.width,
                height: HANDLE_WIDTH,
            },
        }
    }

    /// Calculate new split position from cursor position
    fn calculate_split(&self, bounds: Rectangle, position: Point) -> f32 {
        let total_size = match self.direction {
            SplitDirection::Horizontal => bounds.width,
            SplitDirection::Vertical => bounds.height,
        };

        let relative_pos = match self.direction {
            SplitDirection::Horizontal => position.x - bounds.x,
            SplitDirection::Vertical => position.y - bounds.y,
        };

        relative_pos.clamp(
            self.min_first,
            total_size - self.min_second - HANDLE_WIDTH,
        )
    }
}

impl<'a, Message, Theme, Renderer> Widget<Message, Theme, Renderer>
    for Split<'a, Message, Theme, Renderer>
where
    Message: Clone,
    Renderer: iced::advanced::Renderer,
{
    fn tag(&self) -> widget::tree::Tag {
        widget::tree::Tag::of::<SplitState>()
    }

    fn state(&self) -> widget::tree::State {
        widget::tree::State::new(SplitState::default())
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.first), Tree::new(&self.second)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(&[&self.first, &self.second]);
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fill)
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let size = limits.max();

        let (first_size, second_size, first_pos, second_pos) = match self.direction {
            SplitDirection::Horizontal => {
                let first_width = self.split_at - HANDLE_WIDTH / 2.0;
                let second_width = size.width - self.split_at - HANDLE_WIDTH / 2.0;
                (
                    Size::new(first_width.max(0.0), size.height),
                    Size::new(second_width.max(0.0), size.height),
                    Point::ORIGIN,
                    Point::new(self.split_at + HANDLE_WIDTH / 2.0, 0.0),
                )
            }
            SplitDirection::Vertical => {
                let first_height = self.split_at - HANDLE_WIDTH / 2.0;
                let second_height = size.height - self.split_at - HANDLE_WIDTH / 2.0;
                (
                    Size::new(size.width, first_height.max(0.0)),
                    Size::new(size.width, second_height.max(0.0)),
                    Point::ORIGIN,
                    Point::new(0.0, self.split_at + HANDLE_WIDTH / 2.0),
                )
            }
        };

        let first_limits = layout::Limits::new(Size::ZERO, first_size);
        let second_limits = layout::Limits::new(Size::ZERO, second_size);

        let first_layout = self
            .first
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, &first_limits)
            .move_to(first_pos);

        let second_layout = self
            .second
            .as_widget_mut()
            .layout(&mut tree.children[1], renderer, &second_limits)
            .move_to(second_pos);

        layout::Node::with_children(size, vec![first_layout, second_layout])
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_mut::<SplitState>();
        let bounds = layout.bounds();

        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if let Some(position) = cursor.position_in(bounds) {
                    if self.is_in_handle(bounds, Point::new(bounds.x + position.x, bounds.y + position.y)) {
                        state.is_dragging = true;
                    }
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                if state.is_dragging {
                    state.is_dragging = false;
                    if let Some(ref on_resize_end) = self.on_resize_end {
                        shell.publish((on_resize_end)());
                    }
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { position }) => {
                if state.is_dragging {
                    let new_split = self.calculate_split(bounds, *position);
                    shell.publish((self.on_resize)(new_split));
                }
            }
            _ => {}
        }

        // Forward events to children
        let mut children = layout.children();
        if let Some(first_layout) = children.next() {
            self.first.as_widget_mut().update(
                &mut tree.children[0],
                event,
                first_layout,
                cursor,
                renderer,
                clipboard,
                shell,
                viewport,
            );
        }
        if let Some(second_layout) = children.next() {
            self.second.as_widget_mut().update(
                &mut tree.children[1],
                event,
                second_layout,
                cursor,
                renderer,
                clipboard,
                shell,
                viewport,
            );
        }
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: Cursor,
        viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_ref::<SplitState>();
        let bounds = layout.bounds();
        let mut children = layout.children();

        // Draw first child
        if let Some(first_layout) = children.next() {
            self.first.as_widget().draw(
                &tree.children[0],
                renderer,
                theme,
                style,
                first_layout,
                cursor,
                viewport,
            );
        }

        // Draw second child
        if let Some(second_layout) = children.next() {
            self.second.as_widget().draw(
                &tree.children[1],
                renderer,
                theme,
                style,
                second_layout,
                cursor,
                viewport,
            );
        }

        // Draw handle
        let handle_bounds = self.handle_bounds(bounds);
        let is_hovered = cursor
            .position()
            .map(|pos| self.is_in_handle(bounds, pos))
            .unwrap_or(false);

        let handle_color = if state.is_dragging || is_hovered {
            self.handle_hover_color
        } else {
            self.handle_color
        };

        renderer.fill_quad(
            Quad {
                bounds: handle_bounds,
                border: iced::Border::default(),
                shadow: Default::default(),
                snap: true,
            },
            handle_color,
        );
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        let state = tree.state.downcast_ref::<SplitState>();
        let bounds = layout.bounds();

        // Check if dragging or hovering over handle
        if state.is_dragging {
            return match self.direction {
                SplitDirection::Horizontal => mouse::Interaction::ResizingHorizontally,
                SplitDirection::Vertical => mouse::Interaction::ResizingVertically,
            };
        }

        if let Some(position) = cursor.position() {
            if self.is_in_handle(bounds, position) {
                return match self.direction {
                    SplitDirection::Horizontal => mouse::Interaction::ResizingHorizontally,
                    SplitDirection::Vertical => mouse::Interaction::ResizingVertically,
                };
            }
        }

        // Check children
        let mut children = layout.children();
        if let Some(first_layout) = children.next() {
            let interaction = self.first.as_widget().mouse_interaction(
                &tree.children[0],
                first_layout,
                cursor,
                viewport,
                renderer,
            );
            if interaction != mouse::Interaction::default() {
                return interaction;
            }
        }
        if let Some(second_layout) = children.next() {
            let interaction = self.second.as_widget().mouse_interaction(
                &tree.children[1],
                second_layout,
                cursor,
                viewport,
                renderer,
            );
            if interaction != mouse::Interaction::default() {
                return interaction;
            }
        }

        mouse::Interaction::default()
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        let mut children = layout.children();
        if let Some(first_layout) = children.next() {
            self.first
                .as_widget_mut()
                .operate(&mut tree.children[0], first_layout, renderer, operation);
        }
        if let Some(second_layout) = children.next() {
            self.second
                .as_widget_mut()
                .operate(&mut tree.children[1], second_layout, renderer, operation);
        }
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        let mut children = layout.children();
        let first_layout = children.next()?;
        let second_layout = children.next()?;

        let (first_tree, rest) = tree.children.split_at_mut(1);
        let first_tree = first_tree.first_mut()?;
        let second_tree = rest.first_mut()?;

        self.first
            .as_widget_mut()
            .overlay(first_tree, first_layout, renderer, viewport, translation)
            .or_else(|| {
                self.second.as_widget_mut().overlay(
                    second_tree,
                    second_layout,
                    renderer,
                    viewport,
                    translation,
                )
            })
    }
}

impl<'a, Message, Theme, Renderer> From<Split<'a, Message, Theme, Renderer>>
    for Element<'a, Message, Theme, Renderer>
where
    Message: Clone + 'a,
    Theme: 'a,
    Renderer: iced::advanced::Renderer + 'a,
{
    fn from(split: Split<'a, Message, Theme, Renderer>) -> Self {
        Element::new(split)
    }
}

/// Create a horizontal split (left | right)
pub fn horizontal_split<'a, Message, Theme, Renderer>(
    left: impl Into<Element<'a, Message, Theme, Renderer>>,
    right: impl Into<Element<'a, Message, Theme, Renderer>>,
    split_at: f32,
    on_resize: impl Fn(f32) -> Message + 'a,
) -> Split<'a, Message, Theme, Renderer>
where
    Message: Clone,
    Renderer: iced::advanced::Renderer,
{
    Split::new(left, right, SplitDirection::Horizontal, split_at, on_resize)
}

/// Create a vertical split (top / bottom)
pub fn vertical_split<'a, Message, Theme, Renderer>(
    top: impl Into<Element<'a, Message, Theme, Renderer>>,
    bottom: impl Into<Element<'a, Message, Theme, Renderer>>,
    split_at: f32,
    on_resize: impl Fn(f32) -> Message + 'a,
) -> Split<'a, Message, Theme, Renderer>
where
    Message: Clone,
    Renderer: iced::advanced::Renderer,
{
    Split::new(top, bottom, SplitDirection::Vertical, split_at, on_resize)
}
