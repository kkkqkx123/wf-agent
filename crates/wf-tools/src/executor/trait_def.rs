use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use crate::error::ToolResult;
use crate::general::GeneralToolInvoker;
use wf_types::tool::ToolExecutionOptions;
use wf_types::tool::ToolExecutionResult;
use wf_types::Id;

#[derive(Clone)]
pub struct ToolExecutionContext {
    pub execution_id: Id,
    pub node_id: Option<String>,
    pub metadata: HashMap<String, Value>,
    /// Per-execution `general` tool invoker, injected by the engine when the
    /// execution context is built. Carried through the context so the
    /// builtin `general` handler can resolve its inner-tool invoker without
    /// consulting global per-execution state.
    pub general_invoker: Option<Arc<dyn GeneralToolInvoker>>,
}

impl std::fmt::Debug for ToolExecutionContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToolExecutionContext")
            .field("execution_id", &self.execution_id)
            .field("node_id", &self.node_id)
            .field("metadata", &self.metadata)
            .field("general_invoker", &self.general_invoker.is_some())
            .finish()
    }
}

impl ToolExecutionContext {
    pub fn new(execution_id: Id) -> Self {
        Self {
            execution_id,
            node_id: None,
            metadata: HashMap::new(),
            general_invoker: None,
        }
    }

    pub fn with_node_id(mut self, node_id: impl Into<String>) -> Self {
        self.node_id = Some(node_id.into());
        self
    }

    pub fn with_metadata(mut self, key: impl Into<String>, value: Value) -> Self {
        self.metadata.insert(key.into(), value);
        self
    }

    pub fn with_general_invoker(mut self, invoker: Arc<dyn GeneralToolInvoker>) -> Self {
        self.general_invoker = Some(invoker);
        self
    }
}

#[async_trait]
pub trait ToolExecutor: Send + Sync {
    async fn execute(
        &self,
        tool: &wf_types::tool::Tool,
        parameters: &Value,
        options: &ToolExecutionOptions,
        context: &ToolExecutionContext,
    ) -> ToolResult<ToolExecutionResult>;

    fn executor_type(&self) -> &str;

    async fn cleanup(&self) -> ToolResult<()> {
        Ok(())
    }
}

pub trait ToolExecutorExt: ToolExecutor {
    fn execute_with_retry(
        &self,
        tool: &wf_types::tool::Tool,
        parameters: &Value,
        options: &ToolExecutionOptions,
        context: &ToolExecutionContext,
    ) -> impl std::future::Future<Output = ToolResult<ToolExecutionResult>> + Send {
        let policy = wf_common::retry::RetryPolicy {
            max_retries: options.retries.unwrap_or(0),
            base_delay_ms: options.retry_delay.unwrap_or(1000),
            exponential_backoff: options.exponential_backoff.unwrap_or(false),
        };
        async move {
            wf_common::retry::execute_with_retry(
                Some(&policy),
                |r: &Result<ToolExecutionResult, crate::error::ToolError>| match r {
                    Ok(x) if x.success => false,
                    _ => true,
                },
                None,
                || self.execute(tool, parameters, options, context),
            )
            .await
        }
    }

    fn execute_with_timeout(
        &self,
        tool: &wf_types::tool::Tool,
        parameters: &Value,
        options: &ToolExecutionOptions,
        context: &ToolExecutionContext,
    ) -> impl std::future::Future<Output = ToolResult<ToolExecutionResult>> + Send {
        let timeout_ms = options.timeout.unwrap_or(30000);
        let start = Instant::now();
        async move {
            let result = wf_common::exec::execute_with_timeout(
                self.execute(tool, parameters, options, context),
                Some(timeout_ms),
            )
            .await;

            match result {
                Ok(r) => Ok(r),
                Err(wf_common::exec::TimeoutError::Failed(e)) => Err(e),
                Err(wf_common::exec::TimeoutError::TimedOut(_)) => Ok(ToolExecutionResult {
                    success: false,
                    result: None,
                    error: Some(format!(
                        "Tool '{}' timed out after {}ms",
                        tool.name, timeout_ms
                    )),
                    execution_time: start.elapsed().as_millis() as i64,
                    retry_count: 0,
                }),
            }
        }
    }
}

impl<T: ToolExecutor + ?Sized> ToolExecutorExt for T {}
