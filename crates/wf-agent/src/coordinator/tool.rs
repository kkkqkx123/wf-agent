use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use futures::future::join_all;
use serde_json::Value;

use wf_execution_shared::hooks::executor::HookExecutor;
use wf_execution_shared::hooks::types::{BaseHookContext, HookExecutorConfig};
use wf_metrics::MetricsRegistry;
use wf_tools::approval::ToolApprovalCoordinator;
use wf_tools::registry::ToolRegistry;
use wf_types::interaction::tool_approval::{PendingToolCallInfo, ToolApprovalRequestData};
use wf_types::message::{LlmToolCall, Message, MessageContentValue, MessageRole};
use wf_types::tool::approval::ToolApprovalOptions;
use wf_types::tool::ToolExecutionOptions;
use wf_types::tool::ToolRiskLevel;

use crate::approval::{RejectionMessageBuilder, ToolApprovalHandler, ToolApprovalRequest};
use crate::entity::AgentLoopEntity;
use crate::error::AgentResult;
use crate::hook::AgentHookHandler;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum ToolExecutionMode {
    #[default]
    Sequential,
    Parallel,
}

/// Per-tool-call decision produced by the approval engine.
#[derive(Debug, Clone)]
enum ApprovalOutcome {
    Execute { edited_parameters: Option<Value> },
    Rejected { reason: String },
}

pub struct ToolExecutionCoordinator {
    tool_registry: Arc<ToolRegistry>,
    hook_executor: Arc<HookExecutor>,
    mode: ToolExecutionMode,
    metrics: Option<Arc<MetricsRegistry>>,
    approval_options: Option<ToolApprovalOptions>,
    approval_handler: Option<Arc<dyn ToolApprovalHandler>>,
    rejection_builder: RejectionMessageBuilder,
}

impl ToolExecutionCoordinator {
    pub fn new(tool_registry: Arc<ToolRegistry>, hook_executor: Arc<HookExecutor>) -> Self {
        Self {
            tool_registry,
            hook_executor,
            mode: ToolExecutionMode::default(),
            metrics: None,
            approval_options: None,
            approval_handler: None,
            rejection_builder: RejectionMessageBuilder::new(),
        }
    }

    pub fn with_mode(mut self, mode: ToolExecutionMode) -> Self {
        self.mode = mode;
        self
    }

    pub fn with_metrics(mut self, metrics: Option<Arc<MetricsRegistry>>) -> Self {
        self.metrics = metrics;
        self
    }

    /// Register tool approval configuration. Without a handler and without
    /// explicit options every tool call is auto-approved (TS default).
    pub fn with_approval(
        mut self,
        options: Option<ToolApprovalOptions>,
        handler: Option<Arc<dyn ToolApprovalHandler>>,
    ) -> Self {
        self.approval_options = options;
        self.approval_handler = handler;
        self
    }

    pub fn with_rejection_builder(mut self, builder: RejectionMessageBuilder) -> Self {
        self.rejection_builder = builder;
        self
    }

    pub fn tool_registry(&self) -> &Arc<ToolRegistry> {
        &self.tool_registry
    }

    pub async fn execute_tool_calls(
        &self,
        entity: &AgentLoopEntity,
        tool_calls: &[LlmToolCall],
    ) -> AgentResult<Vec<Message>> {
        match self.mode {
            ToolExecutionMode::Sequential => self.execute_sequential(entity, tool_calls).await,
            ToolExecutionMode::Parallel => self.execute_parallel(entity, tool_calls).await,
        }
    }

