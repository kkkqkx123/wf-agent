//! Definition and stateful instance of the memory_forget tool.

use serde_json::Value;
use std::sync::Arc;

use wf_types::tool::ToolType;

use crate::error::{ToolError, ToolResult};
use crate::executor::StatefulInstance;
use crate::predefined::memory::store::MemoryStore;
use crate::predefined::schema::{ToolDefinition, ToolParameter};
use crate::registry::ToolRegistry;

pub static MEMORY_FORGET: ToolDefinition = ToolDefinition {
    id: "memory_forget",
    tool_type: ToolType::Stateful,
    category: "memory",
    tags: &["forget"],
    description: "Remove a specific piece of information from long-term memory.",
    parameters: &[ToolParameter {
        name: "key",
        r#type: "string",
        required: true,
        description: "The memory key to forget",
        default_json: None,
    }],
    tips: None,
    examples: Some(&["memory_forget(\"user_name\")"]),
};

struct MemoryForgetInstance {
    store: Arc<MemoryStore>,
}

impl StatefulInstance for MemoryForgetInstance {
    fn execute(&self, params: &Value) -> ToolResult<Value> {
        let key = params
            .get("key")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                ToolError::ValidationFailed("Missing or invalid 'key' parameter".into())
            })?;
        let removed = self.store.memory.remove(key).is_some();
        Ok(serde_json::json!({ "removed": removed, "key": key }))
    }
}

/// Register the memory_forget stateful factory into the registry.
pub(crate) fn register(registry: &ToolRegistry, store: &Arc<MemoryStore>) -> ToolResult<()> {
    let store = store.clone();
    registry.register_stateful_factory(
        "memory_forget",
        Arc::new(move |_execution_id| {
            Box::new(MemoryForgetInstance {
                store: store.clone(),
            })
        }),
    );
    Ok(())
}
