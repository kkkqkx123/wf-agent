//! Searchable command palette component.

use std::collections::BTreeSet;

/// A command entry in a palette.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandItem {
    pub id: String,
    pub title: String,
    pub description: String,
    pub shortcut: Option<String>,
    pub context: Option<String>,
}

impl CommandItem {
    pub fn new(id: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            description: String::new(),
            shortcut: None,
            context: None,
        }
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    pub fn with_shortcut(mut self, shortcut: impl Into<String>) -> Self {
        self.shortcut = Some(shortcut.into());
        self
    }

    pub fn with_context(mut self, context: impl Into<String>) -> Self {
        self.context = Some(context.into());
        self
    }
}

/// A searchable, grouped command palette.
#[derive(Debug, Clone)]
pub struct CommandPalette {
    commands: Vec<CommandItem>,
    query: String,
    selected: usize,
}

impl CommandPalette {
    pub fn new(commands: Vec<CommandItem>) -> Self {
        Self {
            commands,
            query: String::new(),
            selected: 0,
        }
    }

    pub fn set_query(&mut self, query: impl Into<String>) {
        self.query = query.into();
        if self.selected >= self.filtered().len() {
            self.selected = 0;
        }
    }

    pub fn query(&self) -> &str {
        &self.query
    }

    pub fn filtered(&self) -> Vec<CommandItem> {
        let query = self.query.to_lowercase();
        self.commands
            .iter()
            .filter(|item| {
                if query.is_empty() {
                    return true;
                }
                item.title.to_lowercase().contains(&query)
                    || item.description.to_lowercase().contains(&query)
                    || item.id.to_lowercase().contains(&query)
            })
            .cloned()
            .collect()
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

    pub fn selected_item(&self) -> Option<CommandItem> {
        self.filtered().get(self.selected).cloned()
    }

    pub fn groups(&self) -> Vec<String> {
        let mut groups: BTreeSet<String> = BTreeSet::new();
        for item in self.filtered() {
            if let Some(context) = item.context {
                groups.insert(context);
            }
        }
        groups.into_iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filters_by_title_or_description() {
        let palette = CommandPalette::new(vec![
            CommandItem::new("open", "Open file").with_description("Open a file"),
            CommandItem::new("quit", "Quit").with_description("Exit app"),
        ]);
        let mut palette = palette;
        palette.set_query("file");
        assert_eq!(palette.filtered().len(), 1);
        assert_eq!(palette.selected_item().unwrap().id, "open");
    }

    #[test]
    fn group_context_is_sorted() {
        let palette = CommandPalette::new(vec![
            CommandItem::new("a", "A").with_context("z"),
            CommandItem::new("b", "B").with_context("a"),
        ]);
        assert_eq!(palette.groups(), vec!["a", "z"]);
    }

    #[test]
    fn navigation_wraps() {
        let mut palette =
            CommandPalette::new(vec![CommandItem::new("a", "A"), CommandItem::new("b", "B")]);
        palette.move_prev();
        assert_eq!(palette.selected_item().unwrap().id, "b");
    }
}