    /// Run the approval engine for a batch of tool calls. Produces one
    /// outcome per tool call, in order.
    async fn approve_tool_calls(
        &self,
        entity: &AgentLoopEntity,
        tool_calls: &[LlmToolCall],
    ) -> Vec<ApprovalOutcome> {
        // Fast path: no handler and no options -> auto-approve everything.
        if self.approval_handler.is_none() && self.approval_options.is_none() {
            return tool_calls
                .iter()
                .map(|_| ApprovalOutcome::Execute {
                    edited_parameters: None,
                })
                .collect();
        }

        let requests: Vec<ToolApprovalRequestData> = tool_calls
            .iter()
            .map(|tc| ToolApprovalRequestData {
                tool_call_id: tc.id.clone(),
                tool_name: tc.function.name.clone(),
                tool_description: None,
                parameters: serde_json::from_str(&tc.function.arguments).unwrap_or(Value::Null),
                risk_level: Self::risk_level_of(&self.tool_registry, &tc.function.name),
                pending_queue: None,
                batch_id: None,
                tool_index: None,
                total_tools: None,
                timeout: None,
                security_preset: None,
            })
            .collect();

        // TS: when a handler is registered it controls the policy; without
        // explicit options fall back to ask-everything for the handler.
        let options = self
            .approval_options
            .clone()
            .unwrap_or_else(|| ToolApprovalOptions {
                auto_approval_enabled: Some(self.approval_handler.is_none()),
                security_preset: None,
                risk_threshold: None,
                auto_approve_patterns: None,
                categories: None,
                workspace_boundary: None,
                file_permissions: None,
                command: None,
                mcp: None,
                network: None,
                interaction: None,
                allow_write_protected: None,
            });

        let coordinator = ToolApprovalCoordinator::new(options);
        let batch = coordinator.process_batch(requests);

        let mut outcomes: Vec<ApprovalOutcome> = vec![
            ApprovalOutcome::Rejected {
                reason: "internal: unclassified".to_string(),
            };
            tool_calls.len()
        ];

        for idx in &batch.auto_approved {
            outcomes[*idx] = ApprovalOutcome::Execute {
                edited_parameters: None,
            };
        }

        for idx in &batch.pending {
            let tc = &tool_calls[*idx];
            let outcome = match self.approval_handler.as_ref() {
                Some(handler) => {
                    let interaction_id = format!(
                        "approval-{}-{}",
                        wf_common::now(),
                        tc.id
                    );
                    let request = ToolApprovalRequest {
                        tool_call_id: tc.id.clone(),
                        tool_name: tc.function.name.clone(),
                        arguments: serde_json::from_str(&tc.function.arguments)
                            .unwrap_or(Value::Null),
                        interaction_id,
                        batch_id: Some(batch.batch_id.clone()),
                        tool_index: Some(*idx as u32),
                        total_tools: Some(tool_calls.len() as u32),
                        pending_queue: Some(
                            batch
                                .pending
                                .iter()
                                .map(|p| PendingToolCallInfo {
                                    id: tool_calls[*p].id.clone(),
                                    name: tool_calls[*p].function.name.clone(),
                                    arguments: Some(
                                        serde_json::from_str(&tool_calls[*p].function.arguments)
                                            .unwrap_or(Value::Null),
                                    ),
                                    risk_level: None,
                                })
                                .collect(),
                        ),
                    };

                    // Approval waits must not consume the wall-clock budget.
                    let _guard = entity.timeout_manager().pause_handle();
                    let result = handler.request_approval(&request).await;
                    if result.approved {
                        ApprovalOutcome::Execute {
                            edited_parameters: result.edited_parameters,
                        }
                    } else {
                        ApprovalOutcome::Rejected {
                            reason: result
                                .rejection_reason
                                .unwrap_or_else(|| "Rejected by user".to_string()),
                        }
                    }
                }
                None => ApprovalOutcome::Rejected {
                    reason: format!(
                        "No approval handler configured. Tool \"{}\" requires manual approval but no handler is registered.",
                        tc.function.name
                    ),
                },
            };
            outcomes[*idx] = outcome;
        }

        outcomes
    }

    fn build_rejection_message(&self, tc: &LlmToolCall, reason: &str) -> Message {
        Message {
            id: wf_types::Id::new(),
            role: MessageRole::Tool,
            content: MessageContentValue::Text(serde_json::json!({
                "error": self.rejection_builder.build_rejection_message(&tc.function.name, Some(reason))
            })
            .to_string()),
            timestamp: wf_common::now(),
            tool_call_id: Some(tc.id.clone()),
            tool_name: Some(tc.function.name.clone()),
            tool_calls: None,
            thinking: None,
            metadata: None,
        }
    }

