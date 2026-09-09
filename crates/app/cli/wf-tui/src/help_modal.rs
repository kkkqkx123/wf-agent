//! Help modal showing key bindings.

use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::keymap::{CKey, Key};
use crate::modal::{centered_rect, Modal, ModalAction, ModalResult};
use crate::theme::Theme;

fn to_color(rgb: crate::theme::Rgb) -> Color {
    Color::Rgb(rgb.r, rgb.g, rgb.b)
}

/// Help modal showing key bindings. It is transparent: the screen behind it
/// keeps rendering so the user can read the shortcut in context.
pub struct HelpModal;

impl Modal for HelpModal {
    fn title(&self) -> &str {
        "Help"
    }

    fn is_transparent(&self) -> bool {
        true
    }

    fn draw(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let help_text = "Keys:\n  q / Esc - quit / close\n  1-8 - switch screens\n  ? - help\n  j/k - navigate\n  Enter - select\n  y/n - confirm/cancel\n  Ctrl-Z - suspend (fg to resume)";
        let block = Block::default()
            .title(" Help (?) ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(to_color(theme.accent)));
        let paragraph = Paragraph::new(help_text)
            .block(block)
            .style(Style::default().fg(to_color(theme.fg)));
        let centered = centered_rect(70, 60, area);
        // Transparent: no Clear, the underlying screen stays visible.
        frame.render_widget(paragraph, centered);
    }

    fn handle_key(&mut self, key: Key) -> ModalAction {
        match key.code {
            CKey::Esc => ModalAction::Close(ModalResult::Dismissed),
            CKey::Char('q') | CKey::Char('?') => ModalAction::Close(ModalResult::Dismissed),
            _ => ModalAction::Stay,
        }
    }
}
