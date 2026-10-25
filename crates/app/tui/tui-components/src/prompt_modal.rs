//! Single-line text prompt modal.

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::modal::{centered_rect, Modal, ModalAction, ModalResult};
use tui_core::keymap::{CKey, Key};
use tui_style::theme::Theme;

fn to_color(rgb: tui_style::theme::Rgb) -> ratatui::style::Color {
    ratatui::style::Color::Rgb(rgb.r, rgb.g, rgb.b)
}

/// Single-line text prompt. Unlike [`crate::password_modal::PasswordModal`]
/// the buffer renders verbatim, and an optional expected string gates
/// submission so destructive actions can demand the typed target.
pub struct PromptModal {
    title: String,
    prompt: String,
    input: String,
    expect: Option<String>,
}

impl PromptModal {
    pub fn new(title: impl Into<String>, prompt: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            prompt: prompt.into(),
            input: String::new(),
            expect: None,
        }
    }

    /// Require the typed input to equal `expected` before Enter submits;
    /// any other input keeps the modal open.
    pub fn expecting(mut self, expected: impl Into<String>) -> Self {
        self.expect = Some(expected.into());
        self
    }

    /// The typed text; empty after a successful submit.
    pub fn value(&self) -> &str {
        &self.input
    }

    fn ready(&self) -> bool {
        let trimmed = self.input.trim();
        !trimmed.is_empty()
            && self
                .expect
                .as_deref()
                .is_none_or(|expected| trimmed == expected)
    }

    fn status(&self) -> &'static str {
        match self.expect.as_deref() {
            Some(_) if !self.input.trim().is_empty() && !self.ready() => {
                "input does not match; type the exact target"
            }
            Some(_) => "type the exact target to confirm · Enter submit · Esc cancel",
            None => "Enter submit · Esc cancel",
        }
    }
}

impl Modal for PromptModal {
    fn title(&self) -> &str {
        &self.title
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let block = Block::default()
            .title(format!(" {} ", self.title))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(to_color(theme.warning)));
        let body = format!("{}\n\n> {}\n\n{}", self.prompt, self.input, self.status());
        let paragraph = Paragraph::new(body).block(block).style(
            Style::default()
                .fg(to_color(theme.fg))
                .add_modifier(Modifier::BOLD),
        );
        let centered = centered_rect(60, 30, area);
        frame.render_widget(paragraph, centered);
    }

    fn handle_key(&mut self, key: Key) -> ModalAction {
        match key.code {
            CKey::Enter if self.ready() => {
                ModalAction::Close(ModalResult::Value(std::mem::take(&mut self.input)))
            }
            CKey::Esc => ModalAction::Close(ModalResult::Cancelled),
            CKey::Backspace => {
                self.input.pop();
                ModalAction::Stay
            }
            CKey::Char(c) if !key.ctrl && !key.alt => {
                self.input.push(c);
                ModalAction::Stay
            }
            _ => ModalAction::Stay,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_modal_submits_typed_text() {
        let mut modal = PromptModal::new("Commit", "Message");
        let _ = modal.handle_key(Key::plain(CKey::Char('h')));
        let _ = modal.handle_key(Key::plain(CKey::Char('i')));
        assert_eq!(modal.value(), "hi");
        assert_eq!(
            modal.handle_key(Key::plain(CKey::Enter)),
            ModalAction::Close(ModalResult::Value("hi".to_string()))
        );
        assert!(modal.value().is_empty());
    }

    #[test]
    fn empty_input_never_submits() {
        let mut modal = PromptModal::new("Commit", "Message");
        assert_eq!(modal.handle_key(Key::plain(CKey::Enter)), ModalAction::Stay);
    }

    #[test]
    fn expecting_gates_submission_on_an_exact_match() {
        let mut modal = PromptModal::new("Delete branch", "Type the branch name").expecting("main");
        let _ = modal.handle_key(Key::plain(CKey::Char('m')));
        // Wrong text stays open; the exact target submits.
        assert_eq!(modal.handle_key(Key::plain(CKey::Enter)), ModalAction::Stay);
        let _ = modal.handle_key(Key::plain(CKey::Char('a')));
        let _ = modal.handle_key(Key::plain(CKey::Char('i')));
        let _ = modal.handle_key(Key::plain(CKey::Char('n')));
        assert_eq!(
            modal.handle_key(Key::plain(CKey::Enter)),
            ModalAction::Close(ModalResult::Value("main".to_string()))
        );
    }

    #[test]
    fn escape_cancels() {
        let mut modal = PromptModal::new("Commit", "Message");
        assert_eq!(
            modal.handle_key(Key::plain(CKey::Esc)),
            ModalAction::Close(ModalResult::Cancelled)
        );
    }
}