    fn risk_level_of(registry: &ToolRegistry, name: &str) -> Option<String> {
        registry
            .list_tools()
            .into_iter()
            .find(|t| t.name == name)
            .and_then(|t| t.metadata)
            .and_then(|m| m.risk_level)
            .map(|level| match level {
                ToolRiskLevel::ReadOnly => "read_only",
                ToolRiskLevel::Write => "write",
                ToolRiskLevel::Execute => "execute",
                ToolRiskLevel::Mcp => "mcp",
                ToolRiskLevel::Network => "network",
                ToolRiskLevel::System => "system",
                ToolRiskLevel::Interaction => "interaction",
            })
            .map(String::from)
    }

    async fn execute_sequential(
        &self,
        entity: &AgentLoopEntity,
        tool_calls: &[LlmToolCall],
    ) -> AgentResult<Vec<Message>> {
        let outcomes = self.approve_tool_calls(entity, tool_calls).await;
        let mut messages = Vec::with_capacity(tool_calls.len());

        for (idx, tc) in tool_calls.iter().enumerate() {
            let outcome = &outcomes[idx];
            match outcome {
                ApprovalOutcome::Rejected { reason } => {
                    AgentHookHandler::execute_agent_hook(
                        &self.hook_executor,
                        entity,
                        "BEFORE_TOOL_CALL",
                        Self::build_hook_data(tc),
                    )
                    .await
                    .map_err(|e| crate::error::AgentError::HookError(e.to_string()))?;
                    let msg = self.build_rejection_message(tc, reason);
                    let mut hook_data = Self::build_hook_data(tc);
                    hook_data.insert("error".to_string(), Value::String(reason.clone()));
                    AgentHookHandler::execute_agent_hook(
                        &self.hook_executor,
                        entity,
                        "AFTER_TOOL_CALL",
                        hook_data,
                    )
                    .await
                    .map_err(|e| crate::error::AgentError::HookError(e.to_string()))?;
                    messages.push(msg);
                    continue;
                }
                ApprovalOutcome::Execute { edited_parameters } => {
                    let mut tc = tc.clone();
                    if let Some(edited) = edited_parameters {
                        tc.function.arguments =
                            serde_json::to_string(edited).unwrap_or(tc.function.arguments);
                    }
                    AgentHookHandler::execute_agent_hook(
                        &self.hook_executor,
                        entity,
                        "BEFORE_TOOL_CALL",
                        Self::build_hook_data(&tc),
                    )
                    .await
                    .map_err(|e| crate::error::AgentError::HookError(e.to_string()))?;

                    let msg = self.execute_single_tool(entity, &tc).await?;

                    AgentHookHandler::execute_agent_hook(
                        &self.hook_executor,
                        entity,
                        "AFTER_TOOL_CALL",
                        Self::build_hook_data(&tc),
                    )
                    .await
                    .map_err(|e| crate::error::AgentError::HookError(e.to_string()))?;

                    messages.push(msg);
                }
            }
        }

        Ok(messages)
    }

