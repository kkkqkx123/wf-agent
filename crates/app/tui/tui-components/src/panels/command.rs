//! The `/` command palette: built-in command set, typed filter and
//! keymap-driven navigation.

use unicode_segmentation::UnicodeSegmentation;

use crate::select::{Group, GroupItem, NavigateDir, SelectList};
use tui_core::keymap::KeyAction;

/// Identifies a command palette entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandId {
    /// `/new` — clear the conversation and start a fresh session.
    New,
    /// `/model` — open the model panel.
    Model,
    /// `/skills` — open the skill panel.
    Skill,
    /// `/queued` — open the queued prompt panel.
    Queued,
    /// `/editor` — edit the composer draft in `$EDITOR`.
    Editor,
    /// `/quit` — leave the TUI inline session.
    Quit,
    /// `/help` — show the keymap and command help.
    Help,
    /// `/workflows` — list and run a workflow.
    Workflows,
    /// `/resume` — resume the most recent session.
    Resume,
    /// `/executions` — list recent executions.
    Executions,
}

/// One command palette row.
#[derive(Debug, Clone)]
pub struct CommandEntry {
    pub id: CommandId,
    pub label: &'static str,
    pub description: &'static str,
}

/// The `/` command palette.
#[derive(Debug, Clone)]
pub struct CommandPalette {
    list: SelectList<CommandId>,
    entries: Vec<CommandEntry>,
    /// Filter text typed while the palette is open.
    filter: String,
}

impl Default for CommandPalette {
    fn default() -> Self {
        Self::new()
    }
}

impl CommandPalette {
    /// Built-in command set.
    pub fn new() -> Self {
        let entries = Self::builtin_entries();
        let list = Self::build_list(&entries, "");
        Self {
            list,
            entries,
            filter: String::new(),
        }
    }

    fn builtin_entries() -> Vec<CommandEntry> {
        vec![
            CommandEntry {
                id: CommandId::New,
                label: "/new",
                description: "start a new session (clears the conversation)",
            },
            CommandEntry {
                id: CommandId::Model,
                label: "/model",
                description: "pick the model profile for the next turns",
            },
            CommandEntry {
                id: CommandId::Skill,
                label: "/skills",
                description: "list and run a skill",
            },
            CommandEntry {
                id: CommandId::Queued,
                label: "/queued",
                description: "manage queued prompts",
            },
            CommandEntry {
                id: CommandId::Editor,
                label: "/editor",
                description: "edit the draft in $EDITOR",
            },
            CommandEntry {
                id: CommandId::Quit,
                label: "/quit",
                description: "exit the TUI inline session",
            },
            CommandEntry {
                id: CommandId::Help,
                label: "/help",
                description: "show the inline keymap and command help",
            },
            CommandEntry {
                id: CommandId::Workflows,
                label: "/workflows",
                description: "list and run a workflow",
            },
            CommandEntry {
                id: CommandId::Resume,
                label: "/resume",
                description: "resume the most recent session",
            },
            CommandEntry {
                id: CommandId::Executions,
                label: "/executions",
                description: "list recent executions",
            },
        ]
    }

    fn build_list(entries: &[CommandEntry], filter: &str) -> SelectList<CommandId> {
        let group = Group::new(Some("commands"));
        let mut group = group;
        for entry in entries {
            group = group.item(GroupItem::new(entry.label, entry.id).described(entry.description));
        }
        let mut list = SelectList::groups(vec![group]);
        list.set_filter(if filter.is_empty() {
            None
        } else {
            Some(filter)
        });
        list
    }

    /// The command highlighted by the cursor.
    pub fn selected_command(&self) -> Option<CommandId> {
        self.list.selected().map(|item| item.data)
    }

    /// Apply a keymap action; returns `true` when the action was consumed.
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
            KeyAction::HistoryPrev => {
                self.list.navigate(NavigateDir::Prev);
                true
            }
            KeyAction::HistoryNext => {
                self.list.navigate(NavigateDir::Next);
                true
            }
            KeyAction::Clear => {
                self.filter.clear();
                self.list.set_filter(None);
                true
            }
            _ => false,
        }
    }

    /// Append a character to the filter.
    pub fn filter_push(&mut self, c: char) {
        self.filter.push(c);
        self.list.set_filter(Some(self.filter.as_str()));
    }

    /// Remove the last grapheme from the filter.
    pub fn filter_backspace(&mut self) {
        let trimmed: String = self
            .filter
            .graphemes(true)
            .rev()
            .skip(1)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        self.filter = trimmed;
        self.list.set_filter(if self.filter.is_empty() {
            None
        } else {
            Some(self.filter.as_str())
        });
    }

    /// The filter text typed so far.
    pub fn filter_text(&self) -> &str {
        &self.filter
    }

    /// Look a command up by its typed label (`"/model"`, leading `/`
    /// optional). Used by direct `/command` submits.
    pub fn find(&self, typed: &str) -> Option<CommandId> {
        let needle = typed.trim();
        let needle = needle.strip_prefix('/').unwrap_or(needle);
        self.entries
            .iter()
            .find(|e| e.label.strip_prefix('/').unwrap_or(e.label) == needle)
            .map(|e| e.id)
    }

    /// Number of visible (filter-passing) commands.
    pub fn visible_len(&self) -> usize {
        self.list.len()
    }

    /// Render the palette rows for the given width / window height.
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
