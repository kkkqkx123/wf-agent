//! Model and session picker modals.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Style, Color};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::keymap::{CKey, Key};
use crate::modal::{centered_rect, Modal, ModalAction, ModalResult};
use crate::select::{Group, GroupItem, NavigateDir, SelectList};
use crate::theme::Theme;

fn to_color(rgb: crate::theme::Rgb) -> Color {
    Color::Rgb(rgb.r, rgb.g, rgb.b)
}

/// Shared state for the picker modals: a filterable select list.
#[derive(Debug)]
struct PickerCore {
    list: SelectList<String>,
    filter: String,
}

impl PickerCore {
    fn new(items: Vec<(String, String)>) -> Self {
        let mut group = Group::new(None);
        for (label, data) in items {
            group = group.item(GroupItem::new(label, data));
        }
        Self {
            list: SelectList::groups(vec![group]),
            filter: String::new(),
        }
    }

    fn apply_filter(&mut self) {
        if self.filter.is_empty() {
            self.list.set_filter(None);
        } else {
            self.list.set_filter(Some(&self.filter));
        }
    }

    fn selected_id(&self) -> Option<String> {
        self.list.selected().map(|item| item.data.clone())
    }

    /// Returns `Some(result)` when the key closes the picker.
    fn handle_key(&mut self, key: Key) -> Option<ModalResult> {
        match key.code {
            CKey::Up | CKey::Char('k') => {
                self.list.navigate(NavigateDir::Prev);
                None
            }
            CKey::Down | CKey::Char('j') => {
                self.list.navigate(NavigateDir::Next);
                None
            }
            CKey::Enter => match self.selected_id() {
                Some(id) => Some(ModalResult::Value(id)),
                None => Some(ModalResult::Cancelled),
            },
            CKey::Esc | CKey::Char('q') if !key.ctrl => Some(ModalResult::Cancelled),
            CKey::Backspace => {
                self.filter.pop();
                self.apply_filter();
                None
            }
            CKey::Char(c) if !key.ctrl && !key.alt => {
                self.filter.push(c);
                self.apply_filter();
                None
            }
            _ => None,
        }
    }
}

fn render_picker(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    hint: &str,
    list: &SelectList<String>,
    filter: &str,
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
        .constraints([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(inner);

    frame.render_widget(
        Paragraph::new(format!("/{filter}")).style(Style::default().fg(to_color(theme.muted))),
        chunks[0],
    );
    let rows = list.render_lines(chunks[1].width, chunks[1].height);
    frame.render_widget(Paragraph::new(rows), chunks[1]);
    frame.render_widget(
        Paragraph::new(format!("{}  {}", list.position_string(), hint))
            .style(Style::default().fg(to_color(theme.muted))),
        chunks[2],
    );
}

/// Model picker: chooses an LLM profile id.
pub struct ModelPicker {
    core: PickerCore,
}

impl ModelPicker {
    /// `models` pairs a display label with the profile id to apply.
    pub fn new(models: Vec<(String, String)>) -> Self {
        Self {
            core: PickerCore::new(models),
        }
    }

    pub fn selected_id(&self) -> Option<String> {
        self.core.selected_id()
    }
}

impl Modal for ModelPicker {
    fn title(&self) -> &str {
        "Select model"
    }

    fn draw(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let centered = centered_rect(70, 70, area);
        render_picker(
            frame,
            centered,
            "Select model",
            "type to filter · j/k move · Enter apply · Esc cancel",
            &self.core.list,
            &self.core.filter,
            theme,
        );
    }

    fn handle_key(&mut self, key: Key) -> ModalAction {
        match self.core.handle_key(key) {
            Some(result) => ModalAction::Close(result),
            None => ModalAction::Stay,
        }
    }
}

/// Session picker: chooses a stored session to replay.
pub struct SessionPicker {
    core: PickerCore,
}

impl SessionPicker {
    /// `sessions` pairs a display label with the session id.
    pub fn new(sessions: Vec<(String, String)>) -> Self {
        Self {
            core: PickerCore::new(sessions),
        }
    }

    pub fn selected_id(&self) -> Option<String> {
        self.core.selected_id()
    }
}

impl Modal for SessionPicker {
    fn title(&self) -> &str {
        "Open session"
    }

    fn draw(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let centered = centered_rect(70, 70, area);
        render_picker(
            frame,
            centered,
            "Open session",
            "type to filter · j/k move · Enter open · Esc cancel",
            &self.core.list,
            &self.core.filter,
            theme,
        );
    }

    fn handle_key(&mut self, key: Key) -> ModalAction {
        match self.core.handle_key(key) {
            Some(result) => ModalAction::Close(result),
            None => ModalAction::Stay,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_picker_returns_the_selected_id() {
        let models = vec![
            ("gpt-4o · openai".to_string(), "openai:gpt-4o".to_string()),
            (
                "claude · anthropic".to_string(),
                "anthropic:claude".to_string(),
            ),
        ];
        let mut picker = ModelPicker::new(models);
        assert_eq!(picker.selected_id().as_deref(), Some("openai:gpt-4o"));
        assert_eq!(picker.handle_key(Key::plain(CKey::Down)), ModalAction::Stay);
        assert_eq!(picker.selected_id().as_deref(), Some("anthropic:claude"));
        assert_eq!(
            picker.handle_key(Key::plain(CKey::Enter)),
            ModalAction::Close(ModalResult::Value("anthropic:claude".to_string()))
        );
    }

    #[test]
    fn session_picker_filters_and_selects() {
        let sessions = vec![
            ("today · fix build".to_string(), "sess-1".to_string()),
            ("yesterday · refactor".to_string(), "sess-2".to_string()),
        ];
        let mut picker = SessionPicker::new(sessions);
        let _ = picker.handle_key(Key::plain(CKey::Char('y')));
        assert_eq!(picker.core.filter, "y");
        assert_eq!(
            picker.handle_key(Key::plain(CKey::Enter)),
            ModalAction::Close(ModalResult::Value("sess-2".to_string()))
        );
    }
}
