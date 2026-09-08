//! Modal overlays and the modal stack.
//!
//! A modal is a short-lived overlay that owns the keyboard until it closes
//! with a [`ModalResult`]. Pushing through
//! [`ModalStack::push_with_result`] returns a oneshot receiver: the caller can
//! await the user's answer while the event loop keeps drawing. Dropping the
//! stack (on shutdown) drops the sender, so waiting tasks resolve with
//! `RecvError` instead of hanging.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;
use tokio::sync::oneshot;

use crate::keymap::{CKey, Key};
use crate::theme::{Rgb, Theme};

/// Convert one theme color value into a ratatui color.
fn to_color(rgb: Rgb) -> ratatui::style::Color {
    ratatui::style::Color::Rgb(rgb.r, rgb.g, rgb.b)
}

/// Result of a modal interaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModalResult {
    Confirmed,
    Cancelled,
    Dismissed,
    /// Closed carrying a payload: picked id, typed secret, chosen path.
    Value(String),
}

/// Behavior of a modal component.
pub trait Modal {
    fn title(&self) -> &str;
    fn draw(&self, frame: &mut Frame, area: Rect, theme: &Theme);
    fn handle_key(&mut self, key: Key) -> ModalAction;
    /// Transparent modals skip the `Clear` pass so the underlying screen keeps
    /// showing through the overlay. Overlays that must fully hide the screen
    /// (confirmations, pickers) stay opaque.
    fn is_transparent(&self) -> bool {
        false
    }
}

/// Action after handling a key in a modal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModalAction {
    Stay,
    Close(ModalResult),
}

/// One entry on the stack: the modal plus its optional result channel.
struct ModalEntry {
    modal: Box<dyn Modal + Send>,
    result: Option<oneshot::Sender<ModalResult>>,
}

/// A stack of modals with oneshot result channels.
pub struct ModalStack {
    stack: Vec<ModalEntry>,
}

impl Default for ModalStack {
    fn default() -> Self {
        Self::new()
    }
}

impl ModalStack {
    pub fn new() -> Self {
        Self { stack: Vec::new() }
    }

    pub fn push(&mut self, modal: Box<dyn Modal + Send>) {
        self.stack.push(ModalEntry {
            modal,
            result: None,
        });
    }

    /// Push a modal and receive the channel its result is delivered on.
    pub fn push_with_result(
        &mut self,
        modal: Box<dyn Modal + Send>,
    ) -> oneshot::Receiver<ModalResult> {
        let (tx, rx) = oneshot::channel();
        self.stack.push(ModalEntry {
            modal,
            result: Some(tx),
        });
        rx
    }

    pub fn pop(&mut self) -> Option<Box<dyn Modal + Send>> {
        self.stack.pop().map(|entry| entry.modal)
    }

    /// Drop every modal, resolving pending waiters with `RecvError` (which
    /// callers map to `Cancelled`).
    pub fn clear(&mut self) {
        self.stack.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.stack.is_empty()
    }

    pub fn len(&self) -> usize {
        self.stack.len()
    }

    pub fn top_mut(&mut self) -> Option<&mut (dyn Modal + Send + '_)> {
        self.stack
            .last_mut()
            .map(|entry| entry.modal.as_mut() as &mut (dyn Modal + Send))
    }

    pub fn draw(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let Some(top) = self.stack.last() else {
            return;
        };
        if !top.modal.is_transparent() {
            frame.render_widget(Clear, area);
        }
        top.modal.draw(frame, area, theme);
    }

