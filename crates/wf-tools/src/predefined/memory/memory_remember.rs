//! Definition and stateful instance of the memory_remember tool.

use serde_json::Value;
use std::sync::Arc;

use wf_types::tool::{ToolRiskLevel, ToolType};

use crate::error::{ToolError, ToolResult};
use crate::executor::StatefulInstance;
use crate::predefined::memory::store::MemoryStore;
use crate::predefined::schema::{ToolDefinition, ToolParameter};
use crate::registry::ToolRegistry;

pub static MEMORY_REMEMBER: ToolDefinition = ToolDefinition {
    id: "memory_remember",
    tool_type: ToolType::Stateful,
    risk_level: ToolRiskLevel::Write,
    create_checkpoint: None,
    category: "memory",
    tags: &["remember"],
    description:
        "Store a piece of information in long-term memory for later recall across sessions.",
    parameters: &[
        ToolParameter {
            name: "key",
            r#type: "string",
            required: true,
            description: "The memory key",
            default_json: None,
        },
        ToolParameter {
            name: "content",
            r#type: "string",
            required: true,
            description: "The content to remember",
            default_json: None,
        },
    ],
    tips: None,
    examples: Some(&["memory_remember(\"user_name\", \"Alice\")"]),
};

struct MemoryRememberInstance {
    store: Arc<MemoryStore>,
}

impl StatefulInstance for MemoryRememberInstance {
    fn execute(&self, params: &Value) -> ToolResult<Value> {
        let key = params
            .get("key")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                ToolError::ValidationFailed("Missing or invalid 'key' parameter".into())
            })?
            .to_string();
        let content = params
            .get("content")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                ToolError::ValidationFailed("Missing or invalid 'content' parameter".into())
            })?;

        self.store
            .memory
            .insert(key.clone(), Value::String(content.into()));
        Ok(serde_json::json!({ "remembered": true, "key": key }))
    }
}

/// Register the memory_remember stateful factory into the registry.
pub(crate) fn register(registry: &ToolRegistry, store: &Arc<MemoryStore>) -> ToolResult<()> {
    let store = store.clone();
    registry.register_stateful_factory(
        "memory_remember",
        Arc::new(move |_execution_id| {
            Box::new(MemoryRememberInstance {
                store: store.clone(),
            })
        }),
    );
    Ok(())
}
