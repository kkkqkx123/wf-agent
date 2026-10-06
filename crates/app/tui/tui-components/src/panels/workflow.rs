//! The workflow panel.

use crate::select::{Group, GroupItem, NavigateDir, SelectList};
use tui_core::keymap::KeyAction;

/// The workflow panel.
#[derive(Debug, Clone)]
pub struct WorkflowPanel {
    list: SelectList<String>,
}

impl WorkflowPanel {
    /// Build the panel from workflow summaries.
    pub fn new(workflows: &[wf_api::workflow::summary::WorkflowSummary]) -> Self {
        let mut group = Group::new(Some("workflows"));
        for wf in workflows {
            let label = if let Some(desc) = &wf.description {
                format!("{} · {} — {desc}", wf.id, wf.name)
            } else {
                format!("{} · {}", wf.id, wf.name)
            };
            group = group.item(
                GroupItem::new(label, wf.id.clone())
                    .described(format!("{} nodes · {} edges", wf.node_count, wf.edge_count)),
            );
        }
        Self {
            list: SelectList::groups(vec![group]),
        }
    }

    /// The workflow id under the cursor.
    pub fn selected_workflow(&self) -> Option<String> {
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
