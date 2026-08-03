use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;
use std::time::Instant;

use crate::callback::{AgentLoopConfig, AgentLoopInput, ExecutionCallback, WorkflowInput};
use crate::error::{ToolError, ToolResult};
use crate::executor::base::BaseExecutor;
use crate::executor::trait_def::{ToolExecutionContext, ToolExecutor};
use crate::skill::SkillLoader;
use wf_types::tool::ToolExecutionOptions;
use wf_types::tool::ToolExecutionResult;

pub struct BuiltinExecutor {
    callback: Option<Arc<dyn ExecutionCallback>>,
    skill_loader: Option<Arc<SkillLoader>>,
}

impl BuiltinExecutor {
    pub fn new() -> Self {
        Self {
            callback: None,
            skill_loader: None,
        }
    }

    pub fn with_callback(callback: Arc<dyn ExecutionCallback>) -> Self {
        Self {
            callback: Some(callback),
            skill_loader: None,
        }
    }

    pub fn with_callback_opt(callback: Option<Arc<dyn ExecutionCallback>>) -> Self {
        Self {
            callback,
            skill_loader: None,
        }
    }

    pub fn with_skill_loader(mut self, loader: Arc<SkillLoader>) -> Self {
        self.skill_loader = Some(loader);
        self
    }

    pub fn set_skill_loader(mut self, loader: Option<Arc<SkillLoader>>) -> Self {
        self.skill_loader = loader;
        self
    }

    fn get_callback(&self, tool_name: &str) -> ToolResult<Arc<dyn ExecutionCallback>> {
        if let Some(ref cb) = self.callback {
            return Ok(cb.clone());
        }
        crate::callback::get_execution_callback()
            .ok_or_else(|| ToolError::CallbackNotRegistered(tool_name.to_string()))
    }

    async fn dispatch(
        &self,
        tool_name: &str,
        parameters: &Value,
        context: &ToolExecutionContext,
    ) -> ToolResult<Value> {
        match tool_name {
            "call_agent" => self.handle_call_agent(parameters, context).await,
            "execute_workflow" => self.handle_execute_workflow(parameters, context).await,
            "query_execution_status" => self.handle_query_status(parameters, context).await,
            "cancel_execution" => self.handle_cancel_execution(parameters, context).await,
            "skill" => self.handle_skill(parameters),
            _ => Err(ToolError::NotFound(format!(
                "Unknown builtin tool: {}",
                tool_name
            ))),
        }
    }

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

    fn handle_skill(&self, parameters: &Value) -> ToolResult<Value> {
        let skill_name = parameters
            .get("skill")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                ToolError::ValidationFailed(
                    "Missing or invalid 'skill' parameter. Please provide a valid skill name."
                        .to_string(),
                )
            })?;

        let loader = self.skill_loader.as_ref().ok_or_else(|| {
            ToolError::ExecutionError(
                "Skill system is not available. Please configure skill paths before using skills."
                    .to_string(),
            )
        })?;

        if !loader.has_skill(skill_name) {
            let available = Self::format_available_skills(&loader.list_skills());
            return Err(ToolError::NotFound(format!(
                "Skill '{}' not found.\n\nAvailable skills:\n{}\n\n\
                 Use the 'skill' tool with one of the available skill names listed above. \
                 Each skill provides specialized instructions for specific tasks.",
                skill_name, available
            )));
        }

        let content = loader.load_content(skill_name)?;
        Ok(Value::String(content))
    }

    async fn handle_call_agent(
        &self,
        parameters: &Value,
        context: &ToolExecutionContext,
    ) -> ToolResult<Value> {
        let callback = self.get_callback("call_agent")?;

        let agent_id = parameters
            .get("agent_id")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();

        let message = parameters
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();

        let profile_id = parameters
            .get("profile_id")
            .or_else(|| parameters.get("model"))
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                ToolError::ExecutionError("call_agent requires a profile_id parameter".to_string())
            })?
            .to_string();

        let config = AgentLoopConfig {
            agent_id,
            model: profile_id,
            max_iterations: parameters
                .get("max_iterations")
                .and_then(|v| v.as_u64())
                .map(|v| v as u32),
            max_execution_time: parameters
                .get("max_execution_time")
                .and_then(|v| v.as_u64()),
            hooks: parameters
                .get("hooks")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default(),
            available_tool_names: parameters
                .get("available_tool_names")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default(),
            tool_call_format: parameters
                .get("tool_call_format")
                .and_then(|v| v.as_str())
                .and_then(wf_types::llm::ToolCallFormatConfig::from_format_str),
            token_limit: parameters.get("token_limit").and_then(|v| v.as_u64()),
            token_warning_threshold: parameters
                .get("token_warning_threshold")
                .and_then(|v| v.as_u64())
                .map(|v| v as u32),
            enable_token_tracking: parameters
                .get("enable_token_tracking")
                .and_then(|v| v.as_bool()),
        };

        let input = AgentLoopInput {
            message,
            context: {
                let mut m = std::collections::HashMap::new();
                m.insert(
                    "parent_execution_id".into(),
                    Value::String(context.execution_id.clone()),
                );
                m
            },
            conversation: parameters
                .get("conversation")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default(),
        };

        let output = callback.execute_agent_loop(config, input).await?;

        Ok(serde_json::json!({
            "result": output.result,
            "iterations": output.iterations,
        }))
    }

    async fn handle_execute_workflow(
        &self,
        parameters: &Value,
        _context: &ToolExecutionContext,
    ) -> ToolResult<Value> {
        let callback = self.get_callback("execute_workflow")?;

        let workflow_id = parameters
            .get("workflow_id")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();

        let variables = parameters
            .get("variables")
            .and_then(|v| v.as_object())
            .map(|o| o.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
            .unwrap_or_default();

        let input = WorkflowInput { variables };

        let output = callback.execute_workflow(&workflow_id, input).await?;

        Ok(serde_json::json!({
            "execution_id": output.execution_id,
            "result": output.result,
        }))
    }

    async fn handle_query_status(
        &self,
        parameters: &Value,
        _context: &ToolExecutionContext,
    ) -> ToolResult<Value> {
        let callback = self.get_callback("query_execution_status")?;

        let execution_id = parameters
            .get("execution_id")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();

        let status = callback.query_execution_status(&execution_id).await?;

        Ok(serde_json::json!({
            "execution_id": status.execution_id,
            "status": status.status,
            "progress": status.progress,
        }))
    }

    async fn handle_cancel_execution(
        &self,
        parameters: &Value,
        _context: &ToolExecutionContext,
    ) -> ToolResult<Value> {
        let callback = self.get_callback("cancel_execution")?;

        let execution_id = parameters
            .get("execution_id")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();

        callback.cancel_execution(&execution_id).await?;

        Ok(serde_json::json!({ "cancelled": true }))
    }
}

