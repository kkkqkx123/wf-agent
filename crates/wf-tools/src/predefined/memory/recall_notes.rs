//! Definition and stateful instance of the recall_notes tool.

use serde_json::Value;
use std::sync::Arc;

use wf_types::tool::ToolType;

use crate::error::ToolResult;
use crate::executor::StatefulInstance;
use crate::predefined::memory::store::{MemoryStore, SessionNote};
use crate::predefined::schema::{ToolDefinition, ToolParameter};
use crate::registry::ToolRegistry;

pub static RECALL_NOTES: ToolDefinition = ToolDefinition {
    id: "recall_notes",
    tool_type: ToolType::Stateful,
    category: "memory",
    tags: &["note", "recall"],
    description:
        "Recall previously recorded session notes. Filters by optional search term and category.",
    parameters: &[
        ToolParameter {
            name: "search",
            r#type: "string",
            required: false,
            description: "Optional search term to filter notes",
            default_json: None,
        },
        ToolParameter {
            name: "category",
            r#type: "string",
            required: false,
            description: "Optional category to filter by",
            default_json: None,
        },
        ToolParameter {
            name: "limit",
            r#type: "number",
            required: false,
            description: "Maximum number of notes to return",
            default_json: None,
        },
    ],
    tips: None,
    examples: Some(&["recall_notes(\"preferences\")"]),
};

struct RecallNotesInstance {
    store: Arc<MemoryStore>,
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
        let limit = params.get("limit").and_then(|v| v.as_u64()).unwrap_or(50) as usize;

        let all = self
            .store
            .notes
            .get(&self.execution_id)
            .map(|e| e.clone())
            .unwrap_or_default();
        let mut matched: Vec<&SessionNote> = all
            .iter()
            .filter(|n| {
                category
                    .map(|c| n.category.as_deref() == Some(c))
                    .unwrap_or(true)
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

/// Register the recall_notes stateful factory into the registry.
pub(crate) fn register(registry: &ToolRegistry, store: &Arc<MemoryStore>) -> ToolResult<()> {
    let store = store.clone();
    registry.register_stateful_factory(
        "recall_notes",
        Arc::new(move |execution_id| {
            Box::new(RecallNotesInstance {
                store: store.clone(),
                execution_id: execution_id.to_string(),
            })
        }),
    );
    Ok(())
}
