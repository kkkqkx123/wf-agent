//! The `general` tool: a pure invoke proxy for tools that are not in the
//! initial schema.
//!
//! Discoverable tools have only their metadata injected into the prompt; the
//! model reaches them through `general`. The tool definition is always
//! registered (like `skill`), while exposure is decided by the assembly
//! layer. The handler itself is stateless: parsing the XML request body and
//! executing the inner tool is delegated to the engine-provided
//! [`GeneralToolInvoker`], keeping this crate free of LLM/engine
//! dependencies.

use async_trait::async_trait;
use serde_json::Value;

use crate::error::{ToolError, ToolResult};
use crate::executor::builtin_handler::{BuiltinHandlerResources, BuiltinToolHandler};
use crate::executor::trait_def::ToolExecutionContext;

/// Name of the general tool (constant so the assembly layer and the agent
/// engine can reference it without magic strings).
pub const GENERAL_TOOL_NAME: &str = "general";

/// Metadata key carrying the outer `general` tool call id into the handler.
/// The execution pipeline stamps it per invocation so inner ids can derive
/// stably from the outer id (checkpoint replay keys survive re-execution).
pub const OUTER_TOOL_CALL_ID_METADATA: &str = "tool_call_id";

/// Runtime resolver for inner tool invocations, implemented by the agent
/// engine (wf-agent) and injected per execution. The handler stays free of
/// engine state; the invoker routes through the shared tool execution
/// pipeline so every control (visibility, approval, checkpoint, timeout)
/// applies to inner tools exactly as to direct calls.
#[async_trait]
pub trait GeneralToolInvoker: Send + Sync {
    /// Execute a `general` request body: a JSON object
    /// `{"tool": "...", "parameters": {...}}` (or an array of such objects).
    /// Returns the inner tool's native result; parse failures return a
    /// format-error text the model can self-correct from.
    async fn invoke_request(&self, request: &str) -> ToolResult<Value>;

    /// Same as [`Self::invoke_request`], with the outer proxy call id for
    /// stable inner id derivation (`"{outer}#{index}#{tool}"`). The default
    /// body keeps backward behavior for invokers that do not track it.
    async fn invoke_request_with_outer(
        &self,
        outer_call_id: &str,
        request: &str,
    ) -> ToolResult<Value> {
        let _ = outer_call_id;
        self.invoke_request(request).await
    }
}

/// Parameters of the `general` tool. The schema is deliberately minimal and
/// fixed: a single string parameter with no inner schema constraints.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeneralParams {
    pub request: String,
}

/// Expected-format error text returned on unparseable requests so the model
/// can self-correct.
pub fn build_format_error() -> ToolError {
    ToolError::ValidationFailed(
        "Invalid general request. Expected a JSON object \
         {\"tool\": \"tool_name\", \"parameters\": {...}} (or an array of such objects) inside \
         the <request> parameter of a <tool_use> call, e.g.:\n\
         <tool_use>\n  <tool_name>general</tool_name>\n  <parameters>\n    \
         <request>{\"tool\": \"web_search\", \"parameters\": {\"query\": \"rust 异步\"}}</request>\n  \
         </parameters>\n</tool_use>"
            .to_string(),
    )
}

/// Handler for the `general` builtin tool.
pub struct GeneralHandler;

#[async_trait]
impl BuiltinToolHandler for GeneralHandler {
    fn tool_name(&self) -> &'static str {
        GENERAL_TOOL_NAME
    }

    async fn handle(
        &self,
        parameters: &Value,
        context: &ToolExecutionContext,
        resources: &BuiltinHandlerResources,
    ) -> ToolResult<Value> {
        let params: GeneralParams = serde_json::from_value(parameters.clone())
            .map_err(|e| ToolError::ValidationFailed(format!("Invalid general parameters: {e}")))?;

        if params.request.trim().is_empty() {
            return Err(build_format_error());
        }

        // Missing-invoker is an environment wiring fault, deliberately an
        // `ExecutionError` (never `ValidationFailed`) so callers can tell it
        // apart from a model format error without sniffing text.
        let invoker = resources.general_invoker.as_ref().ok_or_else(|| {
            ToolError::ExecutionError(
                "[general-unavailable] General tool invoker is not available in this execution"
                    .to_string(),
            )
        })?;

        // Thread the outer call id through when the pipeline stamped it, so
        // inner ids derive stably (`general_history::derive_inner_call_id`).
        let outer_id = context
            .metadata
            .get(OUTER_TOOL_CALL_ID_METADATA)
            .and_then(|v| v.as_str())
            .map(str::to_string);
        match outer_id {
            Some(id) => {
                invoker
                    .invoke_request_with_outer(&id, &params.request)
                    .await
            }
            None => invoker.invoke_request(&params.request).await,
        }
    }
}