impl Default for BuiltinExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ToolExecutor for BuiltinExecutor {
    async fn execute(
        &self,
        tool: &wf_types::tool::Tool,
        parameters: &Value,
        _options: &ToolExecutionOptions,
        context: &ToolExecutionContext,
    ) -> ToolResult<ToolExecutionResult> {
        let start = Instant::now();
        BaseExecutor::validate_parameters(tool, parameters)?;

        let result = self.dispatch(&tool.name, parameters, context).await;
        let execution_time = start.elapsed().as_millis() as i64;

        match result {
            Ok(value) => Ok(BaseExecutor::build_result(
                true,
                Some(value),
                None,
                execution_time,
                0,
            )),
            Err(e) => Ok(BaseExecutor::build_result(
                false,
                None,
                Some(e.to_string()),
                execution_time,
                0,
            )),
        }
    }

    fn executor_type(&self) -> &str {
        "builtin"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_types::skill::SkillConfig;
    use wf_types::tool::ToolExecutionOptions;

    fn make_skill_dir(root: &std::path::Path) -> std::path::PathBuf {
        let dir = root.join("my-skill");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("SKILL.md"),
            "---\nname: my-skill\ndescription: Test skill\n---\n\n# Skill body",
        )
        .unwrap();
        dir
    }

    fn make_tool() -> wf_types::tool::Tool {
        wf_types::tool::Tool {
            id: "skill".into(),
            name: "skill".into(),
            description: "Load a skill".into(),
            tool_type: wf_types::tool::ToolType::BuiltIn,
            parameters: None,
            metadata: None,
            config: None,
            enabled: Some(true),
            strict: None,
            default_timeout_ms: None,
        }
    }

    #[tokio::test]
    async fn test_skill_handler_returns_content() {
        let root = std::env::temp_dir().join(format!("wf-builtin-skill-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        make_skill_dir(&root);

        let loader = Arc::new(SkillLoader::new(SkillConfig {
            paths: vec![root.to_string_lossy().to_string()],
            auto_scan: Some(true),
        }));
        let executor = BuiltinExecutor::new().with_skill_loader(loader);

        let ctx = ToolExecutionContext::new("exec-1".into());
        let options = ToolExecutionOptions {
            timeout: None,
            retries: None,
            retry_delay: None,
            exponential_backoff: None,
        };

        let result = executor
            .execute(
                &make_tool(),
                &serde_json::json!({ "skill": "my-skill" }),
                &options,
                &ctx,
            )
            .await
            .unwrap();
        assert!(result.success);
        assert!(result
            .result
            .and_then(|v| v.as_str().map(String::from))
            .unwrap_or_default()
            .contains("Skill body"));

        let missing = executor
            .execute(
                &make_tool(),
                &serde_json::json!({ "skill": "nope" }),
                &options,
                &ctx,
            )
            .await
            .unwrap();
        assert!(!missing.success);
        assert!(missing
            .error
            .unwrap_or_default()
            .contains("Skill 'nope' not found"));

        let _ = std::fs::remove_dir_all(&root);
    }
}
