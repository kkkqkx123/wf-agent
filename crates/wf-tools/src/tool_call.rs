use serde_json::Value;
use std::sync::Arc;
use tokio::sync::mpsc;

use crate::error::ToolResult;
use crate::executor::trait_def::{ToolExecutionContext, ToolExecutorExt};
use crate::registry::ToolRegistry;
use wf_types::tool::{
    ToolCall, ToolExecutionOptions, ToolExecutionResult,
};

#[derive(Debug, Clone)]
pub enum ToolCallEvent {
    Started {
        tool_id: String,
        call_id: String,
        timestamp: i64,
    },
    Completed {
        tool_id: String,
        call_id: String,
        success: bool,
        execution_time_ms: i64,
    },
    Failed {
        tool_id: String,
        call_id: String,
        error: String,
    },
}

pub struct ToolCallExecutor {
    registry: Arc<ToolRegistry>,
    event_tx: Option<mpsc::Sender<ToolCallEvent>>,
}

impl ToolCallExecutor {
    pub fn new(registry: Arc<ToolRegistry>) -> Self {
        Self {
            registry,
            event_tx: None,
        }
    }

    pub fn with_event_channel(registry: Arc<ToolRegistry>, event_tx: mpsc::Sender<ToolCallEvent>) -> Self {
        Self {
            registry,
            event_tx: Some(event_tx),
        }
    }

    fn emit(&self, event: ToolCallEvent) {
        if let Some(ref tx) = self.event_tx {
            let _ = tx.try_send(event);
        }
    }

    pub async fn execute_single(
        &self,
        tool_id: &str,
        parameters: &Value,
        options: &ToolExecutionOptions,
        execution_id: &str,
        call_id: &str,
    ) -> ToolResult<ToolExecutionResult> {
        self.emit(ToolCallEvent::Started {
            tool_id: tool_id.to_string(),
            call_id: call_id.to_string(),
            timestamp: wf_common::time::now(),
        });

        let context = ToolExecutionContext::new(execution_id.to_string());
        let tool = self
            .registry
            .get_tool(tool_id)
            .ok_or_else(|| crate::error::ToolError::NotFound(tool_id.to_string()))?;

        let result = match self.registry.get_executor(&tool) {
            Ok(executor) => {
                executor
                    .execute_with_timeout(&tool, parameters, options, &context)
                    .await
            }
            Err(e) => Err(e),
        };

        match &result {
            Ok(r) => self.emit(ToolCallEvent::Completed {
                tool_id: tool_id.to_string(),
                call_id: call_id.to_string(),
                success: r.success,
                execution_time_ms: r.execution_time,
            }),
            Err(e) => self.emit(ToolCallEvent::Failed {
                tool_id: tool_id.to_string(),
                call_id: call_id.to_string(),
                error: e.to_string(),
            }),
        }

        result
    }

    pub async fn execute_with_retry(
        &self,
        tool_id: &str,
        parameters: &Value,
        options: &ToolExecutionOptions,
        execution_id: &str,
        call_id: &str,
    ) -> ToolResult<ToolExecutionResult> {
        self.emit(ToolCallEvent::Started {
            tool_id: tool_id.to_string(),
            call_id: call_id.to_string(),
            timestamp: wf_common::time::now(),
        });

        let context = ToolExecutionContext::new(execution_id.to_string());
        let tool = self
            .registry
            .get_tool(tool_id)
            .ok_or_else(|| crate::error::ToolError::NotFound(tool_id.to_string()))?;

        let result = match self.registry.get_executor(&tool) {
            Ok(executor) => {
                executor
                    .execute_with_retry(&tool, parameters, options, &context)
                    .await
            }
            Err(e) => Err(e),
        };

        match &result {
            Ok(r) => self.emit(ToolCallEvent::Completed {
                tool_id: tool_id.to_string(),
                call_id: call_id.to_string(),
                success: r.success,
                execution_time_ms: r.execution_time,
            }),
            Err(e) => self.emit(ToolCallEvent::Failed {
                tool_id: tool_id.to_string(),
                call_id: call_id.to_string(),
                error: e.to_string(),
            }),
        }

        result
    }

    pub async fn execute_batch(
        &self,
        tool_calls: &[ToolCall],
        options: &ToolExecutionOptions,
        execution_id: &str,
    ) -> Vec<(String, ToolResult<ToolExecutionResult>)> {
        let mut results = Vec::with_capacity(tool_calls.len());

        for call in tool_calls {
            let result = self
                .execute_single(&call.tool_id, &call.parameters, options, execution_id, &call.id)
                .await;
            results.push((call.id.clone(), result));
        }

        results
    }

