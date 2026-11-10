//! Shell-level command palette state: filter draft plus selection.
//!
//! The palette is a transient shell overlay. It owns its filter and cursor
//! so any screen can open it without leaving the current page and without
//! touching the session composer draft.

use tui_core::command::{self, CommandSpec, PalettePick, RuntimeCommand};

/// Shell command palette backing the `Ctrl-P` overlay.
#[derive(Debug, Default)]
pub struct CommandPalette {
    filter: String,
    selected: usize,
    plugin_commands: Vec<RuntimeCommand>,
}

impl CommandPalette {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn filter(&self) -> &str {
        &self.filter
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    pub fn push_char(&mut self, c: char) {
        self.filter.push(c);
        self.selected = 0;
    }

    pub fn pop(&mut self) {
        self.filter.pop();
        self.selected = 0;
    }

    pub fn append_paste(&mut self, text: &str) {
        if !text.is_empty() {
            self.filter.push_str(text);
            self.selected = 0;
        }
    }

    pub fn clear(&mut self) {
        self.filter.clear();
        self.selected = 0;
    }

    /// The command word under the cursor: text before the first space with
    /// a leading slash stripped. The rest is the command argument.
    pub fn command_word(&self) -> String {
        let word = match self.filter.split_once(' ') {
            Some((command, _)) => command,
            None => self.filter.as_str(),
        };
        word.trim().trim_start_matches('/').to_lowercase()
    }

    /// Argument typed after the command word.
    pub fn argument(&self) -> String {
        match self.filter.split_once(' ') {
            Some((_, argument)) => argument.trim().to_string(),
            None => String::new(),
        }
    }

    /// Commands matching the current filter in registry order.
    pub fn filtered(&self) -> Vec<&'static CommandSpec> {
        self.builtin_matches(&self.command_word())
    }

