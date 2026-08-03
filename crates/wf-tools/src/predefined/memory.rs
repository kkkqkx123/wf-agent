//! Predefined memory tools: definitions + in-memory note/memory stores.
//!
//! Session notes (record_note / recall_notes / list_categories) are scoped
//! per execution. Long-term memory (memory_remember / memory_forget /
//! memory_list) is shared across executions; these are kept as deprecated
//! aliases.

use dashmap::DashMap;
use serde_json::Value;
use std::sync::Arc;

use wf_types::tool::ToolType;

use super::schema::{ToolDefinition, ToolParameter};
use crate::error::{ToolError, ToolResult};
use crate::executor::StatefulInstance;
use crate::registry::ToolRegistry;

pub static RECORD_NOTE: ToolDefinition = ToolDefinition {
    id: "record_note",
    tool_type: ToolType::Stateful,
    category: "memory",
    tags: &["note", "session"],
    description: "Record a note in memory with an optional category. Notes can be recalled later within the same execution.",
    parameters: &[
        ToolParameter { name: "note", r#type: "string", required: true, description: "The note content to record", default_json: None },
        ToolParameter { name: "category", r#type: "string", required: false, description: "Optional category label for the note", default_json: None },
    ],
    tips: None,
    examples: Some(&["record_note(\"User prefers dark mode\", \"preferences\")"]),
};

pub static RECALL_NOTES: ToolDefinition = ToolDefinition {
    id: "recall_notes",
    tool_type: ToolType::Stateful,
    category: "memory",
    tags: &["note", "recall"],
    description: "Recall previously recorded session notes. Filters by optional search term and category.",
    parameters: &[
        ToolParameter { name: "search", r#type: "string", required: false, description: "Optional search term to filter notes", default_json: None },
        ToolParameter { name: "category", r#type: "string", required: false, description: "Optional category to filter by", default_json: None },
        ToolParameter { name: "limit", r#type: "number", required: false, description: "Maximum number of notes to return", default_json: None },
    ],
    tips: None,
    examples: Some(&["recall_notes(\"preferences\")"]),
};

pub static LIST_CATEGORIES: ToolDefinition = ToolDefinition {
    id: "list_categories",
    tool_type: ToolType::Stateful,
    category: "memory",
    tags: &["category"],
    description: "List all note categories with note counts.",
    parameters: &[],
    tips: None,
    examples: None,
};

pub static MEMORY_REMEMBER: ToolDefinition = ToolDefinition {
    id: "memory_remember",
    tool_type: ToolType::Stateful,
    category: "memory",
    tags: &["remember"],
    description: "Store a piece of information in long-term memory for later recall across sessions.",
    parameters: &[
        ToolParameter { name: "key", r#type: "string", required: true, description: "The memory key", default_json: None },
        ToolParameter { name: "content", r#type: "string", required: true, description: "The content to remember", default_json: None },
    ],
    tips: None,
    examples: Some(&["memory_remember(\"user_name\", \"Alice\")"]),
};

pub static MEMORY_FORGET: ToolDefinition = ToolDefinition {
    id: "memory_forget",
    tool_type: ToolType::Stateful,
    category: "memory",
    tags: &["forget"],
    description: "Remove a specific piece of information from long-term memory.",
    parameters: &[
        ToolParameter { name: "key", r#type: "string", required: true, description: "The memory key to forget", default_json: None },
    ],
    tips: None,
    examples: Some(&["memory_forget(\"user_name\")"]),
};

pub static MEMORY_LIST: ToolDefinition = ToolDefinition {
    id: "memory_list",
    tool_type: ToolType::Stateful,
    category: "memory",
    tags: &["list"],
    description: "List all stored memories. Optionally filter by prefix.",
    parameters: &[
        ToolParameter { name: "prefix", r#type: "string", required: false, description: "Optional prefix to filter by", default_json: None },
    ],
    tips: None,
    examples: Some(&["memory_list()"]),
};

/// All memory tool definitions in registration order.
pub const ALL: &[&ToolDefinition] = &[
    &RECORD_NOTE,
    &RECALL_NOTES,
    &LIST_CATEGORIES,
    &MEMORY_REMEMBER,
    &MEMORY_FORGET,
    &MEMORY_LIST,
];

// ── Session notes (per execution) ──────────────────────────

#[derive(Debug, Clone)]
struct SessionNote {
    note: String,
    category: Option<String>,
    timestamp: i64,
}

type NoteMap = DashMap<String, Vec<SessionNote>>;

struct RecordNoteInstance {
    notes: Arc<NoteMap>,
    execution_id: String,
}

impl StatefulInstance for RecordNoteInstance {
    fn execute(&self, params: &Value) -> ToolResult<Value> {
        let note = params
            .get("note")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                ToolError::ValidationFailed("Missing or invalid 'note' parameter".into())
            })?
            .to_string();
        let category = params
            .get("category")
            .and_then(|v| v.as_str())
            .map(String::from);

        self.notes.entry(self.execution_id.clone()).or_default().push(SessionNote {
            note: note.clone(),
            category,
            timestamp: wf_common::time::now(),
        });

        Ok(serde_json::json!({
            "recorded": true,
            "note": note,
            "execution_id": self.execution_id,
        }))
    }
}

struct RecallNotesInstance {
    notes: Arc<NoteMap>,
    execution_id: String,
}

impl StatefulInstance for RecallNotesInstance {
    fn execute(&self, params: &Value) -> ToolResult<Value> {
        let search = params
            .get("search")
            .and_then(|v| v.as_str())
            .map(|s| s.to_lowercase())
            .filter(|s| !s.is_empty());
        let category = params
            .get("category")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty());
        let limit = params
            .get("limit")
            .and_then(|v| v.as_u64())
            .unwrap_or(50) as usize;

        let all = self.notes.get(&self.execution_id).map(|e| e.clone()).unwrap_or_default();
        let mut matched: Vec<&SessionNote> = all
            .iter()
            .filter(|n| {
                category.map(|c| n.category.as_deref() == Some(c)).unwrap_or(true)
                    && search
                        .as_ref()
                        .map(|s| n.note.to_lowercase().contains(s.as_str()))
                        .unwrap_or(true)
            })
            .collect();
        matched.sort_by_key(|n| -n.timestamp);

        let items: Vec<Value> = matched
            .iter()
            .take(limit)
            .map(|n| {
                serde_json::json!({
                    "note": n.note,
                    "category": n.category,
                    "timestamp": wf_common::time::timestamp_to_iso(n.timestamp),
                })
            })
            .collect();

        Ok(serde_json::json!({
            "notes": items,
            "total": matched.len(),
            "returned": items.len(),
        }))
    }
}

struct ListCategoriesInstance {
    notes: Arc<NoteMap>,
    execution_id: String,
}

impl StatefulInstance for ListCategoriesInstance {
    fn execute(&self, _params: &Value) -> ToolResult<Value> {
        let all = self.notes.get(&self.execution_id).map(|e| e.clone()).unwrap_or_default();
        let mut counts: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
        for note in &all {
            let key = note.category.clone().unwrap_or_else(|| "general".into());
            *counts.entry(key).or_default() += 1;
        }
        let categories: Vec<Value> = counts
            .into_iter()
            .map(|(name, count)| serde_json::json!({ "category": name, "count": count }))
            .collect();

        Ok(serde_json::json!({ "categories": categories, "total": categories.len() }))
    }
}

// ── Long-term memory (shared across executions) ────────────

type MemoryMap = DashMap<String, Value>;

struct MemoryRememberInstance {
    memory: Arc<MemoryMap>,
}

impl StatefulInstance for MemoryRememberInstance {
    fn execute(&self, params: &Value) -> ToolResult<Value> {
        let key = params
            .get("key")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| ToolError::ValidationFailed("Missing or invalid 'key' parameter".into()))?
            .to_string();
        let content = params
            .get("content")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                ToolError::ValidationFailed("Missing or invalid 'content' parameter".into())
            })?;

        self.memory.insert(key.clone(), Value::String(content.into()));
        Ok(serde_json::json!({ "remembered": true, "key": key }))
    }
}

struct MemoryForgetInstance {
    memory: Arc<MemoryMap>,
}

impl StatefulInstance for MemoryForgetInstance {
    fn execute(&self, params: &Value) -> ToolResult<Value> {
        let key = params
            .get("key")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| ToolError::ValidationFailed("Missing or invalid 'key' parameter".into()))?;
        let removed = self.memory.remove(key).is_some();
        Ok(serde_json::json!({ "removed": removed, "key": key }))
    }
}

struct MemoryListInstance {
    memory: Arc<MemoryMap>,
}

impl StatefulInstance for MemoryListInstance {
    fn execute(&self, params: &Value) -> ToolResult<Value> {
        let prefix = params
            .get("prefix")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .unwrap_or("");

        let mut entries: Vec<Value> = self
            .memory
            .iter()
            .filter(|e| e.key().starts_with(prefix))
            .map(|e| serde_json::json!({ "key": e.key(), "content": e.value() }))
            .collect();
        entries.sort_by_key(|e| e["key"].as_str().unwrap_or("").to_string());

        Ok(serde_json::json!({ "memories": entries, "total": entries.len() }))
    }
}

/// Register the memory stateful factories into the registry.
pub fn register(registry: &ToolRegistry) -> ToolResult<()> {
    let notes: Arc<NoteMap> = Arc::new(DashMap::new());
    let memory: Arc<MemoryMap> = Arc::new(DashMap::new());

    let record_store = notes.clone();
    registry.register_stateful_factory("record_note", Arc::new(move |execution_id| {
        Box::new(RecordNoteInstance {
            notes: record_store.clone(),
            execution_id: execution_id.to_string(),
        })
    }));

    let recall_store = notes.clone();
    registry.register_stateful_factory("recall_notes", Arc::new(move |execution_id| {
        Box::new(RecallNotesInstance {
            notes: recall_store.clone(),
            execution_id: execution_id.to_string(),
        })
    }));

    let categories_store = notes;
    registry.register_stateful_factory("list_categories", Arc::new(move |execution_id| {
        Box::new(ListCategoriesInstance {
            notes: categories_store.clone(),
            execution_id: execution_id.to_string(),
        })
    }));

    let remember_store = memory.clone();
    registry.register_stateful_factory("memory_remember", Arc::new(move |_execution_id| {
        Box::new(MemoryRememberInstance {
            memory: remember_store.clone(),
        })
    }));

    let forget_store = memory.clone();
    registry.register_stateful_factory("memory_forget", Arc::new(move |_execution_id| {
        Box::new(MemoryForgetInstance {
            memory: forget_store.clone(),
        })
    }));

    let list_store = memory;
    registry.register_stateful_factory("memory_list", Arc::new(move |_execution_id| {
        Box::new(MemoryListInstance {
            memory: list_store.clone(),
        })
    }));

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::executor::trait_def::ToolExecutionContext;

    #[tokio::test]
    async fn test_session_notes_flow() {
        let registry = ToolRegistry::new();
        register(&registry).unwrap();
        registry.register_tool(RECORD_NOTE.tool_def());
        registry.register_tool(RECALL_NOTES.tool_def());
        registry.register_tool(LIST_CATEGORIES.tool_def());

        let ctx = ToolExecutionContext::new("exec-1".into());
        let options = wf_types::tool::ToolExecutionOptions {
            timeout: None,
            retries: None,
            retry_delay: None,
            exponential_backoff: None,
        };

        let _ = registry
            .execute_tool(
                "record_note",
                &serde_json::json!({ "note": "alpha note", "category": "work" }),
                &options,
                &ctx,
            )
            .await
            .unwrap();
        let _ = registry
            .execute_tool(
                "record_note",
                &serde_json::json!({ "note": "beta note", "category": "personal" }),
                &options,
                &ctx,
            )
            .await
            .unwrap();

        let recalled = registry
            .execute_tool("recall_notes", &serde_json::json!({}), &options, &ctx)
            .await
            .unwrap();
        let notes = recalled.result.unwrap();
        assert_eq!(notes["total"], 2);

        let filtered = registry
            .execute_tool(
                "recall_notes",
                &serde_json::json!({ "search": "beta" }),
                &options,
                &ctx,
            )
            .await
            .unwrap();
        assert_eq!(filtered.result.unwrap()["total"], 1);

        let categories = registry
            .execute_tool("list_categories", &serde_json::json!({}), &options, &ctx)
            .await
            .unwrap();
        let cats = categories.result.unwrap();
        assert_eq!(cats["total"], 2);
        assert!(cats["categories"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["category"] == "work" && c["count"] == 1));
    }

    #[tokio::test]
    async fn test_session_notes_isolated_per_execution() {
        let registry = ToolRegistry::new();
        register(&registry).unwrap();
        registry.register_tool(RECORD_NOTE.tool_def());
        registry.register_tool(RECALL_NOTES.tool_def());
        let options = wf_types::tool::ToolExecutionOptions {
            timeout: None,
            retries: None,
            retry_delay: None,
            exponential_backoff: None,
        };

        let ctx_a = ToolExecutionContext::new("exec-a".into());
        let ctx_b = ToolExecutionContext::new("exec-b".into());
        let _ = registry
            .execute_tool(
                "record_note",
                &serde_json::json!({ "note": "only in a" }),
                &options,
                &ctx_a,
            )
            .await
            .unwrap();

        let recalled_b = registry
            .execute_tool("recall_notes", &serde_json::json!({}), &options, &ctx_b)
            .await
            .unwrap();
        assert_eq!(recalled_b.result.unwrap()["total"], 0);
    }

    #[tokio::test]
    async fn test_long_term_memory_flow() {
        let registry = ToolRegistry::new();
        register(&registry).unwrap();
        registry.register_tool(MEMORY_REMEMBER.tool_def());
        registry.register_tool(MEMORY_FORGET.tool_def());
        registry.register_tool(MEMORY_LIST.tool_def());

        let ctx = ToolExecutionContext::new("exec-1".into());
        let options = wf_types::tool::ToolExecutionOptions {
            timeout: None,
            retries: None,
            retry_delay: None,
            exponential_backoff: None,
        };

        let _ = registry
            .execute_tool(
                "memory_remember",
                &serde_json::json!({ "key": "user.name", "content": "Alice" }),
                &options,
                &ctx,
            )
            .await
            .unwrap();

        let listed = registry
            .execute_tool(
                "memory_list",
                &serde_json::json!({ "prefix": "user." }),
                &options,
                &ctx,
            )
            .await
            .unwrap();
        let memories = listed.result.unwrap();
        assert_eq!(memories["total"], 1);
        assert_eq!(memories["memories"][0]["content"], "Alice");

        let removed = registry
            .execute_tool(
                "memory_forget",
                &serde_json::json!({ "key": "user.name" }),
                &options,
                &ctx,
            )
            .await
            .unwrap();
        assert_eq!(removed.result.unwrap()["removed"], true);

        let listed = registry
            .execute_tool("memory_list", &serde_json::json!({}), &options, &ctx)
            .await
            .unwrap();
        assert_eq!(listed.result.unwrap()["total"], 0);
    }
}
