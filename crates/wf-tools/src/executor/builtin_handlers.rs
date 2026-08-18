//! Default builtin tool handlers.
//!
//! Each builtin tool (call_agent / execute_workflow / query_workflow_status /
//! cancel_workflow / skill) is a [`BuiltinToolHandler`] implementation with a
//! typed parameter struct. Parameter parsing uses serde with
//! `deny_unknown_fields`, so the schema and the implementation are kept in
//! sync by the type system.

use async_trait::async_trait;
use dashmap::DashMap;
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

use crate::callback::{AgentLoopConfig, AgentLoopInput, HookConfig, WorkflowInput};
use crate::error::{ToolError, ToolResult};
use crate::executor::builtin_handler::{
    resolve_callback, BuiltinHandlerResources, BuiltinToolHandler,
};
use crate::executor::trait_def::ToolExecutionContext;
use crate::general::GeneralHandler;
use crate::skill::SkillLoadContext;
use wf_types::llm::ToolCallFormatConfig;
use wf_types::message::Message;
fn default_wait() -> bool {
    true
}

/// Parameters of the `call_agent` tool.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CallAgentParams {
    #[serde(default)]
    pub agent_id: String,
    pub agent_profile_id: String,
    #[serde(default)]
    pub prompt: String,
    #[serde(default = "default_wait")]
    pub wait: bool,
    #[serde(default)]
    pub max_iterations: Option<u32>,
    #[serde(default)]
    pub max_execution_time: Option<u64>,
    #[serde(default)]
    pub hooks: Vec<HookConfig>,
    #[serde(default)]
    pub available_tool_names: Vec<String>,
    /// Tools visible in the initial schema; when absent all available tools
    /// are initially visible (mirrors `resolveInitialTools`).
    #[serde(default)]
    pub initial_tool_names: Vec<String>,
    /// Discoverable tools: metadata-only injection, invoked via `general`.
    #[serde(default)]
    pub discoverable_tool_names: Vec<String>,
    /// Escape hatch controlling `general` tool exposure (default: auto).
    #[serde(default)]
    pub enable_general_tool: Option<bool>,
    #[serde(default)]
    pub tool_call_format: Option<String>,
    #[serde(default)]
    pub token_limit: Option<u64>,
    #[serde(default)]
    pub token_warning_threshold: Option<u32>,
    #[serde(default)]
    pub enable_token_tracking: Option<bool>,
    #[serde(default)]
    pub conversation: Vec<Message>,
}

/// Parameters of the `execute_workflow` tool.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecuteWorkflowParams {
    pub workflow_id: String,
    #[serde(default)]
    pub input: HashMap<String, Value>,
    #[serde(default = "default_wait")]
    pub wait: bool,
}

/// Parameters shared by `query_workflow_status` and `cancel_workflow`.
/// `execution_id` takes precedence; `workflow_id` is the schema-required
/// primary key.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionIdParams {
    pub workflow_id: Option<String>,
    #[serde(default)]
    pub execution_id: Option<String>,
}

/// Parameters of the `skill` tool.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkillParams {
    pub skill: String,
    #[serde(default)]
    pub args: Option<HashMap<String, Value>>,
}

/// Handler for the `call_agent` builtin tool.
pub struct CallAgentHandler;

