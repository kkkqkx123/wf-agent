use async_trait::async_trait;
use dashmap::DashMap;
use serde_json::Value;
use std::sync::Arc;
use std::time::Instant;

use crate::error::{ToolError, ToolResult};
use crate::executor::base::BaseExecutor;
use crate::executor::trait_def::{ToolExecutionContext, ToolExecutor};
use wf_types::tool::ToolExecutionOptions;
use wf_types::tool::ToolExecutionResult;

pub trait StatefulInstance: Send + Sync {
    fn execute(&self, params: &Value) -> ToolResult<Value>;
    fn destroy(&self) -> ToolResult<()> {
        Ok(())
    }
}

pub type InstanceFactory = Arc<dyn Fn(&str) -> Box<dyn StatefulInstance> + Send + Sync>;

pub struct StatefulExecutor {
    storage: DashMap<String, DashMap<String, Value>>,
    factories: Arc<DashMap<String, InstanceFactory>>,
    instances: DashMap<String, DashMap<String, Box<dyn StatefulInstance>>>,
}

impl StatefulExecutor {
    pub fn new() -> Self {
        Self {
            storage: DashMap::new(),
            factories: Arc::new(DashMap::new()),
            instances: DashMap::new(),
        }
    }

    pub fn new_shared(factories: Arc<DashMap<String, InstanceFactory>>) -> Self {
        Self {
            storage: DashMap::new(),
            factories,
            instances: DashMap::new(),
        }
    }

    pub fn factories(&self) -> &Arc<DashMap<String, InstanceFactory>> {
        &self.factories
    }

    pub fn register_factory(&self, tool_id: &str, factory: InstanceFactory) {
        self.factories.insert(tool_id.to_string(), factory);
    }

    pub fn unregister_factory(&self, tool_id: &str) {
        self.factories.remove(tool_id);
    }

    pub fn has_factory(&self, tool_id: &str) -> bool {
        self.factories.contains_key(tool_id)
    }

    pub fn create_instance(&self, tool_id: &str, execution_id: &str) -> ToolResult<()> {
        let factory = self.factories.get(tool_id).ok_or_else(|| {
            ToolError::NotFound(format!("No factory registered for tool '{}'", tool_id))
        })?;
        let instance = factory(execution_id);
        self.instances
            .entry(execution_id.to_string())
            .or_default()
            .insert(tool_id.to_string(), instance);
        Ok(())
    }

    pub fn destroy_instance(&self, tool_id: &str, execution_id: &str) -> ToolResult<()> {
        if let Some(exec) = self.instances.get(execution_id) {
            if let Some((_, instance)) = exec.remove(tool_id) {
                instance.destroy()?;
            }
        }
        Ok(())
    }

    pub fn get_state(&self, execution_id: &str, tool_name: &str) -> Option<Value> {
        self.storage
            .get(execution_id)?
            .get(tool_name)
            .map(|v| v.clone())
    }

    pub fn set_state(&self, execution_id: &str, tool_name: String, value: Value) {
        self.storage
            .entry(execution_id.to_string())
            .or_default()
            .insert(tool_name, value);
    }

    pub fn get_execution_states(&self, execution_id: &str) -> Option<DashMap<String, Value>> {
        self.storage.get(execution_id).map(|entry| entry.clone())
    }

    pub fn cleanup_execution(&self, execution_id: &str) {
        if let Some((_, exec)) = self.instances.remove(execution_id) {
            for entry in exec {
                let _ = entry.1.destroy();
            }
        }
        self.storage.remove(execution_id);
    }

    pub fn clear_all(&self) {
        for exec in self.instances.iter() {
            for entry in exec.iter() {
                let _ = entry.value().destroy();
            }
        }
        self.instances.clear();
        self.storage.clear();
    }

    pub fn execution_count(&self) -> usize {
        self.storage.len()
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

        let tool_id = tool.id.clone();
        let exec_id = &context.execution_id;

        if let Some(factory) = self.factories.get(&tool_id) {
            let exec = self.instances.entry(exec_id.to_string()).or_default();
            if !exec.contains_key(&tool_id) {
                exec.insert(tool_id.clone(), factory(exec_id));
            }
            drop(factory);
            if let Some(instance) = exec.get(&tool_id) {
                let result = instance.execute(parameters)?;
                let execution_time = start.elapsed().as_millis() as i64;
                return Ok(BaseExecutor::build_result(
                    true,
                    Some(result),
                    None,
                    execution_time,
                    0,
                ));
            }
            return Err(ToolError::Internal(
                "Failed to create stateful instance".into(),
            ));
        }

        self.set_state(
            exec_id,
            format!("{}_last_execution", tool.name),
            serde_json::json!({
                "timestamp": wf_common::time::timestamp_to_iso(wf_common::time::now()),
                "parameters": parameters,
            }),
        );

        let state_count = self
            .storage
            .get(exec_id)
            .map(|exec| exec.len())
            .unwrap_or(0);

        let result = serde_json::json!({
            "status": "executed",
            "state_count": state_count,
            "execution_id": exec_id,
        });

        let execution_time = start.elapsed().as_millis() as i64;
        Ok(BaseExecutor::build_result(
            true,
            Some(result),
            None,
            execution_time,
            0,
        ))
    }

    fn executor_type(&self) -> &str {
        "stateful"
    }

    async fn cleanup(&self) -> ToolResult<()> {
        self.clear_all();
        Ok(())
    }
}
