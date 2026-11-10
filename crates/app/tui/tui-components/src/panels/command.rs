//! The `/` command palette: a view over the command registry with typed
//! filtering and keymap-driven navigation.
//!
//! Plugin commands ride alongside the builtin rows for direct `/name`
//! submits: the panel resolves them through its own plugin list and
//! hands the owned definition to the shell, which reports the routing.
//! Execution dispatch stays server-side through the host command protocol.

use crate::panels::filter::{apply, PanelFilter};
use crate::select::{Group, GroupItem, NavigateDir, SelectList};
use tui_core::command::{self, CommandSpec, RuntimeCommand};
use tui_core::keymap::KeyAction;

/// The `/` command palette.
#[derive(Debug, Clone)]
pub struct CommandPalette {
    list: SelectList<&'static CommandSpec>,
    /// Filter text typed while the palette is open.
    filter: PanelFilter,
    /// Plugin commands resolved for direct `/name` submits.
    plugin_commands: Vec<RuntimeCommand>,
}

impl Default for CommandPalette {
    fn default() -> Self {
        Self::new()
    }
}

impl CommandPalette {
    pub fn new() -> Self {
        Self::with_filter("")
    }

    /// Palette commands narrowed by `filter`.
    pub fn with_filter(filter: &str) -> Self {
        let list = Self::build_list();
        let mut panel = Self {
            list,
            filter: PanelFilter::new(),
            plugin_commands: Vec::new(),
        };
        panel.filter.set(filter);
        panel.apply_filter();
        panel
    }

    /// The command part of the filter: everything before the first space.
    /// The rest is the command argument.
    fn command_filter(&self) -> &str {
        let text = self.filter.text();
        match text.split_once(' ') {
            Some((command, _)) => command,
            None => text,
        }
    }

    /// The argument typed after the command word.
    pub fn argument(&self) -> &str {
        match self.filter.text().split_once(' ') {
            Some((_, argument)) => argument.trim(),
            None => "",
        }
    }

    /// Project the command part of the filter onto the list.
    fn apply_filter(&mut self) {
        let command = self.command_filter().to_string();
        apply(&mut self.list, &command);
    }

    /// Registry order grouped by command group.
    fn build_list() -> SelectList<&'static CommandSpec> {
        let mut groups: Vec<Group<&'static CommandSpec>> = Vec::new();
        for spec in command::palette_commands() {
            let position = groups
                .iter()
                .position(|group| group.title.as_deref() == Some(spec.group));
            let index = if let Some(index) = position {
                index
            } else {
                groups.push(Group::new(Some(spec.group)));
                groups.len() - 1
            };
            let mut label = match &spec.slash {
                Some(slash) => format!("/{}", slash.name),
                None => format!("/{}", spec.id),
            };
            if let Some(slash) = &spec.slash {
                if !slash.argument.is_empty() {
                    label.push(' ');
                    label.push_str(slash.argument);
                }
            }
            groups[index]
                .items
                .push(GroupItem::new(label, spec).described(spec.description));
        }
        SelectList::groups(groups).with_group_titles(true)
    }

    /// Replace the plugin command list (converted from engine snapshots
    /// by the shell) without touching the typed filter.
    pub fn set_plugin_commands(&mut self, commands: Vec<RuntimeCommand>) {
        self.plugin_commands = commands;
    }

    /// Plugin command behind a typed slash name or alias (`/review`,
    /// leading slash optional, ASCII case-insensitive).
    pub fn find_plugin(&self, typed: &str) -> Option<RuntimeCommand> {
        let needle = typed.trim().trim_start_matches('/');
        if needle.is_empty() {
            return None;
        }
        self.plugin_commands
            .iter()
            .find(|command| {
                command
                    .slash_name
                    .as_deref()
                    .is_some_and(|name| name.eq_ignore_ascii_case(needle))
                    || command
                        .slash_aliases
                        .iter()
                        .any(|alias| alias.eq_ignore_ascii_case(needle))
            })
            .cloned()
    }

    /// Plugin command matching the filter's command word, with the typed
    /// argument. Used when no builtin row is selected.
    pub fn resolve_plugin(&self) -> Option<(RuntimeCommand, String)> {
        let word = match self.filter.text().split_once(' ') {
            Some((command, _)) => command,
            None => self.filter.text(),
        };
        self.find_plugin(word).map(|command| {
            let argument = self.argument().to_string();
            (command, argument)
        })
    }

    /// The command highlighted by the cursor.
    pub fn selected_command(&self) -> Option<&'static CommandSpec> {
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
                self.apply_filter();
                true
            }
            _ => false,
        }
    }

    /// Append a character to the filter.
    pub fn filter_push(&mut self, c: char) {
        self.filter.push(c);
        self.apply_filter();
    }

    /// Remove the last grapheme from the filter.
    pub fn filter_backspace(&mut self) {
        self.filter.backspace();
        self.apply_filter();
    }

    /// The filter text typed so far.
    pub fn filter_text(&self) -> &str {
        self.filter.text()
    }

    /// Look a command up by its typed slash name (`"/model"`, leading `/`
    /// optional). Used by direct `/command` submits.
    pub fn find(&self, typed: &str) -> Option<&'static CommandSpec> {
        command::lookup_slash(typed)
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
