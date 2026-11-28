//! Read-only tool round for the conversation tree.
//!
//! The narrow driver the branch-tree design calls for in its step 7: one
//! model generation that *may* use tools, followed by at most one bounded
//! round of tool executions. Unlike [`generate_with_tools_once`] in
//! `single_shot`, a plain text answer is valid — a read-only round is a tool
//! the model *can* call, not one it *must* call. No checkpoint, no approval,
//! no workspace writes: tools execute through the caller-provided registry
//! with a bare execution context, so only the allow-list governs what runs.
//!
//! The round-trip messages (assistant `tool_calls` and `tool` results) are
//! returned so the tree driver can archive them as `reply.trace`, keeping
//! tool results in the context fold and in path projection.

use wf_llm::LlmGateway;
use wf_tools::registry::ToolRegistry;
use wf_types::llm::{LlmRequest, LlmResult};
use wf_types::message::Message;
use wf_types::tool::{ToolExecutionOptions, ToolExecutionResult};

use crate::error::{ExecutionSharedError, ExecutionSharedResult};
use crate::single_shot::MAX_SINGLE_SHOT_TOOL_CALLS;

/// Outcome of one read-only tool round.
#[derive(Debug, Clone)]
pub struct ReadonlyTurnOutcome {
    /// The model generation (text answer or tool-call request).
    pub generation: LlmResult,
    /// Executed tools, in call order.
    pub executions: Vec<ReadonlyToolExecution>,
    /// Ordered round-trip messages suitable for `reply.trace`.
    ///
    /// Empty when the model answered in plain text. When tools ran this is
    /// `[assistant(tool_calls), tool_result, ...]`.
    pub trace: Vec<Message>,
}

/// One executed tool call of the read-only round.
#[derive(Debug, Clone)]
pub struct ReadonlyToolExecution {
    pub tool_call_id: String,
    pub tool_name: String,
    pub result: ToolExecutionResult,
}