    pub fn handle_key(&mut self, key: Key) -> Option<ModalResult> {
        let top = self.stack.last_mut()?;
        match top.modal.handle_key(key) {
            ModalAction::Stay => None,
            ModalAction::Close(result) => {
                let entry = self.stack.pop().expect("entry present after handling");
                if let Some(tx) = entry.result {
                    let _ = tx.send(result.clone());
                }
                Some(result)
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Scroll helpers shared by the read-only viewers
// ---------------------------------------------------------------------------

/// Vertical scroll offset shared by the file and diff viewers.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Scroll {
    offset: usize,
}

impl Scroll {
    pub(crate) fn up(&mut self, step: usize) {
        self.offset = self.offset.saturating_sub(step);
    }

    pub(crate) fn down(&mut self, total: usize, view: usize, step: usize) {
        let max = total.saturating_sub(view);
        self.offset = (self.offset + step).min(max);
    }

    pub(crate) fn home(&mut self) {
        self.offset = 0;
    }

    pub(crate) fn end(&mut self, total: usize, view: usize) {
        self.offset = total.saturating_sub(view);
    }
}

/// Shared `j/k/PageUp/PageDown/Home/End` handling for scrollable viewers.
///
/// `q` and `Esc` fall through to the caller, which decides whether the modal
/// closes.
pub(crate) fn scroll_key(scroll: &mut Scroll, key: Key, total: usize, view: usize) -> bool {
    match key.code {
        CKey::Char('j') | CKey::Down => {
            scroll.down(total, view, 1);
            true
        }
        CKey::Char('k') | CKey::Up => {
            scroll.up(1);
            true
        }
        CKey::PageDown => {
            scroll.down(total, view, view.max(1));
            true
        }
        CKey::PageUp => {
            scroll.up(view.max(1));
            true
        }
        CKey::Home => {
            scroll.home();
            true
        }
        CKey::End => {
            scroll.end(total, view);
            true
        }
        _ => false,
    }
}

/// Render a windowed list of styled rows inside a bordered block.
pub(crate) fn render_viewer(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    hint: &str,
    rows: &[Line<'static>],
    scroll: &Scroll,
    theme: &Theme,
) {
    let outer = Block::default()
        .title(format!(" {title} "))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(to_color(theme.accent)));
    let inner = outer.inner(area);
    frame.render_widget(outer, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);

    let view = usize::from(chunks[0].height.max(1));
    let visible: Vec<Line<'static>> = rows
        .iter()
        .skip(scroll.offset)
        .take(view)
        .cloned()
        .collect();
    frame.render_widget(Paragraph::new(visible), chunks[0]);

    let position = if rows.is_empty() {
        "(0/0)".to_string()
    } else {
        format!("({}/{})", scroll.offset + 1, rows.len())
    };
    frame.render_widget(
        Paragraph::new(format!("{position}  {hint}"))
            .style(Style::default().fg(to_color(theme.muted))),
        chunks[1],
    );
}

/// Conservative viewport height used when the real area is unknown.
pub(crate) fn area_rows(default: usize) -> usize {
    default
}

// Re-export concrete modal types so existing `use crate::modal::*` works.
pub use crate::confirm_modal::ConfirmModal;
pub use crate::help_modal::HelpModal;
pub use crate::model_picker::{ModelPicker, SessionPicker};
pub use crate::password_modal::PasswordModal;
pub use crate::file_viewer::{DiffRow, DiffSign, DiffViewer, FileViewer};
pub use crate::file_selection::{FileEntry, FileSelectionDialog, scan_dir};

/// Center a rectangle covering `percent_x` x `percent_y` of `r`.
pub fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modal_stack_push_pop() {
        use crate::confirm_modal::ConfirmModal;
        use crate::help_modal::HelpModal;
        let mut stack = ModalStack::new();
        assert!(stack.is_empty());
        stack.push(Box::new(ConfirmModal::new("t", "m")));
        assert_eq!(stack.len(), 1);
        stack.push(Box::new(HelpModal));
        assert_eq!(stack.len(), 2);
        let _ = stack.pop();
        assert_eq!(stack.len(), 1);
    }

    #[test]
    fn help_modal_is_transparent_and_confirm_is_not() {
        use crate::confirm_modal::ConfirmModal;
        use crate::help_modal::HelpModal;
        assert!(HelpModal.is_transparent());
        assert!(!ConfirmModal::new("t", "m").is_transparent());
    }

    #[tokio::test]
    async fn push_with_result_delivers_the_answer() {
        use crate::confirm_modal::ConfirmModal;
        let mut stack = ModalStack::new();
        let rx = stack.push_with_result(Box::new(ConfirmModal::new("Delete?", "Sure?")));
        assert_eq!(stack.len(), 1);
        assert_eq!(
            stack.handle_key(Key::plain(CKey::Char('y'))),
            Some(ModalResult::Confirmed)
        );
        assert!(stack.is_empty());
        assert_eq!(rx.await, Ok(ModalResult::Confirmed));
    }

    #[tokio::test]
    async fn clearing_the_stack_cancels_waiters() {
        use crate::confirm_modal::ConfirmModal;
        let mut stack = ModalStack::new();
        let rx = stack.push_with_result(Box::new(ConfirmModal::new("Delete?", "Sure?")));
        stack.clear();
        assert!(rx.await.is_err(), "dropped sender must resolve the waiter");
    }
}
