use async_trait::async_trait;
use serde_json::Value;
use std::time::Instant;

use crate::error::ToolResult;
use crate::executor::base::BaseExecutor;
use crate::executor::trait_def::{ToolExecutionContext, ToolExecutor};
use wf_types::tool::ToolExecutionOptions;
use wf_types::tool::ToolExecutionResult;

pub struct StatefulExecutor {
    instances: dashmap::DashMap<String, dashmap::DashMap<String, Value>>,
}

impl StatefulExecutor {
    pub fn new() -> Self {
        Self {
            instances: dashmap::DashMap::new(),
        }
    }

    pub fn get_state(&self, execution_id: &str, tool_name: &str) -> Option<Value> {
        self.instances
            .get(execution_id)?
            .get(tool_name)
            .map(|v| v.clone())
    }

    pub fn set_state(&self, execution_id: &str, tool_name: String, value: Value) {
        self.instances
            .entry(execution_id.to_string())
            .or_default()
            .insert(tool_name, value);
    }

    pub fn get_execution_states(&self, execution_id: &str) -> Option<dashmap::DashMap<String, Value>> {
        self.instances.get(execution_id).map(|entry| entry.clone())
    }

    pub fn cleanup_execution(&self, execution_id: &str) {
        self.instances.remove(execution_id);
    }

    pub fn clear_all(&self) {
        self.instances.clear();
    }

    pub fn execution_count(&self) -> usize {
        self.instances.len()
    }
}

impl Default for StatefulExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ToolExecutor for StatefulExecutor {
    async fn execute(
        &self,
        tool: &wf_types::tool::Tool,
        parameters: &Value,
        _options: &ToolExecutionOptions,
        context: &ToolExecutionContext,
    ) -> ToolResult<ToolExecutionResult> {
        let start = Instant::now();
        BaseExecutor::validate_parameters(tool, parameters)?;

        self.set_state(
            &context.execution_id,
            format!("{}_last_execution", tool.name),
            serde_json::json!({
                "timestamp": wf_common::time::timestamp_to_iso(wf_common::time::now()),
                "parameters": parameters,
            }),
        );

        let state_count = self
            .instances
            .get(&context.execution_id)
            .map(|exec| exec.len())
            .unwrap_or(0);

        let result = serde_json::json!({
            "status": "executed",
            "state_count": state_count,
            "execution_id": context.execution_id,
        });

        let execution_time = start.elapsed().as_millis() as i64;
        Ok(BaseExecutor::build_result(true, Some(result), None, execution_time, 0))
    }

    fn executor_type(&self) -> &str {
        "stateful"
    }

    async fn cleanup(&self) -> ToolResult<()> {
        self.clear_all();
        Ok(())
    }
}
