//! The skill panel.

use crate::select::{Group, GroupItem, NavigateDir, SelectList};
use tui_core::keymap::KeyAction;
use wf_types::skill::SkillMetadata;

/// The skill panel.
#[derive(Debug, Clone)]
pub struct SkillPanel {
    list: SelectList<String>,
}

impl SkillPanel {
    /// Build the panel from the loader's skill list.
    pub fn new(skills: &[SkillMetadata]) -> Self {
        let mut group = Group::new(Some("skills"));
        for skill in skills {
            group = group.item(
                GroupItem::new(skill.name.clone(), skill.name.clone())
                    .described(skill.description.clone()),
            );
        }
        Self {
            list: SelectList::groups(vec![group]),
        }
    }

    /// The skill name under the cursor.
    pub fn selected_skill(&self) -> Option<String> {
        self.list.selected().map(|item| item.data.clone())
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
