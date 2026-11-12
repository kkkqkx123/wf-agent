use std::collections::HashMap;

use serde_json::Value;

use super::variables::VariableSource;

/// Build the volatile tail text. Time comes from the local clock; todo,
/// pinned, workspace tree and custom data come from variables. Enabled but
/// missing or misshapen values warn and skip; the engine never fabricates
/// data. Returns `None` when nothing is enabled or nothing has data.
pub fn build_volatile_tail(
    agent_config: Option<&wf_types::agent::AgentConfig>,
    vars: &impl VariableSource,
) -> Option<String> {
    let dyn_cfg = agent_config?.dynamic_context.as_ref()?;
    let wants_time = dyn_cfg.include_current_time.unwrap_or(false);
    let wants_todo = dyn_cfg.include_todo_list.unwrap_or(false);
    let wants_workspace = dyn_cfg.include_workspace_files.unwrap_or(false);
    let wants_pinned = dyn_cfg.include_pinned_files.unwrap_or(false);
    let wants_custom = vars
        .get_variable(wf_types::dynamic_context::CUSTOM_DATA_VARIABLE)
        .is_some();
    if !wants_time && !wants_todo && !wants_workspace && !wants_pinned && !wants_custom {
        return None;
    }

    let mut input = wf_resource::UserInput::default();
    if wants_time {
        input.current_time = Some(wf_resource::current_time_text());
    }
    if wants_todo {
        match vars.get_variable(wf_types::dynamic_context::TODO_LIST_VARIABLE) {
            Some(value) => match parse_todo_items(value) {
                Some(items) => input.todos = items,
                None => tracing::warn!("todo_list variable has an unsupported shape; skipped"),
            },
            None => tracing::warn!("todo_list requested but no todo_list variable is set; skipped"),
        }
    }
    if wants_pinned {
        match vars.get_variable(wf_types::dynamic_context::PINNED_FILES_VARIABLE) {
            Some(value) => {
                if let Ok(items) =
                    serde_json::from_value::<Vec<wf_types::PinnedFileItem>>(value.clone())
                {
                    input.pinned = items
                        .into_iter()
                        .map(|item| std::path::PathBuf::from(item.path))
                        .collect();
                } else if let Ok(paths) = serde_json::from_value::<Vec<String>>(value) {
                    input.pinned = paths.into_iter().map(std::path::PathBuf::from).collect();
                } else {
                    tracing::warn!("pinned_files variable has an unsupported shape; skipped");
                }
            }
            None => {
                tracing::warn!(
                    "pinned_files requested but no pinned_files variable is set; skipped"
                )
            }
        }
    }
    if wants_workspace {
        match vars.get_variable(wf_types::dynamic_context::WORKSPACE_FILE_TREE_VARIABLE) {
            Some(Value::String(tree)) => input.tree = Some(tree),
            Some(_) => tracing::warn!("workspace_file_tree variable is not a string; skipped"),
            None => {
                tracing::warn!(
                    "workspace files requested but no workspace_file_tree variable is set; skipped"
                )
            }
        }
    }
    if let Some(value) = vars.get_variable(wf_types::dynamic_context::CUSTOM_DATA_VARIABLE) {
        match parse_custom_data(value) {
            Some(map) if !map.is_empty() => input.custom_data = Some(map),
            Some(_) => {}
            None => tracing::warn!("custom_data variable has an unsupported shape; skipped"),
        }
    }

    let rendered = wf_resource::build_user_context(&input);
    if rendered.is_empty() {
        None
    } else {
        Some(rendered)
    }
}

/// Minimal todo display shape: full [`wf_types::TodoItem`] items win, but a
/// bare `{content, status}` object without `id` also renders. Status strings
/// follow the `update_todo_list` tool vocabulary. Returns `None` when the
/// value is not a todo array at all.
fn parse_todo_items(value: Value) -> Option<Vec<wf_types::TodoItem>> {
    if let Ok(items) = serde_json::from_value::<Vec<wf_types::TodoItem>>(value.clone()) {
        return Some(items);
    }
    let raw = value.as_array()?;
    if raw.is_empty() {
        return Some(Vec::new());
    }
    let mut items = Vec::with_capacity(raw.len());
    for entry in raw {
        let content = entry
            .get("content")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())?;
        let status_str = entry
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("pending");
        let status = match status_str.to_ascii_lowercase().as_str() {
            "pending" => wf_types::TodoStatus::Pending,
            "in_progress" | "inprogress" | "in-progress" | "doing" => {
                wf_types::TodoStatus::InProgress
            }
            "completed" | "complete" | "done" => wf_types::TodoStatus::Completed,
            "cancelled" | "canceled" => wf_types::TodoStatus::Cancelled,
            _ => return None,
        };
        let id = entry
            .get("id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(wf_common::generate_id);
        items.push(wf_types::TodoItem {
            id,
            content: content.to_string(),
            status,
            priority: None,
            created_at: None,
            updated_at: None,
            metadata: None,
        });
    }
    Some(items)
}

fn parse_custom_data(value: Value) -> Option<HashMap<String, String>> {
    let obj = value.as_object()?;
    let mut map = HashMap::new();
    for (k, v) in obj {
        let rendered = wf_common::template::value_to_display_string(v);
        if !rendered.trim().is_empty() {
            map.insert(k.clone(), rendered);
        }
    }
    Some(map)
}