    pub async fn execute_batch_parallel(
        &self,
        tool_calls: &[ToolCall],
        options: &ToolExecutionOptions,
        execution_id: &str,
    ) -> Vec<(String, ToolResult<ToolExecutionResult>)> {
        let futures: Vec<_> = tool_calls
            .iter()
            .map(|call| {
                let tool_id = call.tool_id.clone();
                let parameters = call.parameters.clone();
                let call_id = call.id.clone();
                let options = options.clone();
                let exec_id = execution_id;
                let this = self.clone();
                async move {
                    let result = this.execute_single(&tool_id, &parameters, &options, exec_id, &call_id).await;
                    (call.id.clone(), result)
                }
            })
            .collect();

        futures::future::join_all(futures).await
    }

    pub fn registry(&self) -> &ToolRegistry {
        &self.registry
    }
}

impl Clone for ToolCallExecutor {
    fn clone(&self) -> Self {
        Self {
            registry: self.registry.clone(),
            event_tx: self.event_tx.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::executor::StatelessHandler;
    use wf_types::tool::{Tool, ToolType};

    fn make_test_tool(id: &str, name: &str) -> Tool {
        Tool {
            id: id.into(),
            name: name.into(),
            description: format!("Test tool: {}", name),
            tool_type: ToolType::Stateless,
            parameters: None,
            metadata: None,
            config: None,
            enabled: Some(true),
            strict: None,
            default_timeout_ms: None,
        }
    }

    fn register_echo_handler(registry: &ToolRegistry, tool_id: &str) {
        let handler: StatelessHandler = Arc::new(|params, _ctx| {
            Ok(serde_json::json!({
                "status": "executed",
                "parameters": params,
            }))
        });
        registry.register_stateless_handler(tool_id, handler);
    }

    #[tokio::test]
    async fn test_execute_single() {
        let registry = Arc::new(ToolRegistry::new());
        registry.register_tool(make_test_tool("t1", "test_tool"));
        register_echo_handler(&registry, "t1");

        let executor = ToolCallExecutor::new(registry);
        let options = ToolExecutionOptions {
            timeout: Some(5000),
            retries: None,
            retry_delay: None,
            exponential_backoff: None,
        };

        let result = executor
            .execute_single("t1", &serde_json::json!({"key": "value"}), &options, "exec1", "c1")
            .await;

        assert!(result.is_ok());
        let exec_result = result.unwrap();
        assert!(exec_result.success);
    }

    #[tokio::test]
    async fn test_execute_not_found() {
        let registry = Arc::new(ToolRegistry::new());
        let executor = ToolCallExecutor::new(registry);
        let options = ToolExecutionOptions {
            timeout: Some(5000),
            retries: None,
            retry_delay: None,
            exponential_backoff: None,
        };

        let result = executor
            .execute_single("nonexistent", &serde_json::json!({}), &options, "exec1", "c1")
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_execute_batch() {
        let registry = Arc::new(ToolRegistry::new());
        registry.register_tool(make_test_tool("t1", "tool1"));
        registry.register_tool(make_test_tool("t2", "tool2"));
        register_echo_handler(&registry, "t1");
        register_echo_handler(&registry, "t2");

        let executor = ToolCallExecutor::new(registry);
        let options = ToolExecutionOptions {
            timeout: Some(5000),
            retries: None,
            retry_delay: None,
            exponential_backoff: None,
        };

        let calls = vec![
            ToolCall {
                id: "c1".into(),
                tool_id: "t1".into(),
                tool_name: Some("tool1".into()),
                parameters: serde_json::json!({"a": 1}),
                result: None,
                error: None,
                timestamp: wf_common::time::now(),
                execution_time: None,
            },
            ToolCall {
                id: "c2".into(),
                tool_id: "t2".into(),
                tool_name: Some("tool2".into()),
                parameters: serde_json::json!({"b": 2}),
                result: None,
                error: None,
                timestamp: wf_common::time::now(),
                execution_time: None,
            },
        ];

        let results = executor.execute_batch(&calls, &options, "exec1").await;
        assert_eq!(results.len(), 2);
        assert!(results[0].1.is_ok());
        assert!(results[1].1.is_ok());
    }
}
