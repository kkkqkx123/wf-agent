use async_trait::async_trait;
use dashmap::DashMap;
use std::sync::Arc;

use crate::error::{ToolError, ToolResult};
use crate::callback::ExecutionCallback;
use crate::executor::{
    BuiltinExecutor, InstanceFactory, McpExecutor, RestExecutor, StatelessExecutor,
    StatefulExecutor, StatelessHandler, ToolExecutor, ToolExecutorExt,
};
use wf_types::tool::ToolType;
use wf_types::Id;

type ExecutorFactory = Arc<dyn Fn(&wf_types::tool::Tool) -> ToolResult<Arc<dyn ToolExecutor>> + Send + Sync>;

#[async_trait]
pub trait ToolStorage: Send + Sync {
    async fn load_tools(&self) -> ToolResult<Vec<wf_types::tool::Tool>>;
    async fn save_tool(&self, tool: &wf_types::tool::Tool) -> ToolResult<()>;
    async fn delete_tool(&self, tool_id: &str) -> ToolResult<()>;
}

pub struct ToolRegistry {
    executors: DashMap<ToolType, ExecutorFactory>,
    tools: DashMap<Id, wf_types::tool::Tool>,
    stateless_handlers: Arc<DashMap<String, StatelessHandler>>,
    stateful_factories: Arc<DashMap<String, InstanceFactory>>,
    builtin_callback: Arc<std::sync::Mutex<Option<Arc<dyn ExecutionCallback>>>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        let stateless_handlers = Arc::new(DashMap::new());
        let stateful_factories = Arc::new(DashMap::new());
        let sl_handlers = stateless_handlers.clone();
        let sf_factories = stateful_factories.clone();
        let registry = Self {
            executors: DashMap::new(),
            tools: DashMap::new(),
            stateless_handlers,
            stateful_factories,
            builtin_callback: Arc::new(std::sync::Mutex::new(None)),
        };
        registry.register_defaults_shared(sl_handlers, sf_factories);
        registry
    }

    pub fn stateless_handlers(&self) -> &Arc<DashMap<String, StatelessHandler>> {
        &self.stateless_handlers
    }

    pub fn register_stateless_handler(&self, tool_id: &str, handler: StatelessHandler) {
        self.stateless_handlers.insert(tool_id.to_string(), handler);
    }

    pub fn unregister_stateless_handler(&self, tool_id: &str) {
        self.stateless_handlers.remove(tool_id);
    }

    pub fn stateful_factories(&self) -> &Arc<DashMap<String, InstanceFactory>> {
        &self.stateful_factories
    }

    pub fn register_stateful_factory(&self, tool_id: &str, factory: InstanceFactory) {
        self.stateful_factories.insert(tool_id.to_string(), factory);
    }

    pub fn unregister_stateful_factory(&self, tool_id: &str) {
        self.stateful_factories.remove(tool_id);
    }

    pub fn set_builtin_callback(&self, callback: Arc<dyn ExecutionCallback>) {
        *self.builtin_callback.lock().unwrap() = Some(callback);
    }

    fn register_defaults_shared(
        &self,
        sl_handlers: Arc<DashMap<String, StatelessHandler>>,
        sf_factories: Arc<DashMap<String, InstanceFactory>>,
    ) {
        let h = sl_handlers.clone();
        self.register_executor(
            ToolType::Stateless,
            Arc::new(move |tool| Ok(Arc::new(StatelessExecutor::from_tool_config_shared(tool, h.clone())))),
        );
        let f = sf_factories.clone();
        self.register_executor(
            ToolType::Stateful,
            Arc::new(move |_tool| Ok(Arc::new(StatefulExecutor::new_shared(f.clone())))),
        );
        self.register_executor(
            ToolType::Rest,
            Arc::new(|_tool| Ok(Arc::new(RestExecutor::new()))),
        );
        let builtin_cb = self.builtin_callback.clone();
        self.register_executor(
            ToolType::BuiltIn,
            Arc::new(move |_tool| {
                let cb = builtin_cb.lock().unwrap().clone();
                Ok(Arc::new(BuiltinExecutor::with_callback_opt(cb)))
            }),
        );
        self.register_executor(
            ToolType::Mcp,
            Arc::new(|tool| {
                let executor = McpExecutor::from_tool_config(tool)?;
                Ok(Arc::new(executor))
            }),
        );
    }

    pub fn register_executor(&self, tool_type: ToolType, factory: ExecutorFactory) {
        self.executors.insert(tool_type, factory);
    }

    pub fn register_tool(&self, tool: wf_types::tool::Tool) {
        self.tools.insert(tool.id.clone(), tool);
    }

    pub fn get_tool(&self, tool_id: &str) -> Option<wf_types::tool::Tool> {
        self.tools.get(tool_id).map(|t| t.clone())
    }

    pub fn remove_tool(&self, tool_id: &str) -> Option<wf_types::tool::Tool> {
        self.tools.remove(tool_id).map(|(_, t)| t)
    }

    pub fn list_tools(&self) -> Vec<wf_types::tool::Tool> {
        self.tools.iter().map(|entry| entry.value().clone()).collect()
    }

    pub fn list_tools_by_type(&self, tool_type: &ToolType) -> Vec<wf_types::tool::Tool> {
        self.tools
            .iter()
            .filter(|entry| &entry.value().tool_type == tool_type)
            .map(|entry| entry.value().clone())
            .collect()
    }

    pub fn get_executor(&self, tool: &wf_types::tool::Tool) -> ToolResult<Arc<dyn ToolExecutor>> {
        let factory = self
            .executors
            .get(&tool.tool_type)
            .ok_or_else(|| ToolError::NotFound(format!(
                "No executor registered for tool type: {:?}",
                tool.tool_type
            )))?;
        (factory)(tool)
    }

    pub async fn execute_tool(
        &self,
        tool_id: &str,
        parameters: &serde_json::Value,
        options: &wf_types::tool::ToolExecutionOptions,
        context: &crate::executor::trait_def::ToolExecutionContext,
    ) -> ToolResult<wf_types::tool::ToolExecutionResult> {
        let tool = self
            .get_tool(tool_id)
            .ok_or_else(|| ToolError::NotFound(tool_id.to_string()))?;

        if tool.enabled == Some(false) {
            return Err(ToolError::ExecutionFailed {
                tool_id: tool_id.to_string(),
                reason: "Tool is disabled".into(),
            });
        }

        let executor = self.get_executor(&tool)?;
        executor
            .execute_with_timeout(&tool, parameters, options, context)
            .await
    }

    pub fn tool_count(&self) -> usize {
        self.tools.len()
    }

    pub fn clear(&self) {
        self.tools.clear();
    }

    pub fn search(&self, query: &str) -> Vec<wf_types::tool::Tool> {
        let query_lower = query.to_lowercase();
        self.tools
            .iter()
            .filter(|entry| {
                let tool = entry.value();
                tool.name.to_lowercase().contains(&query_lower)
                    || tool.description.to_lowercase().contains(&query_lower)
            })
            .map(|entry| entry.value().clone())
            .collect()
    }

    pub async fn initialize_from_storage(&self, storage: &dyn ToolStorage) -> ToolResult<()> {
        let tools = storage.load_tools().await?;
        for tool in tools {
            self.tools.insert(tool.id.clone(), tool);
        }
        Ok(())
    }

    pub async fn persist_tool(
        &self,
        tool: &wf_types::tool::Tool,
        storage: &dyn ToolStorage,
    ) -> ToolResult<()> {
        storage.save_tool(tool).await?;
        self.tools.insert(tool.id.clone(), tool.clone());
        Ok(())
    }

    pub async fn remove_tool_persistent(
        &self,
        tool_id: &str,
        storage: &dyn ToolStorage,
    ) -> ToolResult<Option<wf_types::tool::Tool>> {
        storage.delete_tool(tool_id).await?;
        Ok(self.remove_tool(tool_id))
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_tool(id: &str, name: &str, tool_type: ToolType) -> wf_types::tool::Tool {
        wf_types::tool::Tool {
            id: id.into(),
            name: name.into(),
            description: format!("Test tool: {}", name),
            tool_type,
            parameters: None,
            metadata: None,
            config: None,
            enabled: Some(true),
            strict: None,
            default_timeout_ms: None,
        }
    }

    #[test]
    fn test_register_and_get_tool() {
        let registry = ToolRegistry::new();
        let tool = make_tool("t1", "test_tool", ToolType::Stateless);
        registry.register_tool(tool.clone());

        let retrieved = registry.get_tool("t1").unwrap();
        assert_eq!(retrieved.id, "t1");
        assert_eq!(retrieved.name, "test_tool");
    }

    #[test]
    fn test_list_tools_by_type() {
        let registry = ToolRegistry::new();
        registry.register_tool(make_tool("t1", "stateless1", ToolType::Stateless));
        registry.register_tool(make_tool("t2", "stateless2", ToolType::Stateless));
        registry.register_tool(make_tool("t3", "rest1", ToolType::Rest));

        let stateless = registry.list_tools_by_type(&ToolType::Stateless);
        assert_eq!(stateless.len(), 2);

        let rest = registry.list_tools_by_type(&ToolType::Rest);
        assert_eq!(rest.len(), 1);
    }

    #[tokio::test]
    async fn test_disabled_tool_returns_error() {
        let registry = ToolRegistry::new();
        let mut tool = make_tool("t1", "disabled_tool", ToolType::Stateless);
        tool.enabled = Some(false);
        registry.register_tool(tool);

        let ctx = crate::executor::trait_def::ToolExecutionContext::new("exec1".into());
        let options = wf_types::tool::ToolExecutionOptions {
            timeout: Some(1000),
            retries: None,
            retry_delay: None,
            exponential_backoff: None,
        };

        let result = registry.execute_tool("t1", &serde_json::json!({}), &options, &ctx).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_get_executor_for_type() {
        let registry = ToolRegistry::new();
        let tool = make_tool("t1", "stateless", ToolType::Stateless);
        let executor = registry.get_executor(&tool).unwrap();
        assert_eq!(executor.executor_type(), "stateless");
    }
}
