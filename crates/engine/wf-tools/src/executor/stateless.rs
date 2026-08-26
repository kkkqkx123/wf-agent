use async_trait::async_trait;
use dashmap::DashMap;
use serde_json::Value;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Instant;

use crate::error::{ToolError, ToolResult};
use crate::executor::base::BaseExecutor;
use crate::executor::trait_def::{ToolExecutionContext, ToolExecutor};
use wf_types::tool::ToolExecutionOptions;
use wf_types::tool::ToolExecutionResult;

pub type StatelessHandler =
    Arc<dyn Fn(&Value, &ToolExecutionContext) -> ToolResult<Value> + Send + Sync>;

pub type StatelessAsyncHandler = Arc<
    dyn Fn(Value, ToolExecutionContext) -> Pin<Box<dyn Future<Output = ToolResult<Value>> + Send>>
        + Send
        + Sync,
>;

#[derive(Debug, Clone)]
pub struct StatelessToolRuntime {
    pub endpoint: Option<String>,
    pub method: Option<String>,
}

pub struct StatelessExecutor {
    handlers: Arc<DashMap<String, StatelessHandler>>,
    async_handlers: Arc<DashMap<String, StatelessAsyncHandler>>,
    #[allow(dead_code)]
    runtime: Option<StatelessToolRuntime>,
}

impl StatelessExecutor {
    pub fn new() -> Self {
        Self {
            handlers: Arc::new(DashMap::new()),
            async_handlers: Arc::new(DashMap::new()),
            runtime: None,
        }
    }

    pub fn new_shared(
        handlers: Arc<DashMap<String, StatelessHandler>>,
        async_handlers: Arc<DashMap<String, StatelessAsyncHandler>>,
    ) -> Self {
        Self {
            handlers,
            async_handlers,
            runtime: None,
        }
    }

    pub fn with_runtime(runtime: StatelessToolRuntime) -> Self {
        Self {
            handlers: Arc::new(DashMap::new()),
            async_handlers: Arc::new(DashMap::new()),
            runtime: Some(runtime),
        }
    }

    pub fn from_tool_config(tool: &wf_types::tool::Tool) -> Self {
        let runtime = tool.config.as_ref().map(|config| StatelessToolRuntime {
            endpoint: config
                .get("endpoint")
                .and_then(|v| v.as_str())
                .map(String::from),
            method: config
                .get("method")
                .and_then(|v| v.as_str())
                .map(String::from),
        });

        Self {
            handlers: Arc::new(DashMap::new()),
            async_handlers: Arc::new(DashMap::new()),
            runtime,
        }
    }

    pub fn from_tool_config_shared(
        tool: &wf_types::tool::Tool,
        handlers: Arc<DashMap<String, StatelessHandler>>,
    ) -> Self {
        let runtime = tool.config.as_ref().map(|config| StatelessToolRuntime {
            endpoint: config
                .get("endpoint")
                .and_then(|v| v.as_str())
                .map(String::from),
            method: config
                .get("method")
                .and_then(|v| v.as_str())
                .map(String::from),
        });

        Self {
            handlers,
            async_handlers: Arc::new(DashMap::new()),
            runtime,
        }
    }

    pub fn with_async_handlers(
        mut self,
        async_handlers: Arc<DashMap<String, StatelessAsyncHandler>>,
    ) -> Self {
        self.async_handlers = async_handlers;
        self
    }

    pub fn handlers(&self) -> &Arc<DashMap<String, StatelessHandler>> {
        &self.handlers
    }

    pub fn async_handlers(&self) -> &Arc<DashMap<String, StatelessAsyncHandler>> {
        &self.async_handlers
    }

    pub fn register_handler(&self, tool_id: &str, handler: StatelessHandler) {
        self.handlers.insert(tool_id.to_string(), handler);
    }

    pub fn register_async_handler(&self, tool_id: &str, handler: StatelessAsyncHandler) {
        self.async_handlers.insert(tool_id.to_string(), handler);
    }

    pub fn unregister_handler(&self, tool_id: &str) {
        self.handlers.remove(tool_id);
        self.async_handlers.remove(tool_id);
    }

    pub fn has_handler(&self, tool_id: &str) -> bool {
        self.handlers.contains_key(tool_id) || self.async_handlers.contains_key(tool_id)
    }

    pub fn clear_handlers(&self) {
        self.handlers.clear();
        self.async_handlers.clear();
    }
}

impl Default for StatelessExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ToolExecutor for StatelessExecutor {
    async fn execute(
        &self,
        tool: &wf_types::tool::Tool,
        parameters: &Value,
        options: &ToolExecutionOptions,
        context: &ToolExecutionContext,
    ) -> ToolResult<ToolExecutionResult> {
        let start = Instant::now();
        BaseExecutor::validate_parameters(tool, parameters)?;

        let result = self.run_stateless(tool, parameters, options, context).await;

        let execution_time = start.elapsed().as_millis() as i64;
        match result {
            Ok(value) => Ok(BaseExecutor::build_result(
                true,
                Some(value),
                None,
                execution_time,
                0,
            )),
            Err(e) => Ok(BaseExecutor::build_result(
                false,
                None,
                Some(e.to_string()),
                execution_time,
                0,
            )),
        }
    }

    fn executor_type(&self) -> &str {
        "stateless"
    }
}

impl StatelessExecutor {
    async fn run_stateless(
        &self,
        tool: &wf_types::tool::Tool,
        parameters: &Value,
        _options: &ToolExecutionOptions,
        context: &ToolExecutionContext,
    ) -> ToolResult<Value> {
        if let Some(handler) = self.async_handlers.get(&tool.id) {
            return handler(parameters.clone(), context.clone()).await;
        }
        if let Some(handler) = self.async_handlers.get(&tool.name) {
            return handler(parameters.clone(), context.clone()).await;
        }
        if let Some(handler) = self.handlers.get(&tool.id) {
            return handler(parameters, context);
        }
        if let Some(handler) = self.handlers.get(&tool.name) {
            return handler(parameters, context);
        }
        Err(ToolError::NotFound(format!(
            "No handler registered for stateless tool '{}' (id: {})",
            tool.name, tool.id
        )))
    }
}
