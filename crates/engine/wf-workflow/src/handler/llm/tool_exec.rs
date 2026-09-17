use std::sync::Arc;

use serde_json::Value;
use wf_agent::VariableBackedVisibilityStore;
use wf_execution_shared::context::NodeExecutionContext;
use wf_types::message::{LlmToolCall, Message};

use super::events::publish_forced_compression;
use super::messages::tool_result_message;
use crate::error::{WorkflowError, WorkflowResult};

/// Single LLM call. Returns the model response.
///
/// Safety-net path: when the provider rejects the *actual* request with
/// a context-length-exceeded error (the local estimate undercounted), a
/// forced CONTEXT_COMPRESSION_REQUESTED is published over the real request
/// messages so the compression chain still fires.
pub async fn call_llm(
    ctx: &NodeExecutionContext,
    gateway: &wf_llm::LlmGateway,
    request: &wf_types::llm::LlmRequest,
) -> WorkflowResult<wf_types::llm::LlmResult> {
    match gateway.generate(request, ctx.cancellation.clone()).await {
        Ok(result) => Ok(result),
        Err(e) if e.is_context_length_exceeded() => {
            publish_forced_compression(ctx, request).await;
            Err(WorkflowError::Internal(format!("LLM call failed: {}", e)))
        }
        Err(e) => Err(WorkflowError::Internal(format!("LLM call failed: {}", e))),
    }
}

/// Resolve declared tool names against the tool registry.
pub fn resolve_tools(ctx: &NodeExecutionContext) -> WorkflowResult<Vec<wf_types::tool::Tool>> {
    let config = ctx.node_config.as_ref().unwrap_or(&Value::Null);
    let Some(names) = config.get("tools").and_then(|v| v.as_array()) else {
        return Ok(Vec::new());
    };
    let names: Vec<String> = names
        .iter()
        .filter_map(|v| v.as_str().map(String::from))
        .collect();
    if names.is_empty() {
        return Ok(Vec::new());
    }

    let registry = ctx.tool_registry.as_ref().ok_or_else(|| {
        WorkflowError::OperationError(
            "LLM node declares tools but no tool registry is available".to_string(),
        )
    })?;

    let mut tools = Vec::new();
    for name in names {
        match registry.get_tool(&name) {
            Some(tool) => tools.push(tool),
            None => {
                return Err(WorkflowError::OperationError(format!(
                    "Tool '{}' declared by LLM node is not registered",
                    name
                )))
            }
        }
    }
    Ok(tools)
}

/// Batch context for one LLM response's tool calls, mirroring the agent
/// gate's batch semantics so parallel calls share one approval view.
pub struct LlmToolCallBatch {
    pub batch_id: String,
    pub index: u32,
    pub total: u32,
    pub pending_queue: Vec<wf_types::interaction::tool_approval::PendingToolCallInfo>,
}

/// Pending-call queue for a response's tool calls, with registry risk levels
/// attached so the approval view matches the agent path.
pub fn pending_queue_for(
    calls: &[LlmToolCall],
    registry: Option<&Arc<wf_tools::registry::ToolRegistry>>,
) -> Vec<wf_types::interaction::tool_approval::PendingToolCallInfo> {
    calls
        .iter()
        .map(|call| {
            let arguments = serde_json::from_str(&call.function.arguments).ok();
            let risk_level = registry
                .and_then(|registry| registry.get_tool(&call.function.name))
                .and_then(|tool| tool.metadata)
                .and_then(|m| m.risk_level);
            wf_types::interaction::tool_approval::PendingToolCallInfo {
                id: call.id.clone(),
                name: call.function.name.clone(),
                arguments,
                risk_level,
            }
        })
        .collect()
}

