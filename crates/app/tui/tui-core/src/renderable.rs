//! Unified rendering interface for TUI components.
//!
//! [`Renderable`] provides a common trait for all visual components that
//! can draw themselves into a ratatui [`Buffer`] and report their desired
//! dimensions. This enables composition: a parent component can hold a list
//! of `impl Renderable` children and render them uniformly.

use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Direction, Layout, Rect};

/// Unified rendering interface for TUI components.
///
/// Components implement this trait to declare how tall/wide they want to be
/// and how to draw themselves into a buffer area. The optional `cursor_pos`
/// lets the parent position the terminal cursor (e.g. for the composer's
/// text caret).
pub trait Renderable {
    /// Draw the component into `area` of `buf`.
    fn render(&self, area: Rect, buf: &mut Buffer);

    /// The number of rows this component needs at the given `width`.
    fn desired_height(&self, width: u16) -> u16;

    /// The number of columns this component needs at the given `height`.
    /// Return `None` to let the parent decide.
    fn desired_width(&self, _height: u16) -> Option<u16> {
        None
    }

    /// The cursor position relative to `area`, if the component owns a
    /// text cursor (e.g. the prompt composer). Return `None` to hide the
    /// cursor.
    fn cursor_pos(&self, _area: Rect) -> Option<(u16, u16)> {
        None
    }

    /// Whether this component is currently visible.
    fn is_visible(&self) -> bool {
        true
    }

    /// Whether this component needs a redraw since the last render.
    fn needs_redraw(&self) -> bool {
        false
    }

    /// Component name for debugging and logging.
    fn name(&self) -> &'static str {
        std::any::type_name::<Self>()
    }
}

/// Layout calculation helper: wraps ratatui's `Layout` for component
/// composition.
pub struct LayoutCalculator;

impl LayoutCalculator {
    /// Split `area` into sub-areas according to `constraints` and `direction`.
    pub fn calculate(area: Rect, constraints: &[Constraint], direction: Direction) -> Vec<Rect> {
        Layout::default()
            .direction(direction)
            .constraints(constraints.as_ref())
            .split(area)
            .to_vec()
    }

    /// Split vertically (top to bottom).
    pub fn vertical(area: Rect, constraints: &[Constraint]) -> Vec<Rect> {
        Self::calculate(area, constraints, Direction::Vertical)
    }

    /// Split horizontally (left to right).
    pub fn horizontal(area: Rect, constraints: &[Constraint]) -> Vec<Rect> {
        Self::calculate(area, constraints, Direction::Horizontal)
    }

    /// Evenly split `area` into `count` rows.
    pub fn rows(area: Rect, count: u16) -> Vec<Rect> {
        let constraint = Constraint::Length(area.height / count.max(1));
        Self::vertical(area, &[constraint]).into_iter().collect()
    }

    /// Evenly split `area` into `count` columns.
    pub fn columns(area: Rect, count: u16) -> Vec<Rect> {
        let constraint = Constraint::Length(area.width / count.max(1));
        Self::horizontal(area, &[constraint]).into_iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_calculator_vertical_split() {
        let area = Rect::new(0, 0, 80, 20);
        let rects = LayoutCalculator::vertical(area, &[Constraint::Length(5), Constraint::Min(10)]);
        assert_eq!(rects.len(), 2);
        assert_eq!(rects[0].height, 5);
        assert_eq!(rects[1].height, 15);
    }

    #[test]
    fn layout_calculator_rows_even_split() {
        let area = Rect::new(0, 0, 80, 10);
        let rects = LayoutCalculator::rows(area, 5);
        assert_eq!(rects.len(), 1);
        assert_eq!(rects[0].height, 2);
    }
}