    async fn execute_parallel(
        &self,
        entity: &AgentLoopEntity,
        tool_calls: &[LlmToolCall],
    ) -> AgentResult<Vec<Message>> {
        let outcomes = self.approve_tool_calls(entity, tool_calls).await;
        let mut messages: Vec<Option<Message>> = vec![None; tool_calls.len()];
        let mut tasks: Vec<(usize, tokio::task::JoinHandle<Message>)> = Vec::new();

        for (idx, tc) in tool_calls.iter().enumerate() {
            match &outcomes[idx] {
                ApprovalOutcome::Rejected { reason } => {
                    messages[idx] = Some(self.build_rejection_message(tc, reason));
                }
                ApprovalOutcome::Execute { edited_parameters } => {
                    let mut tool_call = tc.clone();
                    if let Some(edited) = edited_parameters {
                        tool_call.function.arguments =
                            serde_json::to_string(edited).unwrap_or(tool_call.function.arguments);
                    }
                    let tool_registry = self.tool_registry.clone();
                    let hook_executor = self.hook_executor.clone();
                    let entity_state = entity.state.clone();
                    let entity_hooks = entity.hooks().to_vec();
                    let entity_id = entity.id().clone();
                    let metrics = self.metrics.clone();

                    let task = tokio::spawn(async move {
                        let hook_data = Self::build_hook_data(&tool_call);

                        let _ = AgentHookHandler::execute_hooks(
                            &hook_executor,
                            &entity_hooks,
                            "BEFORE_TOOL_CALL",
                            &BaseHookContext {
                                execution_id: entity_id.clone(),
                                data: hook_data.clone(),
                            },
                            &HookExecutorConfig {
                                parallel: true,
                                continue_on_error: true,
                                warn_on_condition_failure: true,
                            },
                        )
                        .await;

                        let params: Value = serde_json::from_str(&tool_call.function.arguments)
                            .unwrap_or(Value::Null);
                        let result_msg = Self::build_result_msg(
                            &tool_registry,
                            &tool_call,
                            &params,
                            &entity_id,
                            &entity_state,
                            metrics.as_ref(),
                        )
                        .await;

                        let _ = AgentHookHandler::execute_hooks(
                            &hook_executor,
                            &entity_hooks,
                            "AFTER_TOOL_CALL",
                            &BaseHookContext {
                                execution_id: entity_id,
                                data: hook_data,
                            },
                            &HookExecutorConfig {
                                parallel: true,
                                continue_on_error: true,
                                warn_on_condition_failure: true,
                            },
                        )
                        .await;

                        result_msg
                    });

                    tasks.push((idx, task));
                }
            }
        }

        let results = join_all(tasks.iter_mut().map(|(_, t)| t)).await;
        for ((idx, _), result) in tasks.iter().zip(results) {
            messages[*idx] = match result {
                Ok(msg) => Some(msg),
                Err(e) => Some(Self::error_message(
                    &format!("Tool execution panicked: {}", e),
                    None,
                    None,
                )),
            };
        }

        Ok(messages.into_iter().flatten().collect())
    }

    /// Single-tool execution used by the streaming driver; execution errors
    /// surface as tool error messages rather than failures.
    pub async fn execute_single_tool_for_stream(
        &self,
        entity: &AgentLoopEntity,
        tc: &LlmToolCall,
    ) -> Message {
        self.execute_single_tool(entity, tc)
            .await
            .unwrap_or_else(|e| {
                Self::error_message(&e.to_string(), Some(&tc.id), Some(&tc.function.name))
            })
    }

    fn resolve_timeout(&self, tool_name: &str) -> u64 {
        if let Some(tool) = self
            .tool_registry
            .list_tools()
            .iter()
            .find(|t| t.name == tool_name)
        {
            if let Some(ms) = tool.default_timeout_ms {
                return ms;
            }
            if let Some(config) = &tool.config {
                if let Some(ms) = config.get("timeout").and_then(|v| v.as_u64()) {
                    return ms;
                }
            }
        }
        120_000
    }

    fn tool_execution_deadline(timeout_ms: u64) -> Duration {
        let safety_margin = 30_000;
        Duration::from_millis(timeout_ms + safety_margin)
    }

