use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

/// Result of a modal interaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModalResult {
    Confirmed,
    Cancelled,
    Dismissed,
}

/// Behavior of a modal component.
pub trait Modal {
    fn title(&self) -> &str;
    fn draw(&self, frame: &mut Frame, area: Rect);
    fn handle_key(&mut self, key: crate::keymap::Key) -> ModalAction;
}

/// Action after handling a key in a modal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModalAction {
    Stay,
    Close(ModalResult),
}

/// A stack of modals with oneshot result channels.
pub struct ModalStack {
    stack: Vec<Box<dyn Modal + Send>>,
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
        self.stack.push(modal);
    }

    pub fn pop(&mut self) -> Option<Box<dyn Modal + Send>> {
        self.stack.pop()
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
            .map(|m| m.as_mut() as &mut (dyn Modal + Send))
    }

    pub fn draw(&self, frame: &mut Frame, area: Rect) {
        if let Some(top) = self.stack.last() {
            top.draw(frame, area);
        }
    }

    pub fn handle_key(&mut self, key: crate::keymap::Key) -> Option<ModalResult> {
        if let Some(top) = self.stack.last_mut() {
            match top.handle_key(key) {
                ModalAction::Stay => None,
                ModalAction::Close(result) => {
                    self.stack.pop();
                    Some(result)
                }
            }
        } else {
            None
        }
    }
}

/// Simple confirmation modal.
pub struct ConfirmModal {
    title: String,
    message: String,
}

impl ConfirmModal {
    pub fn new(title: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            message: message.into(),
        }
    }
}

impl Modal for ConfirmModal {
    fn title(&self) -> &str {
        &self.title
    }

    fn draw(&self, frame: &mut Frame, area: Rect) {
        let block = Block::default()
            .title(self.title.clone())
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Yellow));
        let paragraph = Paragraph::new(format!("{}\n\n[y] confirm / [n] cancel", self.message))
            .block(block)
            .style(Style::default().fg(Color::White));
        let centered = centered_rect(60, 30, area);
        frame.render_widget(Clear, centered);
        frame.render_widget(paragraph, centered);
    }

    fn handle_key(&mut self, key: crate::keymap::Key) -> ModalAction {
        use crate::keymap::CKey;
        match key.code {
            CKey::Char('y') | CKey::Char('Y') => ModalAction::Close(ModalResult::Confirmed),
            CKey::Char('n') | CKey::Char('N') | CKey::Char('q') | CKey::Char('Q') => {
                ModalAction::Close(ModalResult::Cancelled)
            }
            CKey::Esc => ModalAction::Close(ModalResult::Cancelled),
            _ => ModalAction::Stay,
        }
    }
}

/// Help modal showing key bindings.
pub struct HelpModal;

impl Modal for HelpModal {
    fn title(&self) -> &str {
        "Help"
    }

    fn draw(&self, frame: &mut Frame, area: Rect) {
        let help_text = "Keys:\n  q / Esc - quit / close\n  1-8 - switch screens\n  ? - help\n  j/k - navigate\n  Enter - select\n  y/n - confirm/cancel";
        let block = Block::default()
            .title(" Help (?) ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan));
        let paragraph = Paragraph::new(help_text).block(block);
        let centered = centered_rect(70, 60, area);
        frame.render_widget(Clear, centered);
        frame.render_widget(paragraph, centered);
    }

    fn handle_key(&mut self, key: crate::keymap::Key) -> ModalAction {
        use crate::keymap::CKey;
        match key.code {
            CKey::Esc => ModalAction::Close(ModalResult::Dismissed),
            CKey::Char('q') | CKey::Char('?') => ModalAction::Close(ModalResult::Dismissed),
            _ => ModalAction::Stay,
        }
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    use ratatui::layout::{Constraint, Direction, Layout};
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
    use crate::keymap::{CKey, Key};

    #[test]
    fn modal_stack_push_pop() {
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
    fn confirm_modal_key_handling() {
        let mut modal = ConfirmModal::new("Delete?", "Sure?");
        assert_eq!(
            modal.handle_key(Key::plain(CKey::Char('y'))),
            ModalAction::Close(ModalResult::Confirmed)
        );
        let mut modal = ConfirmModal::new("Delete?", "Sure?");
        assert_eq!(
            modal.handle_key(Key::plain(CKey::Char('n'))),
            ModalAction::Close(ModalResult::Cancelled)
        );
    }
}
