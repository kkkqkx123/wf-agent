//! Single-line secret prompt modal.

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::keymap::{CKey, Key};
use crate::modal::{centered_rect, Modal, ModalAction, ModalResult};
use crate::theme::Theme;

fn to_color(rgb: crate::theme::Rgb) -> ratatui::style::Color {
    ratatui::style::Color::Rgb(rgb.r, rgb.g, rgb.b)
}

/// Single-line secret prompt; the buffer is rendered masked.
pub struct PasswordModal {
    title: String,
    prompt: String,
    input: String,
}

impl PasswordModal {
    pub fn new(title: impl Into<String>, prompt: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            prompt: prompt.into(),
            input: String::new(),
        }
    }

    /// The typed secret; empty after a successful submit.
    pub fn value(&self) -> &str {
        &self.input
    }

    fn masked(&self) -> String {
        "*".repeat(self.input.chars().count())
    }
}

impl Modal for PasswordModal {
    fn title(&self) -> &str {
        &self.title
    }

    fn draw(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let block = Block::default()
            .title(format!(" {} ", self.title))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(to_color(theme.error)));
        let body = format!(
            "{}\n\n> {}\n\nEnter submit · Esc cancel",
            self.prompt,
            self.masked()
        );
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
            CKey::Enter => {
                let value = std::mem::take(&mut self.input);
                if value.is_empty() {
                    ModalAction::Close(ModalResult::Cancelled)
                } else {
                    ModalAction::Close(ModalResult::Value(value))
                }
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
    fn password_modal_masks_and_submits() {
        let mut modal = PasswordModal::new("Token", "Enter API token");
        let _ = modal.handle_key(Key::plain(CKey::Char('a')));
        let _ = modal.handle_key(Key::plain(CKey::Char('b')));
        assert_eq!(modal.masked(), "**");
        assert_eq!(
            modal.handle_key(Key::plain(CKey::Enter)),
            ModalAction::Close(ModalResult::Value("ab".to_string()))
        );
    }
}
