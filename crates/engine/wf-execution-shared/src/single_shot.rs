//! Single-shot LLM call with tools.
//!
//! The middle primitive between a pure text generation (no tools) and the
//! full agent loop: exactly one model generation followed by at most one
//! bounded round of tool executions. There is no second model round, no
//! conversation persistence and no checkpoint attribution — the reviewer
//! runs read-only. Only tools on the caller-provided allow-list ever
//! execute; anything else the model emits becomes an error.

use wf_llm::LlmGateway;
use wf_tools::registry::ToolRegistry;
use wf_types::llm::{LlmRequest, LlmResult};
use wf_types::tool::{ToolExecutionOptions, ToolExecutionResult};

use crate::error::{ExecutionSharedError, ExecutionSharedResult};

/// Upper bound on tool calls executed in the single round. A verdict-style
/// reviewer emits exactly one; the bound only guards against a model that
/// sprays calls.
pub const MAX_SINGLE_SHOT_TOOL_CALLS: usize = 4;

/// One executed tool call of the single round.
#[derive(Debug, Clone)]
pub struct SingleShotToolExecution {
    pub tool_call_id: String,
    pub tool_name: String,
    pub parameters: serde_json::Value,
    pub result: ToolExecutionResult,
}

/// Outcome of a single-shot call: the model generation plus the executed
/// tool calls of the single round, in order.
#[derive(Debug, Clone)]
pub struct SingleShotOutcome {
    pub generation: LlmResult,
    pub executions: Vec<SingleShotToolExecution>,
}

/// Run one model generation and execute the returned tool calls once.
///
/// Fails when the request declares no tools, when the model returns no
/// tool calls, when more than [`MAX_SINGLE_SHOT_TOOL_CALLS`] calls are
/// returned, when a call targets a tool outside `allowed_tools`, when
/// call arguments are not valid JSON, or when a tool execution fails.
/// Tool executions run without a checkpoint session, so they never record
/// file or shell effects as agent edits.
pub async fn generate_with_tools_once(
    gateway: &LlmGateway,
    registry: &ToolRegistry,
    request: &LlmRequest,
    allowed_tools: &[String],
    tool_options: Option<ToolExecutionOptions>,
    execution_id: &str,
    cancel: Option<tokio_util::sync::CancellationToken>,
) -> ExecutionSharedResult<SingleShotOutcome> {
    if request.messages.is_empty() {
        return Err(ExecutionSharedError::Internal(
            "single-shot call requires at least one message".to_string(),
        ));
    }
    let declared = request.tools.as_ref().map(Vec::len).unwrap_or(0);
    if declared == 0 {
        return Err(ExecutionSharedError::Internal(
            "single-shot call with tools requires at least one declared tool".to_string(),
        ));
    }
    if allowed_tools.is_empty() {
        return Err(ExecutionSharedError::Internal(
            "single-shot call requires a non-empty tool allow-list".to_string(),
        ));
    }

    let generation = gateway.generate(request, cancel).await.map_err(|e| {
        ExecutionSharedError::Internal(format!("single-shot generation failed: {e}"))
    })?;

    let mut calls = generation.tool_calls.clone().unwrap_or_default();
    if calls.is_empty() {
        calls = generation.message.tool_calls.clone().unwrap_or_default();
    }
    if calls.is_empty() {
        return Err(ExecutionSharedError::Internal(
            "single-shot call returned no tool calls".to_string(),
        ));
    }
    if calls.len() > MAX_SINGLE_SHOT_TOOL_CALLS {
        return Err(ExecutionSharedError::Internal(format!(
            "single-shot call returned {} tool calls, exceeding the bound of {MAX_SINGLE_SHOT_TOOL_CALLS}",
            calls.len(),
        )));
    }

    let base_options = tool_options.unwrap_or(ToolExecutionOptions {
        timeout: None,
        retries: None,
        retry_delay: None,
        exponential_backoff: None,
    });
    let context =
        wf_tools::executor::trait_def::ToolExecutionContext::new(execution_id.to_string());

    let mut executions = Vec::with_capacity(calls.len());
    for call in &calls {
        let name = call.function.name.clone();
        if !allowed_tools.iter().any(|t| t == &name) {
            return Err(ExecutionSharedError::Internal(format!(
                "single-shot call emitted tool '{name}' outside the allow-list"
            )));
        }
        let trimmed = call.function.arguments.trim();
        let parameters: serde_json::Value = if trimmed.is_empty() {
            serde_json::Value::Null
        } else {
            serde_json::from_str(trimmed).map_err(|e| {
                ExecutionSharedError::Internal(format!(
                    "single-shot call arguments for tool '{name}' are not valid JSON: {e}"
                ))
            })?
        };
        // Honor the tool definition's own default budget unless the caller
        // pinned an explicit timeout, matching the agent and workflow loops.
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
                    "single-shot tool '{name}' execution failed: {e}"
                ))
            })?;
        executions.push(SingleShotToolExecution {
            tool_call_id: call.id.clone(),
            tool_name: name,
            parameters,
            result,
        });
    }

    Ok(SingleShotOutcome {
        generation,
        executions,
    })
}

