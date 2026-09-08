//! Simple confirmation modal.

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

use crate::keymap::{CKey, Key};
use crate::modal::{centered_rect, Modal, ModalAction, ModalResult};
use crate::theme::Theme;

fn to_color(rgb: crate::theme::Rgb) -> ratatui::style::Color {
    ratatui::style::Color::Rgb(rgb.r, rgb.g, rgb.b)
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

    fn draw(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let block = Block::default()
            .title(self.title.clone())
            .borders(Borders::ALL)
            .border_style(Style::default().fg(to_color(theme.warning)));
        let paragraph = Paragraph::new(format!("{}\n\n[y] confirm / [n] cancel", self.message))
            .block(block)
            .style(Style::default().fg(to_color(theme.fg)).add_modifier(Modifier::BOLD))
            .wrap(Wrap { trim: false });
        let centered = centered_rect(60, 30, area);
        frame.render_widget(paragraph, centered);
    }

    fn handle_key(&mut self, key: Key) -> ModalAction {
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

#[cfg(test)]
mod tests {
    use super::*;

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
