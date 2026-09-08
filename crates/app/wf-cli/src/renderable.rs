//! Unified rendering interface for TUI components.
//!
//! [`Renderable`] provides a common trait for all visual components that
//! can draw themselves into a ratatui [`Buffer`] and report their desired
//! height. This enables composition: a parent component can hold a list of
//! `impl Renderable` children and render them uniformly.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

/// Unified rendering interface for TUI components.
///
/// Components implement this trait to declare how tall they want to be at a
/// given width and how to draw themselves into a buffer area. The optional
/// `cursor_pos` lets the parent position the terminal cursor (e.g. for the
/// composer's text caret).
pub trait Renderable {
    /// Draw the component into `area` of `buf`.
    fn render(&self, area: Rect, buf: &mut Buffer);

    /// The number of rows this component needs at the given `width`.
    fn desired_height(&self, width: u16) -> u16;

    /// The cursor position relative to `area`, if the component owns a
    /// text cursor (e.g. the prompt composer). Return `None` to hide the
    /// cursor.
    fn cursor_pos(&self, _area: Rect) -> Option<(u16, u16)> {
        None
    }
}
