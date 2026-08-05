//! Definition and stateful instance of the release_sessions_for_task tool.

use serde_json::Value;
use std::sync::Arc;

use wf_types::tool::{ToolRiskLevel, ToolType};

use crate::error::{ToolError, ToolResult};
use crate::executor::StatefulInstance;
use crate::predefined::schema::{ToolDefinition, ToolParameter};
use crate::registry::ToolRegistry;
use wf_shell::engine::BackgroundShellStore;

pub static RELEASE_SESSIONS_FOR_TASK: ToolDefinition = ToolDefinition {
    id: "release_sessions_for_task",
    tool_type: ToolType::Stateful,
    risk_level: ToolRiskLevel::Execute,
    create_checkpoint: None,
    category: "shell",
    tags: &["shell", "session"],
    description: "Release (or terminate) all background shell sessions bound to a task_id. With 'terminate' set to false the task binding is cleared and idle sessions become reusable by other calls (default); with 'terminate' set to true running commands are killed and the sessions are removed.",
    parameters: &[
        ToolParameter { name: "task_id", r#type: "string", required: true, description: "The task id the sessions are bound to", default_json: None },
        ToolParameter { name: "terminate", r#type: "boolean", required: false, description: "Terminate (kill) the sessions instead of just releasing them (default false)", default_json: Some("false") },
    ],
    tips: None,
    examples: Some(&[
        "release_sessions_for_task(\"exec-1\")",
        "release_sessions_for_task(\"exec-1\", terminate=true)",
    ]),
};

/// Stateful instance for the release_sessions_for_task tool.
struct ReleaseSessionsForTaskInstance {
    store: Arc<BackgroundShellStore>,
}

impl StatefulInstance for ReleaseSessionsForTaskInstance {
    fn execute(&self, params: &Value) -> ToolResult<Value> {
        let task_id = params
            .get("task_id")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                ToolError::ValidationFailed("Missing or invalid 'task_id' parameter".into())
            })?;
        let terminate = params
            .get("terminate")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let released = self.store.release_sessions_for_task(task_id, terminate);
        Ok(serde_json::json!({
            "task_id": task_id,
            "released": released,
            "terminated": terminate,
        }))
    }

    fn destroy(&self) -> ToolResult<()> {
        Ok(())
    }
}

/// Register the release_sessions_for_task stateful factory into the registry.
pub fn register(registry: &ToolRegistry, store: &Arc<BackgroundShellStore>) -> ToolResult<()> {
    let store = store.clone();
    registry.register_stateful_factory(
        "release_sessions_for_task",
        Arc::new(move |_execution_id| {
            Box::new(ReleaseSessionsForTaskInstance {
                store: store.clone(),
            })
        }),
    );
    Ok(())
}