    async fn execute_single_tool(
        &self,
        entity: &AgentLoopEntity,
        tc: &LlmToolCall,
    ) -> AgentResult<Message> {
        let params: Value = serde_json::from_str(&tc.function.arguments).unwrap_or(Value::Null);
        let tool_id = Self::find_tool_id_by_name(&self.tool_registry, &tc.function.name);
        let timeout_ms = self.resolve_timeout(&tc.function.name);
        let execution_id = entity.id().clone();
        let tool_name = tc.function.name.clone();
        let parameter_size = json_size(&params);

        if let Some(ref metrics) = self.metrics {
            metrics
                .tool()
                .record_tool_call_start(&tool_name, &execution_id);
        }

        match tool_id {
            Some(tid) => {
                let ctx =
                    wf_tools::executor::trait_def::ToolExecutionContext::new(entity.id().clone());
                let options = ToolExecutionOptions {
                    timeout: Some(timeout_ms),
                    retries: None,
                    retry_delay: None,
                    exponential_backoff: None,
                };

                let start = wf_common::now();
                let result = tokio::time::timeout(
                    Self::tool_execution_deadline(timeout_ms),
                    self.tool_registry
                        .execute_tool(&tid, &params, &options, &ctx),
                )
                .await;
                let duration_ms = (wf_common::now() - start) as f64;
                let success = matches!(&result, Ok(Ok(_)));

                entity.state.write().await.record_tool_call(
                    &tool_name,
                    duration_ms as i64,
                    success,
                );

                if let Some(ref metrics) = self.metrics {
                    match &result {
                        Ok(Ok(tool_result)) => {
                            metrics.tool().record_tool_call_complete(
                                &tool_name,
                                &execution_id,
                                true,
                                duration_ms,
                                parameter_size,
                                json_size(tool_result.result.as_ref().unwrap_or(&Value::Null)),
                            );
                        }
                        Ok(Err(e)) => {
                            metrics.tool().record_tool_call_complete(
                                &tool_name,
                                &execution_id,
                                false,
                                duration_ms,
                                parameter_size,
                                0,
                            );
                            metrics.tool().record_tool_call_error(
                                &tool_name,
                                &execution_id,
                                "execution_failed",
                            );
                            tracing::warn!(tool = %tool_name, error = %e, "tool call failed");
                        }
                        Err(_) => {
                            metrics.tool().record_tool_call_complete(
                                &tool_name,
                                &execution_id,
                                false,
                                duration_ms,
                                parameter_size,
                                0,
                            );
                            metrics.tool().record_tool_call_error(
                                &tool_name,
                                &execution_id,
                                "timeout",
                            );
                            tracing::warn!(tool = %tool_name, "tool call timed out after {}ms", timeout_ms);
                        }
                    }
                }

                match result {
                    Ok(Ok(tool_result)) => Ok(Message {
                        id: wf_types::Id::new(),
                        role: MessageRole::Tool,
                        content: MessageContentValue::Text(
                            tool_result
                                .result
                                .as_ref()
                                .map(|v| v.to_string())
                                .unwrap_or_default(),
                        ),
                        timestamp: wf_common::now(),
                        tool_call_id: Some(tc.id.clone()),
                        tool_name: Some(tc.function.name.clone()),
                        tool_calls: None,
                        thinking: None,
                        metadata: None,
                    }),
                    Ok(Err(e)) => Ok(Self::error_message(
                        &e.to_string(),
                        Some(&tc.id),
                        Some(&tc.function.name),
                    )),
                    Err(_) => Ok(Self::error_message(
                        &format!(
                            "Tool '{}' timed out after {}ms",
                            tc.function.name, timeout_ms
                        ),
                        Some(&tc.id),
                        Some(&tc.function.name),
                    )),
                }
            }
            None => {
                if let Some(ref metrics) = self.metrics {
                    metrics
                        .tool()
                        .record_tool_call_error(&tool_name, &execution_id, "not_found");
                }
                Ok(Self::error_message(
                    &format!("Tool not found: {}", tc.function.name),
                    Some(&tc.id),
                    Some(&tc.function.name),
                ))
            }
        }
    }