/// Run one model generation without tools.
///
/// Fails when the request carries no messages or declares any tools: a
/// text-only call must not declare tools, so a tool-carrying request is a
/// caller misuse rather than an executable branch.
pub async fn generate_text_once(
    gateway: &LlmGateway,
    request: &LlmRequest,
    cancel: Option<tokio_util::sync::CancellationToken>,
) -> ExecutionSharedResult<LlmResult> {
    if request.messages.is_empty() {
        return Err(ExecutionSharedError::Internal(
            "single-shot text call requires at least one message".to_string(),
        ));
    }
    if request.tools.as_ref().map(Vec::len).unwrap_or(0) > 0 {
        return Err(ExecutionSharedError::Internal(
            "single-shot text call must not declare tools".to_string(),
        ));
    }
    gateway.generate(request, cancel).await.map_err(|e| {
        ExecutionSharedError::Internal(format!("single-shot text generation failed: {e}"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use wf_types::message::{Message, MessageContentValue, MessageRole};

    fn user_message(text: &str) -> Message {
        Message {
            id: wf_types::Id::new(),
            role: MessageRole::User,
            content: MessageContentValue::Text(text.to_string()),
            timestamp: wf_common::now(),
            tool_call_id: None,
            tool_name: None,
            tool_calls: None,
            thinking: None,
            metadata: None,
        }
    }

    fn mock_profile(id: &str) -> wf_types::llm::LlmProfile {
        wf_types::llm::LlmProfile {
            id: id.to_string(),
            name: id.to_string(),
            format: wf_types::llm::LlmFormat::OpenaiChat,
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

    fn request(profile: &str, with_tools: bool) -> LlmRequest {
        LlmRequest {
            profile_id: profile.to_string(),
            messages: vec![user_message("decide")],
            parameters: None,
            generation: None,
            tools: with_tools.then(|| vec![echo_tool()]),
            tool_call_protocol: None,
            locked_tool_call_protocol: None,
            violation_policy: None,
            execution_id: Some("exec-single-shot".into()),
            stream: None,
            dead_loop_detection: None,
            protocol_auto_converted: None,
            timeout_ms: None,
        }
    }

    fn tool_call(id: &str, name: &str, arguments: &str) -> wf_types::message::LlmToolCall {
        wf_types::message::LlmToolCall {
            id: id.to_string(),
            r#type: "function".to_string(),
            function: wf_types::message::LlmFunctionCall {
                name: name.to_string(),
                arguments: arguments.to_string(),
            },
        }
    }

    #[tokio::test]
    async fn happy_path_executes_allowed_tool() {
        let (gateway, registry) = setup();
        let mock = Arc::new(wf_llm::MockLlmClient::new());
        mock.script(wf_llm::LlmResponseSpec::tool_calls(vec![tool_call(
            "call-1",
            "echo_tool",
            r#"{"key":"value"}"#,
        )]));
        gateway.register_mock("mock-ok", mock);
        gateway.register_profile(mock_profile("mock-ok")).unwrap();

        let outcome = generate_with_tools_once(
            &gateway,
            &registry,
            &request("mock-ok", true),
            &["echo_tool".to_string()],
            None,
            "exec-1",
            None,
        )
        .await
        .unwrap();
        assert_eq!(outcome.executions.len(), 1);
        assert_eq!(outcome.executions[0].tool_name, "echo_tool");
        assert!(outcome.executions[0].result.success);
    }

    #[tokio::test]
    async fn missing_tool_declaration_is_rejected() {
        let (gateway, registry) = setup();
        let err = generate_with_tools_once(
            &gateway,
            &registry,
            &request("mock-ok", false),
            &["echo_tool".to_string()],
            None,
            "exec-1",
            None,
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("at least one declared tool"));
    }

    #[tokio::test]
    async fn model_without_tool_calls_is_rejected() {
        let (gateway, registry) = setup();
        let mock = Arc::new(wf_llm::MockLlmClient::new());
        mock.script(wf_llm::LlmResponseSpec::text("no tools here"));
        gateway.register_mock("mock-text", mock);
        gateway.register_profile(mock_profile("mock-text")).unwrap();

        let err = generate_with_tools_once(
            &gateway,
            &registry,
            &request("mock-text", true),
            &["echo_tool".to_string()],
            None,
            "exec-1",
            None,
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("no tool calls"));
    }

    #[tokio::test]
    async fn off_allow_list_tool_is_rejected_without_execution() {
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

        let err = generate_with_tools_once(
            &gateway,
            &registry,
            &request("mock-offlist", true),
            &["echo_tool".to_string()],
            None,
            "exec-1",
            None,
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("outside the allow-list"));
    }

    #[tokio::test]
    async fn malformed_arguments_are_rejected() {
        let (gateway, registry) = setup();
        let mock = Arc::new(wf_llm::MockLlmClient::new());
        mock.script(wf_llm::LlmResponseSpec::tool_calls(vec![tool_call(
            "call-1",
            "echo_tool",
            "not json {{{",
        )]));
        gateway.register_mock("mock-badargs", mock);
        gateway
            .register_profile(mock_profile("mock-badargs"))
            .unwrap();

        let err = generate_with_tools_once(
            &gateway,
            &registry,
            &request("mock-badargs", true),
            &["echo_tool".to_string()],
            None,
            "exec-1",
            None,
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("not valid JSON"));
    }

    #[tokio::test]
    async fn calls_beyond_the_bound_are_rejected() {
        let (gateway, registry) = setup();
        let mock = Arc::new(wf_llm::MockLlmClient::new());
        mock.script(wf_llm::LlmResponseSpec::tool_calls(
            (0..MAX_SINGLE_SHOT_TOOL_CALLS + 1)
                .map(|i| tool_call(&format!("call-{i}"), "echo_tool", "{}"))
                .collect(),
        ));
        gateway.register_mock("mock-many", mock);
        gateway.register_profile(mock_profile("mock-many")).unwrap();

        let err = generate_with_tools_once(
            &gateway,
            &registry,
            &request("mock-many", true),
            &["echo_tool".to_string()],
            None,
            "exec-1",
            None,
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("exceeding the bound"));
    }

    #[tokio::test]
    async fn text_call_returns_generation() {
        let (gateway, _) = setup();
        let mock = Arc::new(wf_llm::MockLlmClient::new());
        mock.script(wf_llm::LlmResponseSpec::text("plain reply"));
        gateway.register_mock("mock-text-ok", mock);
        gateway
            .register_profile(mock_profile("mock-text-ok"))
            .unwrap();

        let result = generate_text_once(&gateway, &request("mock-text-ok", false), None)
            .await
            .unwrap();
        assert_eq!(result.content.as_deref(), Some("plain reply"));
    }

    #[tokio::test]
    async fn text_call_rejects_empty_messages() {
        let (gateway, _) = setup();
        let mut req = request("mock-text-ok", false);
        req.messages.clear();
        let err = generate_text_once(&gateway, &req, None).await.unwrap_err();
        assert!(err.to_string().contains("at least one message"));
    }

    #[tokio::test]
    async fn text_call_rejects_declared_tools() {
        let (gateway, _) = setup();
        let err = generate_text_once(&gateway, &request("mock-text-ok", true), None)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("must not declare tools"));
    }
}