/// Execute one tool call through the registry, returning a Tool message.
/// File-tool and shell writes are attributed to the same workflow-level
/// actor the script nodes use (`resolve_actor(execution, parent)`), via the
/// handler's file-checkpoint observer. Calls without a manager keep plain
/// tool behavior with no invented attribution.
pub async fn execute_tool_call(
    ctx: &NodeExecutionContext,
    call: &wf_types::message::LlmToolCall,
    file_checkpoint: Option<&wf_checkpoint::file::FileCheckpointManager>,
    batch: Option<&LlmToolCallBatch>,
) -> Message {
    let tool_name = call.function.name.clone();
    let args: Value = serde_json::from_str(&call.function.arguments).unwrap_or(Value::Null);

    // Runtime visibility gate (aligned with the AGENT_LOOP path): tools
    // blocked by TOOL_VISIBILITY nodes are rejected here, before the
    // approval gate, so plain LLM nodes cannot bypass the block. The
    // model-visible schema stays unchanged (KV-cache friendly); blocked
    // calls are intercepted at execution time only.
    let visibility_store = VariableBackedVisibilityStore::new(ctx.variables.clone());
    if visibility_store.is_blocked(&tool_name) {
        return tool_result_message(
            &call.id,
            &tool_name,
            format!("Tool \"{tool_name}\" is not visible in this execution"),
            true,
        );
    }

    let mut tool_ctx =
        wf_tools::executor::trait_def::ToolExecutionContext::new(ctx.execution_id.clone())
            .with_node_id(ctx.node_id.clone())
            .with_cancellation(ctx.cancellation.clone());
    if let Some(manager) = file_checkpoint {
        let parent = ctx.parent_execution_id.as_ref().map(|id| id.to_string());
        let session = wf_checkpoint::CheckpointSession::new(
            manager.clone(),
            &ctx.execution_id.to_string(),
            parent.as_deref(),
        )
        .expect("failed to build checkpoint session");
        tool_ctx = tool_ctx.with_checkpoint_session(Some(session));
    }
    let options = wf_types::tool::ToolExecutionOptions {
        timeout: None,
        retries: None,
        retry_delay: None,
        exponential_backoff: None,
    };

    // Tool-level approval gate (pre-execution side-effect guard, mirroring
    // the agent gate): the policy engine decides first; denials are final
    // and never reach the handler; approvals execute; only `Ask` consults
    // the external handler, failing closed when none is attached. With
    // neither options nor handler the call is auto-approved (library
    // opt-in default, same as the agent fast path).
    let (registered_tool, risk_level, tool_description) = ctx
        .tool_registry
        .as_ref()
        .and_then(|registry| registry.get_tool(&tool_name))
        .map(|tool| {
            let risk = tool
                .metadata
                .as_ref()
                .and_then(|m| m.risk_level)
                .map(|level| level.as_str().to_string());
            (Some(tool.clone()), risk, Some(tool.description.clone()))
        })
        .unwrap_or((None, None, None));
    // A handler without explicit options falls back to ask-everything over
    // the default sensitive-file rules, like the agent gate, so attaching
    // a handler never silently weakens the baseline.
    let effective_options: Option<wf_types::tool::approval::ToolApprovalOptions> = match &ctx
        .tool_approval_options
    {
        Some(approval_options) => {
            let mut approval_options = approval_options.clone();
            if approval_options.file_permissions.is_none() {
                approval_options.file_permissions =
                    Some(wf_types::tool::file_permission::FilePermissionSettings::default_rules());
            }
            Some(approval_options)
        }
        None => {
            if ctx.tool_approval_handler.is_some() {
                Some(wf_types::tool::approval::ToolApprovalOptions::handler_fallback())
            } else {
                None
            }
        }
    };
    let approved = match effective_options {
        None => Ok(None),
        Some(approval_options) => {
            let request_data = wf_types::interaction::tool_approval::ToolApprovalRequestData {
                tool_call_id: call.id.clone(),
                tool_name: tool_name.clone(),
                tool_description: tool_description.clone(),
                parameters: args.clone(),
                risk_level: risk_level.clone(),
                pending_queue: batch.map(|b| b.pending_queue.clone()),
                batch_id: batch.map(|b| b.batch_id.clone()),
                tool_index: batch.map(|b| b.index),
                total_tools: batch.map(|b| b.total),
            };
            let coordinator = wf_tools::approval::ToolApprovalCoordinator::new(approval_options);
            let mcp_manager = ctx
                .tool_registry
                .as_ref()
                .and_then(|registry| registry.mcp_manager());
            let mcp_registry = mcp_manager.as_ref().map(|m| m.registry().as_ref());
            let mcp_context = wf_tools::approval::McpToolContext {
                tool: registered_tool.as_ref(),
                mcp_registry,
            };
            let decision = coordinator
                .evaluate_with_mcp_context(
                    std::slice::from_ref(&request_data),
                    std::slice::from_ref(&mcp_context),
                )
                .remove(0);
            match decision {
                wf_tools::approval::ApprovalDecision::Approve => Ok(None),
                wf_tools::approval::ApprovalDecision::Deny(reason) => Err(reason),
                wf_tools::approval::ApprovalDecision::Ask => {
                    match &ctx.tool_approval_handler {
                        Some(handler) => {
                            let interaction_id =
                                format!("approval-{}-{}", wf_common::now(), call.id);
                            let request =
                                wf_execution_shared::approval::ToolApprovalRequest {
                                    tool_call_id: call.id.clone(),
                                    tool_name: tool_name.clone(),
                                    arguments: args.clone(),
                                    interaction_id,
                                    risk_level: risk_level.clone(),
                                    tool_description: tool_description.clone(),
                                    batch_id: batch.map(|b| b.batch_id.clone()),
                                    tool_index: batch.map(|b| b.index),
                                    total_tools: batch.map(|b| b.total),
                                    pending_queue: batch.map(|b| b.pending_queue.clone()),
                                };
                            let result = handler.request_approval(&request).await;
                            match result.approved {
                                true => Ok(result.edited_parameters),
                                false => Err(result.rejection_reason.unwrap_or_else(|| {
                                    "Rejected by user".to_string()
                                })),
                            }
                        }
                        None => Err(format!(
                            "No approval handler configured. Tool \"{tool_name}\" requires manual approval but no handler is registered."
                        )),
                    }
                }
            }
        }
    };

    let effective_args = match approved {
        Ok(edited) => edited.unwrap_or(args),
        Err(reason) => {
            return tool_result_message(
                &call.id,
                &tool_name,
                format!("Tool \"{tool_name}\" execution rejected: {reason}"),
                true,
            );
        }
    };

    let result = match &ctx.tool_registry {
        Some(registry) => match ctx.cancellation.clone() {
            Some(token) => {
                tokio::select! {
                    result = registry.execute_tool(&tool_name, &effective_args, &options, &tool_ctx) => result,
                    _ = token.cancelled() => Err(wf_tools::error::ToolError::Cancelled {
                        tool_id: tool_name.clone(),
                    }),
                }
            }
            None => {
                registry
                    .execute_tool(&tool_name, &effective_args, &options, &tool_ctx)
                    .await
            }
        },
        None => Err(wf_tools::error::ToolError::NotFound(tool_name.clone())),
    };

    match result {
        Ok(exec_result) => {
            let is_error = !exec_result.success;
            let content = exec_result
                .result
                .map(|v| v.to_string())
                .unwrap_or_else(|| "".to_string());
            tool_result_message(&call.id, &tool_name, content, is_error)
        }
        Err(e) => tool_result_message(&call.id, &tool_name, format!("Error: {}", e), true),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use wf_execution_shared::context::NodeExecutionContext;
    use wf_types::message::MessageContentValue;
    use wf_types::message::MessageRole;
    use wf_types::node::StaticNodeType;

    #[test]
    fn resolves_no_tools_without_config() {
        let vars = std::sync::Arc::new(dashmap::DashMap::new());
        let ctx = NodeExecutionContext::new(
            wf_types::Id::new(),
            "llm1".to_string(),
            StaticNodeType::Llm,
            Value::Null,
            vars,
        )
        .with_node_config(serde_json::json!({}));
        assert!(resolve_tools(&ctx).unwrap().is_empty());
    }

    #[test]
    fn unknown_tool_errors() {
        let vars = std::sync::Arc::new(dashmap::DashMap::new());
        let registry = std::sync::Arc::new(wf_tools::registry::ToolRegistry::new());
        let ctx = NodeExecutionContext::new(
            wf_types::Id::new(),
            "llm1".to_string(),
            StaticNodeType::Llm,
            Value::Null,
            vars,
        )
        .with_node_config(serde_json::json!({
            "tools": ["missing_tool"]
        }));
        let mut ctx = ctx;
        ctx.tool_registry = Some(registry);
        let err = resolve_tools(&ctx).unwrap_err();
        assert!(err.to_string().contains("missing_tool"));
    }

    #[tokio::test]
    async fn blocked_tool_is_intercepted_before_execution() {
        let vars = std::sync::Arc::new(dashmap::DashMap::new());
        vars.insert(
            format!("{}{}", wf_agent::BLOCKED_VARIABLE_PREFIX, "shell"),
            serde_json::json!(true),
        );
        let ctx = NodeExecutionContext::new(
            wf_types::Id::new(),
            "llm1".to_string(),
            StaticNodeType::Llm,
            Value::Null,
            vars,
        );

        let call = wf_types::message::LlmToolCall {
            id: "call-1".to_string(),
            r#type: "function".to_string(),
            function: wf_types::message::LlmFunctionCall {
                name: "shell".to_string(),
                arguments: "{}".to_string(),
            },
        };
        let result = execute_tool_call(&ctx, &call, None, None).await;
        assert_eq!(result.role, MessageRole::Tool);
        assert_eq!(result.tool_call_id.as_deref(), Some("call-1"));
        let is_error = result
            .metadata
            .as_ref()
            .and_then(|m| m.get("is_error"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        assert!(is_error, "blocked call must surface as an error");
        assert!(
            matches!(&result.content, MessageContentValue::Text(t) if t.contains("not visible"))
        );
    }

    #[tokio::test]
    async fn unblocked_tool_executes_but_blocked_same_tool_is_rejected() {
        struct EchoBuiltin;
        #[async_trait]
        impl wf_tools::executor::BuiltinToolHandler for EchoBuiltin {
            fn tool_name(&self) -> &'static str {
                "echo_tool"
            }
            async fn handle(
                &self,
                parameters: &Value,
                _context: &wf_tools::executor::trait_def::ToolExecutionContext,
                _resources: &wf_tools::executor::BuiltinHandlerResources,
            ) -> wf_tools::error::ToolResult<Value> {
                Ok(parameters.clone())
            }
        }

        let vars = std::sync::Arc::new(dashmap::DashMap::new());
        let registry = std::sync::Arc::new(wf_tools::registry::ToolRegistry::new());
        registry.register_builtin_handler("echo_tool", std::sync::Arc::new(EchoBuiltin));
        let mut tool = wf_types::tool::Tool {
            id: wf_types::Id::from("echo_tool"),
            name: "echo_tool".to_string(),
            description: "echo".to_string(),
            tool_type: wf_types::tool::ToolType::BuiltIn,
            parameters: None,
            metadata: None,
            config: None,
            enabled: Some(true),
            strict: None,
            default_timeout_ms: None,
        };
        tool.parameters = Some(wf_types::tool::ToolParameterSchema {
            r#type: "object".into(),
            properties: Default::default(),
            required: Vec::new(),
            additional_properties: Some(true),
        });
        registry.register_tool(tool);

        let mut ctx = NodeExecutionContext::new(
            wf_types::Id::new(),
            "llm1".to_string(),
            StaticNodeType::Llm,
            Value::Null,
            vars.clone(),
        );
        ctx.tool_registry = Some(registry);

        let call = wf_types::message::LlmToolCall {
            id: "call-1".to_string(),
            r#type: "function".to_string(),
            function: wf_types::message::LlmFunctionCall {
                name: "echo_tool".to_string(),
                arguments: "{\"x\": 1}".to_string(),
            },
        };

        let ok = execute_tool_call(&ctx, &call, None, None).await;
        let is_error = ok
            .metadata
            .as_ref()
            .and_then(|m| m.get("is_error"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        assert!(!is_error, "unblocked tool must execute");

        vars.insert(
            format!("{}{}", wf_agent::BLOCKED_VARIABLE_PREFIX, "echo_tool"),
            serde_json::json!(true),
        );
        let blocked = execute_tool_call(&ctx, &call, None, None).await;
        assert!(
            matches!(&blocked.content, MessageContentValue::Text(t) if t.contains("not visible"))
        );
    }
}