impl GeneralHandler {
    /// Whether an error is the missing-invoker wiring fault (as opposed to
    /// a model format error). Matches on the stable `[general-unavailable]`
    /// marker, never on free-form text.
    pub fn is_invoker_missing(error: &ToolError) -> bool {
        matches!(error, ToolError::ExecutionError(msg) if msg.starts_with("[general-unavailable]"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_error_describes_expected_json() {
        let err = build_format_error();
        assert!(err.to_string().contains("\"tool\""));
        assert!(err.to_string().contains("<request>"));
    }

    #[tokio::test]
    async fn handler_rejects_empty_request() {
        let handler = GeneralHandler;
        let ctx = ToolExecutionContext::new("exec-1".into());
        let resources = BuiltinHandlerResources::default();
        let result = handler
            .handle(&serde_json::json!({ "request": "" }), &ctx, &resources)
            .await;
        assert!(matches!(result, Err(ToolError::ValidationFailed(_))));
    }

    #[tokio::test]
    async fn handler_requires_invoker() {
        let handler = GeneralHandler;
        let ctx = ToolExecutionContext::new("exec-1".into());
        let resources = BuiltinHandlerResources::default();
        let result = handler
            .handle(
                &serde_json::json!({ "request": "{\"tool\": \"x\", \"parameters\": {\"a\": 1}}" }),
                &ctx,
                &resources,
            )
            .await;
        assert!(matches!(result, Err(ToolError::ExecutionError(_))));
    }

    #[tokio::test]
    async fn handler_forwards_request_to_invoker() {
        struct EchoInvoker;
        #[async_trait]
        impl GeneralToolInvoker for EchoInvoker {
            async fn invoke_request(&self, request: &str) -> ToolResult<Value> {
                Ok(Value::String(format!("echo:{request}")))
            }
        }

        let handler = GeneralHandler;
        let ctx = ToolExecutionContext::new("exec-1".into());
        let resources = BuiltinHandlerResources {
            general_invoker: Some(std::sync::Arc::new(EchoInvoker)),
            ..Default::default()
        };
        let result = handler
            .handle(
                &serde_json::json!({ "request": "inner-body" }),
                &ctx,
                &resources,
            )
            .await
            .expect("invoker must be used");
        assert_eq!(result, serde_json::json!("echo:inner-body"));
    }

    #[test]
    fn invoker_missing_is_distinguishable_from_format_error() {
        let missing = ToolError::ExecutionError("[general-unavailable] nope".to_string());
        assert!(GeneralHandler::is_invoker_missing(&missing));
        assert!(!GeneralHandler::is_invoker_missing(&build_format_error()));
    }

    #[tokio::test]
    async fn handler_threads_outer_call_id_to_invoker() {
        use std::sync::Mutex;
        struct RecordingInvoker {
            seen_outer: Mutex<Vec<String>>,
        }
        #[async_trait]
        impl GeneralToolInvoker for RecordingInvoker {
            async fn invoke_request(&self, request: &str) -> ToolResult<Value> {
                Ok(Value::String(format!("echo:{request}")))
            }

            async fn invoke_request_with_outer(
                &self,
                outer_call_id: &str,
                request: &str,
            ) -> ToolResult<Value> {
                self.seen_outer
                    .lock()
                    .expect("lock")
                    .push(outer_call_id.to_string());
                self.invoke_request(request).await
            }
        }

        let invoker = std::sync::Arc::new(RecordingInvoker {
            seen_outer: Mutex::new(Vec::new()),
        });
        let handler = GeneralHandler;
        let ctx = ToolExecutionContext::new("exec-1".into())
            .with_metadata(OUTER_TOOL_CALL_ID_METADATA, serde_json::json!("outer-42"));
        let resources = BuiltinHandlerResources {
            general_invoker: Some(invoker.clone()),
            ..Default::default()
        };
        handler
            .handle(
                &serde_json::json!({ "request": "{\"tool\": \"x\", \"parameters\": {}}" }),
                &ctx,
                &resources,
            )
            .await
            .expect("invoker must be used");
        assert_eq!(
            invoker.seen_outer.lock().expect("lock").as_slice(),
            ["outer-42"]
        );
    }
}
