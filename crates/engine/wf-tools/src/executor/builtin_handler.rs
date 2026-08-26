//! Builtin tool handler trait.
//!
//! Builtin tools (call_agent / execute_workflow / query_workflow_status /
//! cancel_workflow / skill) are dispatched through a shared handler registry
//! instead of a hardcoded match, so adding a new builtin tool never touches
//! the executor core.

use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;

use crate::callback::ExecutionCallback;
use crate::error::{ToolError, ToolResult};
use crate::executor::trait_def::ToolExecutionContext;
use crate::general::GeneralToolInvoker;
use crate::skill::SkillLoader;

/// Resources the builtin executor injects into handlers at dispatch time.
///
/// Handlers resolve their dependencies dynamically per call so executor
/// instances created before (or after) a callback / skill loader
/// registration keep working.
#[derive(Clone, Default)]
pub struct BuiltinHandlerResources {
    pub callback: Option<Arc<dyn ExecutionCallback>>,
    pub skill_loader: Option<Arc<SkillLoader>>,
    /// Per-execution resolver backing the `general` tool (inner tool
    /// invocations); provided by the agent engine.
    pub general_invoker: Option<Arc<dyn GeneralToolInvoker>>,
}

/// A single builtin tool handler. Implementations are stateless registries
/// of behavior keyed by [`Self::tool_name`].
#[async_trait]
pub trait BuiltinToolHandler: Send + Sync {
    /// The builtin tool name this handler serves.
    fn tool_name(&self) -> &'static str;

    /// Execute the tool with the given parameters. Validation against the
    /// tool schema has already happened in the executor; handlers parse
    /// parameters into typed structs and reject unknown fields.
    async fn handle(
        &self,
        parameters: &Value,
        context: &ToolExecutionContext,
        resources: &BuiltinHandlerResources,
    ) -> ToolResult<Value>;
}

/// Resolve the execution callback for a builtin tool. The handler-level
/// callback wins; otherwise the globally registered callback is used.
pub(crate) fn resolve_callback(
    resources: &BuiltinHandlerResources,
    tool_name: &str,
) -> ToolResult<Arc<dyn ExecutionCallback>> {
    if let Some(ref cb) = resources.callback {
        return Ok(cb.clone());
    }
    crate::callback::get_execution_callback()
        .ok_or_else(|| ToolError::CallbackNotRegistered(tool_name.to_string()))
}
