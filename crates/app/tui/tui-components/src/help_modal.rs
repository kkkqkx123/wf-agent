//! Help modal derived from the command registry.

use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::modal::{centered_rect, Modal, ModalAction, ModalResult};
use tui_core::command::help_sections;
use tui_core::keymap::{CKey, Key};
use tui_style::theme::Theme;

fn to_color(rgb: tui_style::theme::Rgb) -> Color {
    Color::Rgb(rgb.r, rgb.g, rgb.b)
}

/// Help modal listing every command in the registry. It is transparent:
/// the screen behind it keeps rendering so the user can read the shortcut
/// in context.
#[derive(Default)]
pub struct HelpModal;

impl HelpModal {
    pub fn new() -> Self {
        Self
    }
}

impl Modal for HelpModal {
    fn title(&self) -> &str {
        "Help"
    }

    fn is_transparent(&self) -> bool {
        true
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let mut help_text = String::new();
        for (group, rows) in help_sections() {
            if !help_text.is_empty() {
                help_text.push('\n');
            }
            help_text.push_str(group);
            help_text.push_str(":\n");
            for row in rows {
                help_text.push_str("  ");
                help_text.push_str(&row.keys);
                help_text.push_str(" - ");
                help_text.push_str(row.title);
                help_text.push('\n');
            }
        }
        let block = Block::default()
            .title(" Help (?) ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(to_color(theme.accent)));
        let paragraph = Paragraph::new(help_text.trim_end().to_string())
            .block(block)
            .style(Style::default().fg(to_color(theme.fg)));
        let centered = centered_rect(70, 70, area);
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
