use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::time::Instant;

use crate::error::ToolResult;
use wf_types::tool::ToolExecutionOptions;
use wf_types::tool::ToolExecutionResult;
use wf_types::Id;

#[derive(Debug, Clone)]
pub struct ToolExecutionContext {
    pub execution_id: Id,
    pub node_id: Option<String>,
    pub metadata: HashMap<String, Value>,
}

impl ToolExecutionContext {
    pub fn new(execution_id: Id) -> Self {
        Self {
            execution_id,
            node_id: None,
            metadata: HashMap::new(),
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
        let max_retries = options.retries.unwrap_or(0);
        let retry_delay = options.retry_delay.unwrap_or(1000);
        let exponential_backoff = options.exponential_backoff.unwrap_or(false);
        async move {
            let mut retry_count = 0;
            loop {
                let result = self.execute(tool, parameters, options, context).await;

                match &result {
                    Ok(r) if r.success => return result,
                    _ => {
                        if retry_count >= max_retries {
                            return result;
                        }
                        retry_count += 1;

                        let delay = if exponential_backoff {
                            retry_delay * 2u64.pow(retry_count - 1)
                        } else {
                            retry_delay
                        };
                        tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
                    }
                }
            }
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
            let result = tokio::time::timeout(
                std::time::Duration::from_millis(timeout_ms),
                self.execute(tool, parameters, options, context),
            )
            .await;

            match result {
                Ok(Ok(r)) => Ok(r),
                Ok(Err(e)) => Err(e),
                Err(_) => Ok(ToolExecutionResult {
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
