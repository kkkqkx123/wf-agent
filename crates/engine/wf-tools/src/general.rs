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
        _context: &ToolExecutionContext,
        resources: &BuiltinHandlerResources,
    ) -> ToolResult<Value> {
        let params: GeneralParams = serde_json::from_value(parameters.clone())
            .map_err(|e| ToolError::ValidationFailed(format!("Invalid general parameters: {e}")))?;

        if params.request.trim().is_empty() {
            return Err(build_format_error());
        }

        let invoker = resources.general_invoker.as_ref().ok_or_else(|| {
            ToolError::ExecutionError(
                "General tool invoker is not available in this execution".to_string(),
            )
        })?;

        invoker.invoke_request(&params.request).await
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
}