    async fn build_result_msg(
        tool_registry: &ToolRegistry,
        tc: &LlmToolCall,
        params: &Value,
        entity_id: &str,
        entity_state: &tokio::sync::RwLock<crate::state::AgentLoopState>,
        metrics: Option<&Arc<MetricsRegistry>>,
    ) -> Message {
        let tool_id = Self::find_tool_id_by_name(tool_registry, &tc.function.name);
        let timeout_ms = tool_id
            .as_ref()
            .and_then(|tid| {
                tool_registry
                    .get_tool(tid)
                    .and_then(|t| t.default_timeout_ms)
            })
            .unwrap_or(120_000);
        let tool_name = tc.function.name.clone();
        let parameter_size = json_size(params);

        if let Some(metrics) = metrics {
            metrics.tool().record_tool_call_start(&tool_name, entity_id);
        }

        match tool_id {
            Some(tid) => {
                let ctx =
                    wf_tools::executor::trait_def::ToolExecutionContext::new(entity_id.to_string());
                let options = ToolExecutionOptions {
                    timeout: Some(timeout_ms),
                    retries: None,
                    retry_delay: None,
                    exponential_backoff: None,
                };
                let deadline = Duration::from_millis(timeout_ms + 30_000);

                let start = wf_common::now();
                let result = tokio::time::timeout(
                    deadline,
                    tool_registry.execute_tool(&tid, params, &options, &ctx),
                )
                .await;
                let duration_ms = (wf_common::now() - start) as f64;
                let success = matches!(&result, Ok(Ok(_)));

                entity_state.write().await.record_tool_call(
                    &tool_name,
                    duration_ms as i64,
                    success,
                );

                if let Some(metrics) = metrics {
                    match &result {
                        Ok(Ok(tool_result)) => {
                            metrics.tool().record_tool_call_complete(
                                &tool_name,
                                entity_id,
                                true,
                                duration_ms,
                                parameter_size,
                                json_size(tool_result.result.as_ref().unwrap_or(&Value::Null)),
                            );
                        }
                        Ok(Err(e)) => {
                            metrics.tool().record_tool_call_complete(
                                &tool_name,
                                entity_id,
                                false,
                                duration_ms,
                                parameter_size,
                                0,
                            );
                            metrics.tool().record_tool_call_error(
                                &tool_name,
                                entity_id,
                                "execution_failed",
                            );
                            tracing::warn!(tool = %tool_name, error = %e, "tool call failed");
                        }
                        Err(_) => {
                            metrics.tool().record_tool_call_complete(
                                &tool_name,
                                entity_id,
                                false,
                                duration_ms,
                                parameter_size,
                                0,
                            );
                            metrics
                                .tool()
                                .record_tool_call_error(&tool_name, entity_id, "timeout");
                            tracing::warn!(tool = %tool_name, "tool call timed out after {}ms", timeout_ms);
                        }
                    }
                }

                match result {
                    Ok(Ok(tool_result)) => Message {
                        id: wf_types::Id::new(),
                        role: MessageRole::Tool,
                        content: MessageContentValue::Text(
                            tool_result
                                .result
                                .as_ref()
                                .map(|v| v.to_string())
                                .unwrap_or_default(),
                        ),
                        timestamp: wf_common::now(),
                        tool_call_id: Some(tc.id.clone()),
                        tool_name: Some(tc.function.name.clone()),
                        tool_calls: None,
                        thinking: None,
                        metadata: None,
                    },
                    Ok(Err(e)) => {
                        Self::error_message(&e.to_string(), Some(&tc.id), Some(&tc.function.name))
                    }
                    Err(_) => Self::error_message(
                        &format!(
                            "Tool '{}' timed out after {}ms",
                            tc.function.name, timeout_ms
                        ),
                        Some(&tc.id),
                        Some(&tc.function.name),
                    ),
                }
            }
            None => {
                if let Some(metrics) = metrics {
                    metrics
                        .tool()
                        .record_tool_call_error(&tool_name, entity_id, "not_found");
                }
                Self::error_message(
                    &format!("Tool not found: {}", tc.function.name),
                    Some(&tc.id),
                    Some(&tc.function.name),
                )
            }
        }
    }

    fn error_message(error: &str, tool_call_id: Option<&str>, tool_name: Option<&str>) -> Message {
        Message {
            id: wf_types::Id::new(),
            role: MessageRole::Tool,
            content: MessageContentValue::Text(serde_json::json!({"error": error}).to_string()),
            timestamp: wf_common::now(),
            tool_call_id: tool_call_id.map(String::from),
            tool_name: tool_name.map(String::from),
            tool_calls: None,
            thinking: None,
            metadata: None,
        }
    }

    fn build_hook_data(tc: &LlmToolCall) -> HashMap<String, Value> {
        let mut data = HashMap::new();
        data.insert("tool_call_id".to_string(), Value::String(tc.id.clone()));
        data.insert(
            "tool_name".to_string(),
            Value::String(tc.function.name.clone()),
        );
        data.insert(
            "tool_arguments".to_string(),
            Value::String(tc.function.arguments.clone()),
        );
        data
    }

    fn find_tool_id_by_name(registry: &ToolRegistry, name: &str) -> Option<String> {
        registry
            .list_tools()
            .into_iter()
            .find(|t| t.name == name)
            .map(|t| t.id)
    }
}

