//! The model profile panel.

use crate::select::{Group, GroupItem, NavigateDir, SelectList};
use tui_core::keymap::KeyAction;
use wf_types::llm::LlmProfile;

/// The model profile panel.
#[derive(Debug, Clone)]
pub struct ModelPanel {
    list: SelectList<String>,
    /// Currently active profile id (marked in the list).
    current: Option<String>,
}

impl ModelPanel {
    /// Build the panel from the gateway profile list.
    pub fn new(profiles: &[LlmProfile], current: Option<&str>) -> Self {
        let mut group = Group::new(Some("model profiles"));
        for profile in profiles {
            let label = if Some(profile.id.as_str()) == current {
                format!("{} · {} (active)", profile.id, profile.model)
            } else {
                format!("{} · {}", profile.id, profile.model)
            };
            group = group
                .item(GroupItem::new(label, profile.id.clone()).described(profile.name.clone()));
        }
        // SelectList tracks position over filtered candidates; with no
        // filter every item is a candidate, so the flat index (position in
        // `profiles`) equals the candidate index.
        let cursor = current.and_then(|id| profiles.iter().position(|p| p.id == id));
        let list = SelectList::groups(vec![group]);
        let mut panel = Self {
            list,
            current: current.map(str::to_string),
        };
        if let Some(cursor) = cursor {
            panel.list.move_to(cursor);
        }
        panel
    }

    /// The profile id under the cursor.
    pub fn selected_model(&self) -> Option<String> {
        self.list.selected().map(|item| item.data.clone())
    }

    /// Whether the cursor sits on the active profile.
    pub fn on_current(&self) -> bool {
        self.selected_model()
            .is_some_and(|id| Some(&id) == self.current.as_ref())
    }

    /// The currently active profile id.
    pub fn current_id(&self) -> Option<&str> {
        self.current.as_deref()
    }

    /// Apply a keymap action; returns whether it was consumed.
    pub fn handle(&mut self, action: KeyAction) -> bool {
        match action {
            KeyAction::MovePrev => {
                self.list.navigate(NavigateDir::Prev);
                true
            }
            KeyAction::MoveNext => {
                self.list.navigate(NavigateDir::Next);
                true
            }
            _ => false,
        }
    }

    /// Render the panel rows.
    pub fn render_lines(
        &self,
        width: u16,
        window_height: u16,
    ) -> Vec<ratatui::text::Line<'static>> {
        self.list.render_lines(width, window_height)
    }

    /// `(N/M)` position indicator.
    pub fn position_string(&self) -> String {
        self.list.position_string()
    }
}
