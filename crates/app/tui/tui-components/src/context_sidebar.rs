//! Context sidebar panels for the session workspace.

/// One context sidebar section.
#[derive(Debug, Clone, Default)]
pub struct ContextSection {
    pub title: String,
    pub lines: Vec<String>,
}

impl ContextSection {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            lines: Vec::new(),
        }
    }

    pub fn with_lines(title: impl Into<String>, lines: Vec<String>) -> Self {
        Self {
            title: title.into(),
            lines,
        }
    }

    pub fn push(&mut self, line: impl Into<String>) {
        self.lines.push(line.into());
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }
}

/// Session-workspace context sidebar combining several panels.
#[derive(Debug, Clone, Default)]
pub struct ContextSidebar {
    pub files: ContextSection,
    pub todos: ContextSection,
    pub context: ContextSection,
    pub mcp: ContextSection,
    pub git: ContextSection,
}

impl ContextSidebar {
    pub fn new() -> Self {
        let mut sidebar = Self::default();
        sidebar.files.title = "Files".to_string();
        sidebar.todos.title = "Todos".to_string();
        sidebar.context.title = "Context".to_string();
        sidebar.mcp.title = "MCP".to_string();
        sidebar.git.title = "Git".to_string();
        sidebar
    }

    /// All non-empty sections in stable order.
    pub fn visible_sections(&self) -> Vec<&ContextSection> {
        [
            &self.files,
            &self.todos,
            &self.context,
            &self.mcp,
            &self.git,
        ]
        .into_iter()
        .filter(|section| !section.is_empty())
        .collect()
    }

    /// Render the sidebar to plain lines for tests and headless use.
    pub fn render_lines(&self) -> Vec<String> {
        let mut out = Vec::new();
        for section in self.visible_sections() {
            out.push(format!("## {}", section.title));
            for line in &section.lines {
                out.push(format!("- {line}"));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_sections_are_hidden() {
        let mut sidebar = ContextSidebar::new();
        sidebar.files.push("main.rs".to_string());
        sidebar.git.push("clean".to_string());
        let lines = sidebar.render_lines();
        assert_eq!(lines, vec!["## Files", "- main.rs", "## Git", "- clean"]);
    }

    #[test]
    fn sections_preserve_order() {
        assert_eq!(ContextSidebar::new().visible_sections().len(), 0);
    }
}
