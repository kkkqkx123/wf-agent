//! Help modal showing key bindings.

use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::modal::{centered_rect, Modal, ModalAction, ModalResult};
use tui_core::keymap::{CKey, Key};
use tui_style::theme::Theme;

fn to_color(rgb: tui_style::theme::Rgb) -> Color {
    Color::Rgb(rgb.r, rgb.g, rgb.b)
}

/// Help modal showing key bindings. It is transparent: the screen behind it
/// keeps rendering so the user can read the shortcut in context. User
/// keymap overrides can be attached at push time and are listed under the
/// static overview so the help reflects the active configuration.
#[derive(Default)]
pub struct HelpModal {
    /// User overrides as `(binding, key description)` rows.
    overrides: Vec<(String, String)>,
}

impl HelpModal {
    /// Build the modal with the user's keymap overrides listed after the
    /// static overview.
    pub fn with_overrides(overrides: Vec<(String, String)>) -> Self {
        Self { overrides }
    }
}

impl Modal for HelpModal {
    fn title(&self) -> &str {
        "Help"
    }

    fn is_transparent(&self) -> bool {
        true
    }

    fn draw(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let mut help_text = String::from(
            "Keys:\n  q / Esc - quit / close\n  0-6 - switch screens\n  Ctrl-B - navigation overlay (all screens incl. Catalog/Git)\n  Ctrl-K or 7 - search palette\n  Ctrl-T - history transcript overlay\n  ? - help\n  j/k - navigate / scroll settings\n  [ ] / Tab - switch exec/workflow views, catalog tabs\n  / filter · X clear filter\n  PgUp/PgDn - detail scroll\n  Enter - select\n  y/n - confirm/cancel\n  o/c/e/l/+/- - settings local keys\n  Ctrl-Z - suspend (fg to resume)",
        );
        if !self.overrides.is_empty() {
            help_text.push_str("\n\nCustom bindings:");
            for (binding, key) in &self.overrides {
                help_text.push_str(&format!("\n  {key} - {binding}"));
            }
        }
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
