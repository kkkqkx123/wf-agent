//! Predefined utility tools: definitions + handler.
//!
//! Tools: update_todo_list.

use serde_json::Value;
use std::sync::Arc;

use wf_types::tool::ToolType;

use super::schema::{ToolDefinition, ToolParameter};
use crate::error::{ToolError, ToolResult};
use crate::executor::StatelessHandler;
use crate::registry::ToolRegistry;

pub static UPDATE_TODO_LIST: ToolDefinition = ToolDefinition {
    id: "update_todo_list",
    tool_type: ToolType::Stateless,
    category: "utility",
    tags: &["todo", "list"],
    description: "Update the current todo list with markdown-formatted tasks. Supports [ ], [-], [x] statuses for pending, in-progress and completed items.",
    parameters: &[
        ToolParameter { name: "todos", r#type: "string", required: true, description: "Markdown-formatted todo list with [ ], [-], [x] prefixes", default_json: None },
    ],
    tips: None,
    examples: None,
};

/// All utility tool definitions in registration order.
pub const ALL: &[&ToolDefinition] = &[&UPDATE_TODO_LIST];

/// Parse the markdown checklist format into items with statuses. Lines may
/// optionally start with a list marker (`- ` or `* `) before the checkbox.
fn parse_todo_list(todos: &str) -> Vec<(String, String)> {
    let mut items = Vec::new();
    for line in todos.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        // Optional markdown list marker before the checkbox.
        let trimmed = trimmed
            .strip_prefix("- ")
            .or_else(|| trimmed.strip_prefix("* "))
            .unwrap_or(trimmed);
        let Some(rest) = trimmed.strip_prefix('[') else { continue };
        let Some(status_char) = rest.chars().next() else { continue };
        if !matches!(status_char, ' ' | '-' | 'x') {
            continue;
        }
        let Some(content) = rest
            .strip_prefix(status_char)
            .and_then(|r| r.strip_prefix("]"))
        else {
            continue;
        };
        let content = content.trim_start();
        if content.is_empty() {
            continue;
        }
        let status = match status_char {
            'x' => "completed",
            '-' => "in_progress",
            _ => "pending",
        };
        items.push((content.to_string(), status.to_string()));
    }
    items
}

/// Create the update_todo_list handler.
pub fn update_todo_list_handler() -> StatelessHandler {
    Arc::new(|parameters: &Value, _ctx| {
        let todos = parameters
            .get("todos")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                ToolError::ValidationFailed("Missing or invalid 'todos' parameter".into())
            })?;

        let items = parse_todo_list(todos);
        if items.is_empty() {
            return Err(ToolError::ValidationFailed(
                "No valid todo items found. Use format: [ ] pending, [x] completed, [-] in progress"
                    .into(),
            ));
        }

        let pending = items.iter().filter(|i| i.1 == "pending").count();
        let in_progress = items.iter().filter(|i| i.1 == "in_progress").count();
        let completed = items.iter().filter(|i| i.1 == "completed").count();

        let mut display = String::from("TODO list updated:\n");
        for (content, status) in &items {
            let checkbox = match status.as_str() {
                "completed" => "[x]",
                "in_progress" => "[-]",
                _ => "[ ]",
            };
            display.push_str(&format!("{} {}\n", checkbox, content));
        }
        display.push_str(&format!(
            "\nSummary: {} pending, {} in progress, {} completed",
            pending, in_progress, completed
        ));

        Ok(serde_json::json!({
            "items": items.iter().map(|(content, status)| serde_json::json!({
                "content": content,
                "status": status,
            })).collect::<Vec<_>>(),
            "total": items.len(),
            "pending": pending,
            "in_progress": in_progress,
            "completed": completed,
            "display": display.trim_end(),
        }))
    })
}

/// Register the update_todo_list handler into the registry.
pub fn register(registry: &ToolRegistry) -> ToolResult<()> {
    registry.register_stateless_handler("update_todo_list", update_todo_list_handler());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_todo_list() {
        let items = parse_todo_list("- [ ] pending task\n- [-] in progress task\n- [x] completed task\n");
        assert_eq!(items.len(), 3);
        assert_eq!(items[0], ("pending task".into(), "pending".into()));
        assert_eq!(items[1], ("in progress task".into(), "in_progress".into()));
        assert_eq!(items[2], ("completed task".into(), "completed".into()));
    }

    #[test]
    fn test_parse_todo_list_invalid_lines() {
        let items = parse_todo_list("not a todo\n- [z] bad status\n\n- [x] valid\n");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].1, "completed");
    }
}
