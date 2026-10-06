//! Combined `@` mention panel: files (with optional `:#lines`), skills and
//! workflows presented in three groups with a shared filter.

use crate::select::{Group, GroupItem, NavigateDir, SelectList};
use tui_core::keymap::KeyAction;
use wf_types::skill::SkillMetadata;

#[derive(Debug, Clone)]
pub struct MentionPanel {
    list: SelectList<String>,
    filter: String,
}

impl MentionPanel {
    /// Build the panel from file paths, skill names and workflow summaries.
    /// `filter` is an optional substring filter applied to labels.
    pub fn new(
        files: &[String],
        skills: &[SkillMetadata],
        workflows: &[wf_api::workflow::summary::WorkflowSummary],
        filter: Option<&str>,
    ) -> Self {
        let mut groups: Vec<Group<String>> = Vec::new();
        if !files.is_empty() {
            let mut g = Group::new(Some("files"));
            for f in files {
                g = g.item(GroupItem::new(f.clone(), f.clone()));
            }
            groups.push(g);
        }
        if !skills.is_empty() {
            let mut g = Group::new(Some("skills"));
            for s in skills {
                let label = format!("skill:{}", s.name);
                g = g.item(GroupItem::new(label.clone(), label).described(s.description.clone()));
            }
            groups.push(g);
        }
        if !workflows.is_empty() {
            let mut g = Group::new(Some("workflows"));
            for wf in workflows {
                let label = format!("workflow:{}", wf.id);
                g = g.item(GroupItem::new(label.clone(), label).described(wf.name.clone()));
            }
            groups.push(g);
        }
        if groups.is_empty() {
            groups.push(Group::new(Some("mentions")));
        }
        let mut list = SelectList::groups(groups);
        list.set_filter(filter);
        Self {
            list,
            filter: filter.unwrap_or_default().to_string(),
        }
    }

    /// The candidate label under the cursor.
    pub fn selected_candidate(&self) -> Option<String> {
        self.list.selected().map(|item| item.data.clone())
    }

    /// Push a character into the filter.
    pub fn filter_push(&mut self, c: char) {
        self.filter.push(c);
        self.list.set_filter(if self.filter.is_empty() {
            None
        } else {
            Some(self.filter.as_str())
        });
    }

    /// Pop the last character from the filter.
    pub fn filter_backspace(&mut self) {
        self.filter.pop();
        self.list.set_filter(if self.filter.is_empty() {
            None
        } else {
            Some(self.filter.as_str())
        });
    }

    /// Re-apply a filter (fuzzy substring).
    pub fn set_filter(&mut self, filter: Option<&str>) {
        self.filter = filter.unwrap_or_default().to_string();
        self.list.set_filter(filter);
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

    /// Number of visible candidates.
    pub fn len(&self) -> usize {
        self.list.len()
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }
}
