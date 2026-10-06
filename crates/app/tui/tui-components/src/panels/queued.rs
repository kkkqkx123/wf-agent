//! The queued prompt panel (edit / delete entries of a [`crate::queue::PromptQueue`]).

use crate::queue::QueuedPrompt;
use crate::select::{Group, GroupItem, NavigateDir, SelectList};
use tui_core::keymap::KeyAction;

/// The queued prompt panel (edit / delete entries of a [`crate::queue::PromptQueue`]).
#[derive(Debug, Clone)]
pub struct QueuedPanel {
    list: SelectList<u64>,
}

impl QueuedPanel {
    /// Rebuild the panel from the current queue contents.
    pub fn new(items: &[QueuedPrompt]) -> Self {
        let mut group = Group::new(Some("queued prompts"));
        for prompt in items {
            group = group.item(GroupItem::new(prompt.text.clone(), prompt.id));
        }
        Self {
            list: SelectList::groups(vec![group]),
        }
    }

    /// The queued prompt id under the cursor.
    pub fn selected_id(&self) -> Option<u64> {
        self.list.selected().map(|item| item.data)
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
