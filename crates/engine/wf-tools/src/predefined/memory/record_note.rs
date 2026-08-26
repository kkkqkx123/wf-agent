//! Definition and stateful instance of the record_note tool.

use serde_json::Value;
use std::sync::Arc;

use wf_types::tool::{ToolRiskLevel, ToolType};

use crate::error::{ToolError, ToolResult};
use crate::executor::StatefulInstance;
use crate::predefined::memory::store::{MemoryStore, SessionNote};
use crate::predefined::schema::{ToolDefinition, ToolParameter};
use crate::registry::ToolRegistry;

pub static RECORD_NOTE: ToolDefinition = ToolDefinition {
    id: "record_note",
    tool_type: ToolType::Stateful,
    risk_level: ToolRiskLevel::Write,
    create_checkpoint: None,
    category: "memory",
    tags: &["note", "session"],
    description: "Record a note in memory with an optional category. Notes can be recalled later within the same execution.",
    parameters: &[
        ToolParameter { name: "note", r#type: "string", required: true, description: "The note content to record", default_json: None, constraints: None },
        ToolParameter { name: "category", r#type: "string", required: false, description: "Optional category label for the note", default_json: None, constraints: None },
    ],
    tips: None,
    examples: Some(&["record_note(\"User prefers dark mode\", \"preferences\")"]),
};

struct RecordNoteInstance {
    store: Arc<MemoryStore>,
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

        self.store
            .notes
            .entry(self.execution_id.clone())
            .or_default()
            .push(SessionNote {
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

/// Register the record_note stateful factory into the registry.
pub(crate) fn register(registry: &ToolRegistry, store: &Arc<MemoryStore>) -> ToolResult<()> {
    let store = store.clone();
    registry.register_stateful_factory(
        "record_note",
        Arc::new(move |execution_id| {
            Box::new(RecordNoteInstance {
                store: store.clone(),
                execution_id: execution_id.to_string(),
            })
        }),
    );
    Ok(())
}
