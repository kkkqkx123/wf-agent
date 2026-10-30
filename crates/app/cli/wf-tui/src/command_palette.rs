//! Shell-level command palette state: filter draft plus selection.
//!
//! The palette is a transient shell overlay. It owns its filter and cursor
//! so any screen can open it without leaving the current page and without
//! touching the session composer draft.

use tui_core::command::{self, CommandSpec};

/// Shell command palette backing the `Ctrl-P` overlay.
#[derive(Debug, Default)]
pub struct CommandPalette {
    filter: String,
    selected: usize,
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
        let needle = self.command_word();
        command::palette_commands()
            .filter(|spec| {
                if needle.is_empty() {
                    return true;
                }
                let title = spec.title.to_lowercase();
                let id = spec.id.to_lowercase();
                let description = spec.description.to_lowercase();
                if title.contains(&needle) || id.contains(&needle) || description.contains(&needle)
                {
                    return true;
                }
                if let Some(slash) = spec.slash {
                    if slash.name.to_lowercase().contains(&needle) {
                        return true;
                    }
                    if slash
                        .aliases
                        .iter()
                        .any(|alias| alias.to_lowercase().contains(&needle))
                    {
                        return true;
                    }
                }
                false
            })
            .collect()
    }

    pub fn selected_command(&self) -> Option<&'static CommandSpec> {
        let filtered = self.filtered();
        filtered.get(self.selected).copied()
    }

    pub fn move_next(&mut self) {
        let len = self.filtered().len();
        if len == 0 {
            return;
        }
        self.selected = (self.selected + 1) % len;
    }

    pub fn move_prev(&mut self) {
        let len = self.filtered().len();
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
        let len = self.filtered().len();
        if len == 0 {
            return;
        }
        self.selected = (self.selected + 10).min(len - 1);
    }

    pub fn page_up(&mut self) {
        if self.filtered().is_empty() {
            return;
        }
        self.selected = self.selected.saturating_sub(10);
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
        assert!(palette.selected_command().is_some());
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
        assert_eq!(palette.selected_command().unwrap().id, "session.model");
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
}