    fn builtin_matches(&self, needle: &str) -> Vec<&'static CommandSpec> {
        command::palette_commands()
            .filter(|spec| {
                if needle.is_empty() {
                    return true;
                }
                let title = spec.title.to_lowercase();
                let id = spec.id.to_lowercase();
                let description = spec.description.to_lowercase();
                if title.contains(needle) || id.contains(needle) || description.contains(needle)
                {
                    return true;
                }
                if let Some(slash) = spec.slash {
                    if slash.name.to_lowercase().contains(needle) {
                        return true;
                    }
                    if slash
                        .aliases
                        .iter()
                        .any(|alias| alias.to_lowercase().contains(needle))
                    {
                        return true;
                    }
                }
                false
            })
            .collect()
    }

    /// Replace the plugin command list (converted from engine snapshots
    /// by the loader) and reset the selection. Empty restores the
    /// builtin-only palette.
    pub fn set_plugin_commands(&mut self, commands: Vec<RuntimeCommand>) {
        self.plugin_commands = commands;
        self.selected = 0;
    }

    /// Plugin command behind a [`PalettePick::Plugin`] row.
    pub fn plugin_command(&self, index: usize) -> Option<&RuntimeCommand> {
        self.plugin_commands.get(index)
    }

    /// Builtin plus plugin rows matching the current filter: builtins in
    /// registry order first, then plugins in registration order.
    pub fn picks(&self) -> Vec<PalettePick> {
        let needle = self.command_word();
        let mut picks: Vec<PalettePick> = self
            .builtin_matches(&needle)
            .into_iter()
            .map(PalettePick::Builtin)
            .collect();
        picks.extend(
            self.plugin_commands
                .iter()
                .enumerate()
                .filter(|(_, command)| {
                    command.palette && command::plugin_matches_filter(command, &needle)
                })
                .map(|(index, _)| PalettePick::Plugin(index)),
        );
        picks
    }

    pub fn selected_pick(&self) -> Option<PalettePick> {
        self.picks().get(self.selected).copied()
    }

    pub fn move_next(&mut self) {
        let len = self.picks().len();
        if len == 0 {
            return;
        }
        self.selected = (self.selected + 1) % len;
    }

    pub fn move_prev(&mut self) {
        let len = self.picks().len();
        if len == 0 {
            return;
        }
        if self.selected == 0 {
            self.selected = len - 1;
        } else {
            self.selected -= 1;
        }
    }

    pub fn page_down(&mut self) {
        let len = self.picks().len();
        if len == 0 {
            return;
        }
        self.selected = (self.selected + 10).min(len - 1);
    }

    pub fn page_up(&mut self) {
        if self.picks().is_empty() {
            return;
        }
        self.selected = self.selected.saturating_sub(10);
    }

    /// Focus one filtered row from a pointer hit. Out-of-range rows keep
    /// the current selection so stale hits stay inert.
    pub fn select(&mut self, index: usize) {
        let len = self.picks().len();
        if index < len {
            self.selected = index;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_filter_lists_every_palette_command() {
        let palette = CommandPalette::new();
        assert!(!palette.filtered().is_empty());
        assert_eq!(palette.selected(), 0);
        assert!(palette.selected_pick().is_some());
    }

    #[test]
    fn filter_matches_slash_name_title_and_id() {
        let mut palette = CommandPalette::new();
        palette.push_char('m');
        palette.push_char('o');
        palette.push_char('d');
        palette.push_char('e');
        palette.push_char('l');
        let filtered = palette.filtered();
        assert!(filtered.iter().any(|spec| spec.id == "session.model"));
        assert!(matches!(
            palette.selected_pick(),
            Some(PalettePick::Builtin(spec)) if spec.id == "session.model"
        ));
    }

    #[test]
    fn command_word_and_argument_split_on_first_space() {
        let mut palette = CommandPalette::new();
        for c in "/copy message".chars() {
            palette.push_char(c);
        }
        assert_eq!(palette.command_word(), "copy");
        assert_eq!(palette.argument(), "message");
    }

    #[test]
    fn navigation_wraps_and_pages() {
        let mut palette = CommandPalette::new();
        let len = palette.filtered().len();
        assert!(len > 1);
        palette.move_prev();
        assert_eq!(palette.selected(), len - 1);
        palette.move_next();
        assert_eq!(palette.selected(), 0);
        palette.page_down();
        assert_eq!(palette.selected(), 10.min(len - 1));
        palette.page_up();
        assert_eq!(palette.selected(), 0);
    }

    #[test]
    fn typing_resets_selection() {
        let mut palette = CommandPalette::new();
        palette.move_next();
        palette.move_next();
        palette.push_char('z');
        assert_eq!(palette.selected(), 0);
    }

    #[test]
    fn pointer_focus_clamps_to_filtered_rows() {
        let mut palette = CommandPalette::new();
        let len = palette.picks().len();
        palette.select(1.min(len - 1));
        assert_eq!(palette.selected(), 1.min(len - 1));
        palette.select(len + 10);
        assert_eq!(palette.selected(), 1.min(len - 1));
    }

    fn plugin_review() -> RuntimeCommand {
        RuntimeCommand {
            id: "review.run".to_string(),
            title: "Run review".to_string(),
            group: "Review".to_string(),
            description: "Review the current diff".to_string(),
            slash_name: Some("review".to_string()),
            slash_aliases: vec![],
            slash_argument: "[path]".to_string(),
            palette: true,
            key_hint: None,
        }
    }

    #[test]
    fn plugin_rows_join_the_palette_after_builtins() {
        let mut palette = CommandPalette::new();
        let builtin_len = palette.picks().len();
        palette.set_plugin_commands(vec![plugin_review()]);
        let picks = palette.picks();
        assert_eq!(picks.len(), builtin_len + 1);
        assert!(matches!(picks[builtin_len], PalettePick::Plugin(0)));
        for c in "review".chars() {
            palette.push_char(c);
        }
        let picks = palette.picks();
        assert!(picks
            .iter()
            .any(|pick| matches!(pick, PalettePick::Plugin(0))));
        assert_eq!(
            palette.plugin_command(0).map(|command| command.id.as_str()),
            Some("review.run")
        );
    }
}