/// Serialized size of a value in bytes, used for tool parameter/result metrics.
fn json_size(value: &Value) -> u64 {
    serde_json::to_string(value)
        .map(|s| s.len() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use wf_execution_shared::hooks::executor::HookExecutor;
    use wf_types::tool::Tool;
    use wf_types::Id;

    fn mock_tool_registry(executed: &Arc<AtomicU32>) -> Arc<ToolRegistry> {
        let registry = Arc::new(ToolRegistry::new());
        let handler: wf_tools::executor::stateless::StatelessHandler = {
            let executed = executed.clone();
            Arc::new(
                move |_params: &Value,
                      _ctx: &wf_tools::executor::trait_def::ToolExecutionContext| {
                    executed.fetch_add(1, Ordering::SeqCst);
                    Ok(Value::from("tool-result-ok"))
                },
            )
        };
        registry.register_tool(Tool {
            id: "tool-1".to_string(),
            name: "mock_write".to_string(),
            description: "mock tool".to_string(),
            tool_type: wf_types::tool::ToolType::Stateless,
            parameters: None,
            metadata: Some(wf_types::tool::ToolMetadata {
                category: Some("mock".to_string()),
                tags: None,
                documentation_url: None,
                custom_fields: None,
                risk_level: Some(ToolRiskLevel::Write),
                auto_approvable: None,
            }),
            config: None,
            enabled: Some(true),
            strict: None,
            default_timeout_ms: Some(5000),
        });
        registry.register_stateless_handler("tool-1", handler);
        registry
    }

    fn make_tool_call(id: &str, name: &str) -> LlmToolCall {
        LlmToolCall {
            id: id.to_string(),
            r#type: "function".to_string(),
            function: wf_types::message::LlmFunctionCall {
                name: name.to_string(),
                arguments: "{}".to_string(),
            },
        }
    }

    fn text_of(msg: &Message) -> String {
        match &msg.content {
            MessageContentValue::Text(t) => t.clone(),
            MessageContentValue::Rich(_) => String::new(),
        }
    }

    fn make_entity() -> AgentLoopEntity {
        AgentLoopEntity::new(Id::from("agent-approval-1".to_string()))
    }

    #[tokio::test]
    async fn test_no_handler_auto_approves_and_executes() {
        let executed = Arc::new(AtomicU32::new(0));
        let registry = mock_tool_registry(&executed);
        let hook_executor = Arc::new(HookExecutor::new());
        let coordinator = ToolExecutionCoordinator::new(registry, hook_executor);
        let entity = make_entity();

        let messages = coordinator
            .execute_tool_calls(&entity, &[make_tool_call("tc-1", "mock_write")])
            .await
            .expect("tool execution must succeed");

        assert_eq!(messages.len(), 1);
        assert_eq!(executed.load(Ordering::SeqCst), 1);
        assert!(text_of(&messages[0]).contains("tool-result-ok"));
    }

    struct RejectingHandler {
        reason: String,
    }

    #[async_trait::async_trait]
    impl crate::approval::ToolApprovalHandler for RejectingHandler {
        async fn request_approval(
            &self,
            request: &crate::approval::ToolApprovalRequest,
        ) -> crate::approval::ToolApprovalResult {
            crate::approval::ToolApprovalResult::rejected(
                request.tool_call_id.clone(),
                self.reason.clone(),
            )
        }
    }

    struct ApprovingHandler;

    #[async_trait::async_trait]
    impl crate::approval::ToolApprovalHandler for ApprovingHandler {
        async fn request_approval(
            &self,
            request: &crate::approval::ToolApprovalRequest,
        ) -> crate::approval::ToolApprovalResult {
            crate::approval::ToolApprovalResult::approved(request.tool_call_id.clone())
        }
    }

    #[tokio::test]
    async fn test_rejecting_handler_blocks_tool_and_produces_message() {
        let executed = Arc::new(AtomicU32::new(0));
        let registry = mock_tool_registry(&executed);
        let hook_executor = Arc::new(HookExecutor::new());
        let coordinator = ToolExecutionCoordinator::new(registry, hook_executor).with_approval(
            None,
            Some(Arc::new(RejectingHandler {
                reason: "too risky".to_string(),
            })),
        );
        let entity = make_entity();

        let messages = coordinator
            .execute_tool_calls(&entity, &[make_tool_call("tc-1", "mock_write")])
            .await
            .expect("rejection must not fail the loop");

        // Tool never executed; a rejection tool message is produced.
        assert_eq!(executed.load(Ordering::SeqCst), 0);
        assert_eq!(messages.len(), 1);
        let content = text_of(&messages[0]);
        assert!(content.contains("too risky"));
        assert!(content.contains("mock_write"));
    }

    #[tokio::test]
    async fn test_approving_handler_allows_execution() {
        let executed = Arc::new(AtomicU32::new(0));
        let registry = mock_tool_registry(&executed);
        let hook_executor = Arc::new(HookExecutor::new());
        let coordinator = ToolExecutionCoordinator::new(registry, hook_executor)
            .with_approval(None, Some(Arc::new(ApprovingHandler)));
        let entity = make_entity();

        let messages = coordinator
            .execute_tool_calls(&entity, &[make_tool_call("tc-1", "mock_write")])
            .await
            .expect("approved tool must execute");

        assert_eq!(executed.load(Ordering::SeqCst), 1);
        assert_eq!(messages.len(), 1);
        assert!(text_of(&messages[0]).contains("tool-result-ok"));
    }

    #[tokio::test]
    async fn test_parallel_approval_no_crosstalk() {
        let executed = Arc::new(AtomicU32::new(0));
        let registry = mock_tool_registry(&executed);
        // Second tool with the same underlying counter: only approved calls
        // reach execution.
        let registry2 = registry.clone();
        registry2.register_tool(Tool {
            id: "tool-2".to_string(),
            name: "mock_read".to_string(),
            description: "mock read".to_string(),
            tool_type: wf_types::tool::ToolType::Stateless,
            parameters: None,
            metadata: Some(wf_types::tool::ToolMetadata {
                category: Some("mock".to_string()),
                tags: None,
                documentation_url: None,
                custom_fields: None,
                risk_level: Some(ToolRiskLevel::ReadOnly),
                auto_approvable: None,
            }),
            config: None,
            enabled: Some(true),
            strict: None,
            default_timeout_ms: Some(5000),
        });
        {
            let handler: wf_tools::executor::stateless::StatelessHandler = {
                let executed = executed.clone();
                Arc::new(
                    move |_p: &Value, _c: &wf_tools::executor::trait_def::ToolExecutionContext| {
                        executed.fetch_add(1, Ordering::SeqCst);
                        Ok(Value::from("tool-result-ok"))
                    },
                )
            };
            registry2.register_stateless_handler("tool-2", handler);
        }
        let handler_executed = Arc::new(AtomicU32::new(0));
        let handler_executed_clone = handler_executed.clone();
        let handler = Arc::new(move |request: &crate::approval::ToolApprovalRequest| {
            handler_executed_clone.fetch_add(1, Ordering::SeqCst);
            crate::approval::ToolApprovalResult::approved(request.tool_call_id.clone())
        });

        struct FnHandler(
            Arc<
                dyn Fn(&crate::approval::ToolApprovalRequest) -> crate::approval::ToolApprovalResult
                    + Send
                    + Sync,
            >,
        );

        #[async_trait::async_trait]
        impl crate::approval::ToolApprovalHandler for FnHandler {
            async fn request_approval(
                &self,
                request: &crate::approval::ToolApprovalRequest,
            ) -> crate::approval::ToolApprovalResult {
                (self.0)(request)
            }
        }

        let coordinator =
            ToolExecutionCoordinator::new(registry.clone(), Arc::new(HookExecutor::new()))
                .with_mode(ToolExecutionMode::Parallel)
                .with_approval(None, Some(Arc::new(FnHandler(handler))));
        let entity = make_entity();

        let messages = coordinator
            .execute_tool_calls(
                &entity,
                &[
                    make_tool_call("tc-1", "mock_write"),
                    make_tool_call("tc-2", "mock_read"),
                ],
            )
            .await
            .expect("parallel approval must not fail");

        assert_eq!(messages.len(), 2);
        // Both asked the handler (no auto approval with handler present).
        assert_eq!(handler_executed.load(Ordering::SeqCst), 2);
        // Both approved and executed exactly once.
        assert_eq!(executed.load(Ordering::SeqCst), 2);
        assert!(messages
            .iter()
            .all(|m| text_of(m).contains("tool-result-ok")));
    }
}