/// Run one generation and, when the model asks for them, execute allowed
/// tools once.
///
/// Rules:
/// - `allowed_tools` must be non-empty when the request declares tools;
/// - a tool the model emits but that is not on the allow-list is rejected
///   before execution, so no off-list tool ever runs;
/// - more than [`MAX_SINGLE_SHOT_TOOL_CALLS`] calls are rejected;
/// - a plain text answer (no tool calls) is a valid outcome;
/// - tools run without a checkpoint session.
pub async fn generate_readonly_turn(
    gateway: &LlmGateway,
    registry: &ToolRegistry,
    request: &LlmRequest,
    allowed_tools: &[String],
    tool_options: Option<ToolExecutionOptions>,
    execution_id: &str,
    cancel: Option<tokio_util::sync::CancellationToken>,
) -> ExecutionSharedResult<ReadonlyTurnOutcome> {
    if request.messages.is_empty() {
        return Err(ExecutionSharedError::Internal(
            "read-only turn requires at least one message".to_string(),
        ));
    }

    let generation = gateway.generate(request, cancel).await.map_err(|e| {
        ExecutionSharedError::Internal(format!("read-only turn generation failed: {e}"))
    })?;

    let mut calls = generation.tool_calls.clone().unwrap_or_default();
    if calls.is_empty() {
        calls = generation.message.tool_calls.clone().unwrap_or_default();
    }

    // Plain text answer: no tools to run, no trace to archive.
    if calls.is_empty() {
        return Ok(ReadonlyTurnOutcome {
            generation,
            executions: Vec::new(),
            trace: Vec::new(),
        });
    }

    // Tool calls present: the allow-list must be configured and respected.
    if allowed_tools.is_empty() {
        return Err(ExecutionSharedError::Internal(
            "read-only turn returned tool calls but no tool allow-list is configured".to_string(),
        ));
    }
    if calls.len() > MAX_SINGLE_SHOT_TOOL_CALLS {
        return Err(ExecutionSharedError::Internal(format!(
            "read-only turn returned {} tool calls, exceeding the bound of {MAX_SINGLE_SHOT_TOOL_CALLS}",
            calls.len(),
        )));
    }

    for call in &calls {
        let name = &call.function.name;
        if !allowed_tools.iter().any(|t| t == name) {
            return Err(ExecutionSharedError::Internal(format!(
                "read-only turn emitted tool '{name}' outside the allow-list"
            )));
        }
    }

    let base_options = tool_options.unwrap_or(ToolExecutionOptions {
        timeout: None,
        retries: None,
        retry_delay: None,
        exponential_backoff: None,
    });
    let context = wf_tools::executor::trait_def::ToolExecutionContext::new(execution_id.to_string());

    // The assistant message carrying the tool_calls is the head of the trace.
    let mut trace = vec![generation.message.clone()];
    let mut executions = Vec::with_capacity(calls.len());
    for call in &calls {
        let name = call.function.name.clone();
        let trimmed = call.function.arguments.trim();
        let parameters: serde_json::Value = if trimmed.is_empty() {
            serde_json::Value::Null
        } else {
            serde_json::from_str(trimmed).map_err(|e| {
                ExecutionSharedError::Internal(format!(
                    "read-only turn arguments for tool '{name}' are not valid JSON: {e}"
                ))
            })?
        };
        let mut options = base_options.clone();
        if options.timeout.is_none() {
            options.timeout = registry
                .get_tool(&name)
                .and_then(|tool| tool.default_timeout_ms);
        }
        let result = registry
            .execute_tool(&name, &parameters, &options, &context)
            .await
            .map_err(|e| {
                ExecutionSharedError::Internal(format!(
                    "read-only turn tool '{name}' execution failed: {e}"
                ))
            })?;

        let content = match &result.result {
            Some(value) => value.to_string(),
            None => result.error.clone().unwrap_or_default(),
        };
        trace.push(Message::tool_result(
            call.id.clone(),
            Some(name.clone()),
            content,
            !result.success,
        ));
        executions.push(ReadonlyToolExecution {
            tool_call_id: call.id.clone(),
            tool_name: name,
            result,
        });
    }

    Ok(ReadonlyTurnOutcome {
        generation,
        executions,
        trace,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use wf_types::llm::LlmFormat;
    use wf_types::message::MessageRole;

    fn mock_profile(id: &str) -> wf_types::llm::LlmProfile {
        wf_types::llm::LlmProfile {
            id: id.to_string(),
            name: id.to_string(),
            format: LlmFormat::OpenaiChat,
            provider_id: None,
            model: "mock-model".to_string(),
            api_key: Some("sk-test".into()),
            base_url: None,
            parameters: None,
            generation: None,
            timeout: None,
            max_retries: None,
            retry_delay: None,
            headers: None,
            metadata: None,
            tool_call_protocol: None,
            auth_type: None,
            custom_headers: None,
            custom_body: None,
            custom_body_enabled: None,
            query_params: None,
            stream_options: None,
            context_window_size: None,
            max_output_tokens: None,
            proxy: None,
            no_proxy: None,
            circuit_breaker: None,
            prompt_caching: None,
            auth: None,
            compat: None,
            identity: None,
            transport: None,
            env: None,
        }
    }

    fn request(profile: &str) -> LlmRequest {
        LlmRequest {
            profile_id: profile.to_string(),
            messages: vec![Message::user_text("look this up".to_string())],
            parameters: None,
            generation: None,
            tools: None,
            tool_call_protocol: None,
            locked_tool_call_protocol: None,
            violation_policy: None,
            execution_id: Some("exec-readonly".into()),
            stream: None,
            dead_loop_detection: None,
            protocol_auto_converted: None,
            timeout_ms: None,
            prompt_caching: None,
            cached_content: None,
            max_retries: None,
            retry_delay_ms: None,
            extra_headers: None,
        }
    }

    fn echo_tool() -> wf_types::tool::Tool {
        wf_types::tool::Tool {
            id: wf_types::Id::from("echo_tool"),
            name: "echo_tool".to_string(),
            description: "Echoes its input".to_string(),
            tool_type: wf_types::tool::ToolType::Stateless,
            parameters: None,
            metadata: None,
            config: None,
            enabled: Some(true),
            strict: Some(true),
            default_timeout_ms: None,
        }
    }

    fn setup() -> (LlmGateway, Arc<ToolRegistry>) {
        let gateway = LlmGateway::new();
        let registry = Arc::new(ToolRegistry::new());
        registry.register_tool(echo_tool());
        let handler: wf_tools::executor::stateless::StatelessAsyncHandler =
            Arc::new(move |args, _ctx| {
                Box::pin(async move {
                    Ok(serde_json::json!({"echo": args}))
                        as Result<serde_json::Value, wf_tools::error::ToolError>
                })
            });
        registry.register_stateless_async_handler("echo_tool", handler);
        (gateway, registry)
    }

    fn tool_call(id: &str, name: &str, arguments: &str) -> wf_types::message::LlmToolCall {
        wf_types::message::LlmToolCall {
            id: id.to_string(),
            r#type: "function".to_string(),
            function: wf_types::message::LlmFunctionCall {
                name: name.to_string(),
                arguments: arguments.to_string(),
            },
            call_id: None,
            thought_signature: None,
        }
    }

    #[tokio::test]
    async fn plain_text_answer_is_valid_with_empty_trace() {
        let (gateway, registry) = setup();
        let mock = Arc::new(wf_llm::MockLlmClient::new());
        mock.script(wf_llm::LlmResponseSpec::text("here is the answer"));
        gateway.register_mock("mock-text", mock);
        gateway.register_profile(mock_profile("mock-text")).unwrap();

        let outcome = generate_readonly_turn(
            &gateway,
            &registry,
            &request("mock-text"),
            &["echo_tool".to_string()],
            None,
            "exec-1",
            None,
        )
        .await
        .unwrap();
        assert!(outcome.trace.is_empty());
        assert!(outcome.executions.is_empty());
        assert_eq!(outcome.generation.content.as_deref(), Some("here is the answer"));
    }

    #[tokio::test]
    async fn allowed_tool_runs_and_produces_trace() {
        let (gateway, registry) = setup();
        let mock = Arc::new(wf_llm::MockLlmClient::new());
        mock.script(wf_llm::LlmResponseSpec::tool_calls(vec![tool_call(
            "call-1",
            "echo_tool",
            r#"{"key":"value"}"#,
        )]));
        gateway.register_mock("mock-tool", mock);
        gateway.register_profile(mock_profile("mock-tool")).unwrap();

        let outcome = generate_readonly_turn(
            &gateway,
            &registry,
            &request("mock-tool"),
            &["echo_tool".to_string()],
            None,
            "exec-2",
            None,
        )
        .await
        .unwrap();
        assert_eq!(outcome.executions.len(), 1);
        assert_eq!(outcome.executions[0].tool_name, "echo_tool");
        // assistant tool_calls + one tool result.
        assert_eq!(outcome.trace.len(), 2);
        assert_eq!(outcome.trace[1].role, MessageRole::Tool);
    }

    #[tokio::test]
    async fn off_allow_list_tool_is_rejected_before_execution() {
        let (gateway, registry) = setup();
        let mock = Arc::new(wf_llm::MockLlmClient::new());
        mock.script(wf_llm::LlmResponseSpec::tool_calls(vec![tool_call(
            "call-1",
            "delete_everything",
            "{}",
        )]));
        gateway.register_mock("mock-offlist", mock);
        gateway
            .register_profile(mock_profile("mock-offlist"))
            .unwrap();

        let err = generate_readonly_turn(
            &gateway,
            &registry,
            &request("mock-offlist"),
            &["echo_tool".to_string()],
            None,
            "exec-3",
            None,
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("outside the allow-list"));
    }
}
