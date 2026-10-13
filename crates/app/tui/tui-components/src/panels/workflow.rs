//! The workflow panel.

use crate::select::{Group, GroupItem, NavigateDir, SelectList};
use tui_core::keymap::KeyAction;

/// Display row for one workflow summary. The server owns the typed
/// summary; panels only need the display fields, converted from the
/// summaries response at the call site.
#[derive(Debug, Clone)]
pub struct WorkflowSummaryRow {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub node_count: usize,
    pub edge_count: usize,
}

impl WorkflowSummaryRow {
    /// Build a row from a summaries-list item, tolerating page shells.
    pub fn from_value(value: &serde_json::Value) -> Option<Self> {
        Some(Self {
            id: value.get("id")?.as_str()?.to_string(),
            name: value
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            description: value
                .get("description")
                .and_then(|v| v.as_str())
                .map(str::to_string),
            node_count: value
                .get("node_count")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as usize,
            edge_count: value
                .get("edge_count")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as usize,
        })
    }

    /// Rows for a summaries response (bare array or `PageView` shell).
    pub fn rows_for(value: &serde_json::Value) -> Vec<Self> {
        value
            .as_array()
            .cloned()
            .or_else(|| value.get("items").and_then(|v| v.as_array()).cloned())
            .unwrap_or_default()
            .iter()
            .filter_map(Self::from_value)
            .collect()
    }
}

/// The workflow panel.
#[derive(Debug, Clone)]
pub struct WorkflowPanel {
    list: SelectList<String>,
}

impl WorkflowPanel {
    /// Build the panel from workflow summaries.
    pub fn new(workflows: &[WorkflowSummaryRow]) -> Self {
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