#[async_trait]
impl BuiltinToolHandler for CallAgentHandler {
    fn tool_name(&self) -> &'static str {
        "call_agent"
    }

    async fn handle(
        &self,
        parameters: &Value,
        context: &ToolExecutionContext,
        resources: &BuiltinHandlerResources,
    ) -> ToolResult<Value> {
        let callback = resolve_callback(resources, self.tool_name())?;
        let params: CallAgentParams = serde_json::from_value(parameters.clone()).map_err(|e| {
            ToolError::ValidationFailed(format!("Invalid call_agent parameters: {e}"))
        })?;

        if params.agent_profile_id.is_empty() {
            return Err(ToolError::ExecutionError(
                "call_agent requires a agent_profile_id parameter".into(),
            ));
        }

        let config = AgentLoopConfig {
            agent_id: params.agent_id,
            model: params.agent_profile_id,
            max_iterations: params.max_iterations,
            max_execution_time: params.max_execution_time,
            hooks: params.hooks,
            available_tool_names: params.available_tool_names,
            tool_call_format: params
                .tool_call_format
                .as_deref()
                .and_then(ToolCallFormatConfig::from_format_str),
            token_limit: params.token_limit,
            token_warning_threshold: params.token_warning_threshold,
            enable_token_tracking: params.enable_token_tracking,
            general_description: None,
            discoverable_metadata_block: None,
            initial_tool_names: params.initial_tool_names,
            discoverable_tool_names: params.discoverable_tool_names,
            enable_general_tool: params.enable_general_tool,
            activated_tool_names: Vec::new(),
            hidden_tool_names: Vec::new(),
        };

        let input = AgentLoopInput {
            message: params.prompt,
            context: {
                let mut m = HashMap::new();
                m.insert(
                    "parent_execution_id".into(),
                    Value::String(context.execution_id.clone()),
                );
                m
            },
            conversation: params.conversation,
        };

        // Async dispatch: hand the execution to the engine's spawn path and
        // return a handle immediately; the result is retrieved through
        // query_workflow_status (polling).
        if !params.wait {
            let spawned = callback.spawn_agent_loop(config, input).await?;
            return Ok(serde_json::json!({
                "agent_id": spawned.agent_loop_id,
                "execution_id": spawned.execution_id,
                "status": spawned.status,
            }));
        }

        let output = callback.execute_agent_loop(config, input).await?;

        Ok(serde_json::json!({
            "result": output.result,
            "iterations": output.iterations,
        }))
    }
}

/// Handler for the `execute_workflow` builtin tool.
pub struct ExecuteWorkflowHandler;

#[async_trait]
impl BuiltinToolHandler for ExecuteWorkflowHandler {
    fn tool_name(&self) -> &'static str {
        "execute_workflow"
    }

    async fn handle(
        &self,
        parameters: &Value,
        _context: &ToolExecutionContext,
        resources: &BuiltinHandlerResources,
    ) -> ToolResult<Value> {
        let callback = resolve_callback(resources, self.tool_name())?;
        let params: ExecuteWorkflowParams =
            serde_json::from_value(parameters.clone()).map_err(|e| {
                ToolError::ValidationFailed(format!("Invalid execute_workflow parameters: {e}"))
            })?;

        let input = WorkflowInput {
            variables: params.input,
        };

        // Async dispatch: hand the execution to the engine's spawn path and
        // return a handle immediately; the result is retrieved through
        // query_workflow_status (polling).
        if !params.wait {
            let spawned = callback.spawn_workflow(&params.workflow_id, input).await?;
            return Ok(serde_json::json!({
                "execution_id": spawned.execution_id,
                "status": spawned.status,
            }));
        }

        let output = callback
            .execute_workflow(&params.workflow_id, input)
            .await?;

        Ok(serde_json::json!({
            "execution_id": output.execution_id,
            "result": output.result,
        }))
    }
}

/// Handler for the `query_workflow_status` builtin tool.
pub struct QueryWorkflowStatusHandler;

#[async_trait]
impl BuiltinToolHandler for QueryWorkflowStatusHandler {
    fn tool_name(&self) -> &'static str {
        "query_workflow_status"
    }

    async fn handle(
        &self,
        parameters: &Value,
        _context: &ToolExecutionContext,
        resources: &BuiltinHandlerResources,
    ) -> ToolResult<Value> {
        let callback = resolve_callback(resources, self.tool_name())?;
        let params: ExecutionIdParams =
            serde_json::from_value(parameters.clone()).map_err(|e| {
                ToolError::ValidationFailed(format!(
                    "Invalid query_workflow_status parameters: {e}"
                ))
            })?;

        let execution_id = params
            .execution_id
            .or(params.workflow_id)
            .unwrap_or_default();
        let status = callback.query_execution_status(&execution_id).await?;

        Ok(serde_json::json!({
            "execution_id": status.execution_id,
            "status": status.status,
            "progress": status.progress,
            "result": status.result,
        }))
    }
}

