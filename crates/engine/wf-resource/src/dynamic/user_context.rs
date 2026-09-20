use std::collections::HashMap;
use std::path::PathBuf;

use wf_types::TodoItem;

#[derive(Debug, Clone, Default)]
pub struct UserInput {
    pub todos: Vec<TodoItem>,
    pub pinned: Vec<PathBuf>,
    pub tree: Option<String>,
    pub custom_data: Option<HashMap<String, String>>,
}

pub fn build_user_context(input: &UserInput) -> String {
    let mut sections: Vec<String> = Vec::new();

    if !input.todos.is_empty() {
        let mut todo_lines: Vec<String> = Vec::new();
        todo_lines.push("TODO list:".into());
        for item in &input.todos {
            let status = match item.status {
                wf_types::TodoStatus::Pending => "[ ]",
                wf_types::TodoStatus::InProgress => "[~]",
                wf_types::TodoStatus::Completed => "[x]",
                wf_types::TodoStatus::Cancelled => "[-]",
            };
            todo_lines.push(format!("  {} {}", status, item.content));
        }
        sections.push(todo_lines.join("\n"));
    }

    if !input.pinned.is_empty() {
        let mut pinned_lines: Vec<String> = Vec::new();
        pinned_lines.push("Pinned files:".into());
        for path in &input.pinned {
            pinned_lines.push(format!("  - {}", path.display()));
        }
        sections.push(pinned_lines.join("\n"));
    }

    if let Some(ref tree) = input.tree {
        sections.push(format!("Workspace structure:\n{}", tree));
    }

    if let Some(ref custom) = input.custom_data {
        for (key, value) in custom {
            sections.push(format!("{}: {}", key, value));
        }
    }

    sections.join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_context_empty() {
        let input = UserInput::default();
        let ctx = build_user_context(&input);
        assert_eq!(ctx, "");
    }

    #[test]
    fn test_user_context_with_todos() {
        let input = UserInput {
            todos: vec![
                TodoItem {
                    id: "1".into(),
                    content: "Task one".into(),
                    status: wf_types::TodoStatus::Pending,
                    priority: None,
                    created_at: None,
                    updated_at: None,
                    metadata: None,
                },
                TodoItem {
                    id: "2".into(),
                    content: "Task two".into(),
                    status: wf_types::TodoStatus::Completed,
                    priority: None,
                    created_at: None,
                    updated_at: None,
                    metadata: None,
                },
            ],
            pinned: Vec::new(),
            tree: None,
            custom_data: None,
        };
        let ctx = build_user_context(&input);
        assert!(ctx.contains("TODO list:"));
        assert!(ctx.contains("[ ] Task one"));
        assert!(ctx.contains("[x] Task two"));
    }

    #[test]
    fn test_user_context_with_pinned() {
        let input = UserInput {
            todos: Vec::new(),
            pinned: vec![PathBuf::from("/home/user/project/src/main.rs")],
            tree: None,
            custom_data: None,
        };
        let ctx = build_user_context(&input);
        assert!(ctx.contains("Pinned files:"));
        assert!(ctx.contains("main.rs"));
    }

    #[test]
    fn test_user_context_with_tree() {
        let input = UserInput {
            todos: Vec::new(),
            pinned: Vec::new(),
            tree: Some("src/\n  main.rs\n  lib.rs".into()),
            custom_data: None,
        };
        let ctx = build_user_context(&input);
        assert!(ctx.contains("Workspace structure:"));
        assert!(ctx.contains("main.rs"));
    }

    #[test]
    fn test_user_context_with_custom_data() {
        let input = UserInput {
            todos: Vec::new(),
            pinned: Vec::new(),
            tree: None,
            custom_data: Some(HashMap::from([("project".into(), "wf-agent".into())])),
        };
        let ctx = build_user_context(&input);
        assert!(ctx.contains("project: wf-agent"));
    }
}
