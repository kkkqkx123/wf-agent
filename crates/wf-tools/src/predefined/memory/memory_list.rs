//! Definition and stateful instance of the memory_list tool.

use serde_json::Value;
use std::sync::Arc;

use wf_types::tool::{ToolRiskLevel, ToolType};

use crate::error::ToolResult;
use crate::executor::StatefulInstance;
use crate::predefined::memory::store::MemoryStore;
use crate::predefined::schema::{ToolDefinition, ToolParameter};
use crate::registry::ToolRegistry;

pub static MEMORY_LIST: ToolDefinition = ToolDefinition {
    id: "memory_list",
    tool_type: ToolType::Stateful,
    risk_level: ToolRiskLevel::Write,
    create_checkpoint: None,
    category: "memory",
    tags: &["list"],
    description: "List all stored memories. Optionally filter by prefix.",
    parameters: &[ToolParameter {
        name: "prefix",
        r#type: "string",
        required: false,
        description: "Optional prefix to filter by",
        default_json: None,
    }],
    tips: None,
    examples: Some(&["memory_list()"]),
};

struct MemoryListInstance {
    store: Arc<MemoryStore>,
}

impl StatefulInstance for MemoryListInstance {
    fn execute(&self, params: &Value) -> ToolResult<Value> {
        let prefix = params
            .get("prefix")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .unwrap_or("");

        let mut entries: Vec<Value> = self
            .store
            .memory
            .iter()
            .filter(|e| e.key().starts_with(prefix))
            .map(|e| serde_json::json!({ "key": e.key(), "content": e.value() }))
            .collect();
        entries.sort_by_key(|e| e["key"].as_str().unwrap_or("").to_string());

        Ok(serde_json::json!({ "memories": entries, "total": entries.len() }))
    }
}

/// Register the memory_list stateful factory into the registry.
pub(crate) fn register(registry: &ToolRegistry, store: &Arc<MemoryStore>) -> ToolResult<()> {
    let store = store.clone();
    registry.register_stateful_factory(
        "memory_list",
        Arc::new(move |_execution_id| {
            Box::new(MemoryListInstance {
                store: store.clone(),
            })
        }),
    );
    Ok(())
}