/// Handler for the `cancel_workflow` builtin tool.
pub struct CancelWorkflowHandler;

#[async_trait]
impl BuiltinToolHandler for CancelWorkflowHandler {
    fn tool_name(&self) -> &'static str {
        "cancel_workflow"
    }

    async fn handle(
        &self,
        parameters: &Value,
        _context: &ToolExecutionContext,
        resources: &BuiltinHandlerResources,
    ) -> ToolResult<Value> {
        let callback = resolve_callback(resources, self.tool_name())?;
        let params: ExecutionIdParams =
            serde_json::from_value(parameters.clone()).map_err(|e| {
                ToolError::ValidationFailed(format!("Invalid cancel_workflow parameters: {e}"))
            })?;

        let execution_id = params
            .execution_id
            .or(params.workflow_id)
            .unwrap_or_default();
        callback.cancel_execution(&execution_id).await?;

        Ok(serde_json::json!({ "cancelled": true }))
    }
}

/// Handler for the `skill` builtin tool.
pub struct SkillHandler;

impl SkillHandler {
    fn format_available_skills(skills: &[wf_types::skill::SkillMetadata]) -> String {
        if skills.is_empty() {
            return "(no skills available)".into();
        }
        skills
            .iter()
            .map(|s| format!("  - {}: {}", s.name, s.description))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[async_trait]
impl BuiltinToolHandler for SkillHandler {
    fn tool_name(&self) -> &'static str {
        "skill"
    }

    async fn handle(
        &self,
        parameters: &Value,
        _context: &ToolExecutionContext,
        resources: &BuiltinHandlerResources,
    ) -> ToolResult<Value> {
        let params: SkillParams = serde_json::from_value(parameters.clone())
            .map_err(|e| ToolError::ValidationFailed(format!("Invalid skill parameters: {e}")))?;

        if params.skill.is_empty() {
            return Err(ToolError::ValidationFailed(
                "Missing or invalid 'skill' parameter. Please provide a valid skill name."
                    .to_string(),
            ));
        }

        let loader = resources.skill_loader.as_ref().ok_or_else(|| {
            ToolError::ExecutionError(
                "Skill system is not available. Please configure skill paths before using skills."
                    .to_string(),
            )
        })?;

        if !loader.has_skill(&params.skill) {
            let available = Self::format_available_skills(&loader.list_skills());
            return Err(ToolError::NotFound(format!(
                "Skill '{}' not found.\n\nAvailable skills:\n{}\n\n\
                 Use the 'skill' tool with one of the available skill names listed above. \
                 Each skill provides specialized instructions for specific tasks.",
                params.skill, available
            )));
        }

        let context = SkillLoadContext {
            variables: params.args,
            tools: None,
        };
        let content = loader.load_skill_content(&params.skill, Some(&context))?;
        Ok(Value::String(content))
    }
}

/// Register the six default builtin tool handlers into a handler map.
///
/// Idempotent: re-registration replaces the previous handler for the same
/// tool name. Called by the standalone [`super::builtin::BuiltinExecutor`]
/// constructor and by `crate::handlers::register_builtin_handlers` for
/// registry-shared maps.
pub fn register_default_builtin_handlers(handlers: &DashMap<String, Arc<dyn BuiltinToolHandler>>) {
    let defaults: [Arc<dyn BuiltinToolHandler>; 6] = [
        Arc::new(CallAgentHandler),
        Arc::new(ExecuteWorkflowHandler),
        Arc::new(QueryWorkflowStatusHandler),
        Arc::new(CancelWorkflowHandler),
        Arc::new(SkillHandler),
        Arc::new(GeneralHandler),
    ];
    for handler in defaults {
        handlers.insert(handler.tool_name().to_string(), handler);
    }
}
