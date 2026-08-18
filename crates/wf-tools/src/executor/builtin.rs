use async_trait::async_trait;
use dashmap::DashMap;
use serde_json::Value;
use std::sync::Arc;
use std::time::Instant;

use crate::callback::ExecutionCallback;
use crate::error::{ToolError, ToolResult};
use crate::executor::base::BaseExecutor;
use crate::executor::builtin_handler::{BuiltinHandlerResources, BuiltinToolHandler};
use crate::executor::builtin_handlers::register_default_builtin_handlers;
use crate::executor::trait_def::{ToolExecutionContext, ToolExecutor};
use crate::skill::SkillLoader;
use wf_types::tool::ToolExecutionOptions;
use wf_types::tool::ToolExecutionResult;

/// Executor for `BuiltIn` tools.
///
/// Dispatch is registry-based: every builtin tool (including the six
/// defaults call_agent / execute_workflow / query_workflow_status /
/// cancel_workflow / skill / general) is served by a [`BuiltinToolHandler`]
/// registered in a shared handler map, so adding a new builtin tool never
/// requires touching this executor.
pub struct BuiltinExecutor {
    handlers: Arc<DashMap<String, Arc<dyn BuiltinToolHandler>>>,
    callback: Option<Arc<dyn ExecutionCallback>>,
    skill_loader: Option<Arc<SkillLoader>>,
}

impl BuiltinExecutor {
    /// Create an executor with its own handler map pre-wired with the six
    /// default builtin handlers.
    pub fn new() -> Self {
        let handlers = Arc::new(DashMap::new());
        register_default_builtin_handlers(&handlers);
        Self::new_shared(handlers, None, None)
    }

    /// Create an executor dispatching through the given shared handler map
    /// (typically owned by the `ToolRegistry`). The map must already contain
    /// the desired handlers; the six defaults are registered by
    /// `crate::handlers::register_builtin_handlers`.
    pub fn new_shared(
        handlers: Arc<DashMap<String, Arc<dyn BuiltinToolHandler>>>,
        callback: Option<Arc<dyn ExecutionCallback>>,
        skill_loader: Option<Arc<SkillLoader>>,
    ) -> Self {
        Self {
            handlers,
            callback,
            skill_loader,
        }
    }

    pub fn with_callback(self, callback: Arc<dyn ExecutionCallback>) -> Self {
        Self {
            callback: Some(callback),
            ..self
        }
    }

    pub fn with_callback_opt(self, callback: Option<Arc<dyn ExecutionCallback>>) -> Self {
        Self { callback, ..self }
    }

    pub fn with_skill_loader(mut self, loader: Arc<SkillLoader>) -> Self {
        self.skill_loader = Some(loader);
        self
    }

    pub fn set_skill_loader(mut self, loader: Option<Arc<SkillLoader>>) -> Self {
        self.skill_loader = loader;
        self
    }

    /// Register a builtin tool handler. A later registration for the same
    /// tool name replaces the earlier one.
    pub fn register_builtin_handler(&self, tool_name: &str, handler: Arc<dyn BuiltinToolHandler>) {
        self.handlers.insert(tool_name.to_string(), handler);
    }

    pub fn unregister_builtin_handler(&self, tool_name: &str) {
        self.handlers.remove(tool_name);
    }

    fn resources(&self, context: &ToolExecutionContext) -> BuiltinHandlerResources {
        let callback = self
            .callback
            .clone()
            .or_else(crate::callback::get_execution_callback);
        BuiltinHandlerResources {
            callback,
            skill_loader: self.skill_loader.clone(),
            general_invoker: context.general_invoker.clone(),
        }
    }

    async fn dispatch(
        &self,
        tool_name: &str,
        parameters: &Value,
        context: &ToolExecutionContext,
    ) -> ToolResult<Value> {
        let handler = self
            .handlers
            .get(tool_name)
            .ok_or_else(|| ToolError::NotFound(format!("Unknown builtin tool: {}", tool_name)))?;
        let resources = self.resources(context);
        handler.handle(parameters, context, &resources).await
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
    use crate::executor::register_default_builtin_handlers;
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

    fn make_call_agent_tool() -> wf_types::tool::Tool {
        crate::predefined::agent::CALL_AGENT.tool_def()
    }

    #[derive(Default)]
    struct RecordingCallback {
        sync_calls: std::sync::atomic::AtomicU32,
        spawn_calls: std::sync::atomic::AtomicU32,
        sync_workflow_calls: std::sync::atomic::AtomicU32,
        spawn_workflow_calls: std::sync::atomic::AtomicU32,
    }

    #[async_trait]
    impl ExecutionCallback for RecordingCallback {
        async fn execute_agent_loop(
            &self,
            _config: crate::callback::AgentLoopConfig,
            _input: crate::callback::AgentLoopInput,
        ) -> ToolResult<crate::callback::AgentLoopOutput> {
            self.sync_calls
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(crate::callback::AgentLoopOutput {
                agent_loop_id: wf_types::Id::from("sync-run".to_string()),
                result: serde_json::json!({"synced": true}),
                iterations: 1,
                conversation: Vec::new(),
            })
        }

        async fn spawn_agent_loop(
            &self,
            _config: crate::callback::AgentLoopConfig,
            _input: crate::callback::AgentLoopInput,
        ) -> ToolResult<crate::callback::SpawnedAgentLoop> {
            self.spawn_calls
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(crate::callback::SpawnedAgentLoop {
                agent_loop_id: wf_types::Id::from("spawned-run".to_string()),
                execution_id: wf_types::Id::from("spawned-run".to_string()),
                status: "started".to_string(),
            })
        }

        async fn execute_workflow(
            &self,
            _workflow_id: &str,
            _input: crate::callback::WorkflowInput,
        ) -> ToolResult<crate::callback::WorkflowOutput> {
            self.sync_workflow_calls
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(crate::callback::WorkflowOutput {
                execution_id: wf_types::Id::from("sync-wf".to_string()),
                result: serde_json::json!({"done": true}),
            })
        }

        async fn spawn_workflow(
            &self,
            _workflow_id: &str,
            _input: crate::callback::WorkflowInput,
        ) -> ToolResult<crate::callback::SpawnedWorkflow> {
            self.spawn_workflow_calls
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(crate::callback::SpawnedWorkflow {
                execution_id: wf_types::Id::from("spawned-wf".to_string()),
                status: "started".to_string(),
            })
        }

        async fn query_execution_status(
            &self,
            _execution_id: &str,
        ) -> ToolResult<crate::callback::ExecutionStatus> {
            Err(ToolError::ExecutionError("not supported".into()))
        }

        async fn cancel_execution(&self, _execution_id: &str) -> ToolResult<()> {
            Err(ToolError::ExecutionError("not supported".into()))
        }
    }

    #[tokio::test]
    async fn test_call_agent_wait_true_runs_sync() {
        let callback = Arc::new(RecordingCallback::default());
        let executor = BuiltinExecutor::new().with_callback(callback.clone());
        let ctx = ToolExecutionContext::new("exec-1".into());
        let options = wf_types::tool::ToolExecutionOptions {
            timeout: None,
            retries: None,
            retry_delay: None,
            exponential_backoff: None,
        };

        let result = executor
            .execute(
                &make_call_agent_tool(),
                &serde_json::json!({
                    "agent_id": "agent-a",
                    "agent_profile_id": "profile-a",
                    "prompt": "do the thing",
                }),
                &options,
                &ctx,
            )
            .await
            .unwrap();
        assert!(result.success);
        assert_eq!(
            callback
                .sync_calls
                .load(std::sync::atomic::Ordering::SeqCst),
            1
        );
        assert_eq!(
            callback
                .spawn_calls
                .load(std::sync::atomic::Ordering::SeqCst),
            0
        );
        let value = result.result.unwrap();
        assert_eq!(value["iterations"], 1);
        assert_eq!(value["result"], serde_json::json!({"synced": true}));
    }

    #[tokio::test]
    async fn test_call_agent_wait_false_dispatches_async() {
        let callback = Arc::new(RecordingCallback::default());
        let executor = BuiltinExecutor::new().with_callback(callback.clone());
        let ctx = ToolExecutionContext::new("exec-1".into());
        let options = wf_types::tool::ToolExecutionOptions {
            timeout: None,
            retries: None,
            retry_delay: None,
            exponential_backoff: None,
        };

        let result = executor
            .execute(
                &make_call_agent_tool(),
                &serde_json::json!({
                    "agent_id": "agent-a",
                    "agent_profile_id": "profile-a",
                    "prompt": "do the thing",
                    "wait": false,
                }),
                &options,
                &ctx,
            )
            .await
            .unwrap();
        assert!(result.success);
        assert_eq!(
            callback
                .sync_calls
                .load(std::sync::atomic::Ordering::SeqCst),
            0
        );
        assert_eq!(
            callback
                .spawn_calls
                .load(std::sync::atomic::Ordering::SeqCst),
            1
        );
        let value = result.result.unwrap();
        assert_eq!(value["execution_id"], "spawned-run");
        assert_eq!(value["status"], "started");
    }

    #[tokio::test]
    async fn test_call_agent_rejects_unknown_parameter() {
        let callback = Arc::new(RecordingCallback::default());
        let executor = BuiltinExecutor::new().with_callback(callback.clone());
        let ctx = ToolExecutionContext::new("exec-1".into());
        let options = wf_types::tool::ToolExecutionOptions {
            timeout: None,
            retries: None,
            retry_delay: None,
            exponential_backoff: None,
        };

        let err = executor
            .execute(
                &make_call_agent_tool(),
                &serde_json::json!({
                    "agent_id": "agent-a",
                    "agent_profile_id": "profile-a",
                    "prompt": "do the thing",
                    "wait": false,
                    "sneaky_extra": 1,
                }),
                &options,
                &ctx,
            )
            .await
            .expect_err("unknown parameter must be rejected by validation");
        assert!(err.to_string().contains("sneaky_extra"));
    }

    #[test]
    fn test_call_agent_schema_covers_every_read_parameter() {
        use std::collections::HashSet;
        let tool = make_call_agent_tool();
        let schema = tool.parameters.expect("call_agent must declare a schema");
        let declared: HashSet<&str> = schema.properties.keys().map(|k| k.as_str()).collect();
        for param in [
            "agent_id",
            "agent_profile_id",
            "prompt",
            "wait",
            "max_iterations",
            "max_execution_time",
            "hooks",
            "available_tool_names",
            "tool_call_format",
            "token_limit",
            "token_warning_threshold",
            "enable_token_tracking",
            "conversation",
        ] {
            assert!(
                declared.contains(param),
                "call_agent schema must declare '{param}'"
            );
        }
    }

    fn make_execute_workflow_tool() -> wf_types::tool::Tool {
        crate::predefined::workflow::EXECUTE_WORKFLOW.tool_def()
    }

    #[tokio::test]
    async fn test_execute_workflow_wait_true_runs_sync() {
        let callback = Arc::new(RecordingCallback::default());
        let executor = BuiltinExecutor::new().with_callback(callback.clone());
        let ctx = ToolExecutionContext::new("exec-1".into());
        let options = wf_types::tool::ToolExecutionOptions {
            timeout: None,
            retries: None,
            retry_delay: None,
            exponential_backoff: None,
        };

        let result = executor
            .execute(
                &make_execute_workflow_tool(),
                &serde_json::json!({
                    "workflow_id": "wf-a",
                    "input": { "text": "hi" },
                }),
                &options,
                &ctx,
            )
            .await
            .unwrap();
        assert!(result.success);
        assert_eq!(
            callback
                .sync_workflow_calls
                .load(std::sync::atomic::Ordering::SeqCst),
            1
        );
        assert_eq!(
            callback
                .spawn_workflow_calls
                .load(std::sync::atomic::Ordering::SeqCst),
            0
        );
        let value = result.result.unwrap();
        assert_eq!(value["execution_id"], "sync-wf");
        assert_eq!(value["result"], serde_json::json!({"done": true}));
    }

    #[tokio::test]
    async fn test_execute_workflow_wait_false_dispatches_async() {
        let callback = Arc::new(RecordingCallback::default());
        let executor = BuiltinExecutor::new().with_callback(callback.clone());
        let ctx = ToolExecutionContext::new("exec-1".into());
        let options = wf_types::tool::ToolExecutionOptions {
            timeout: None,
            retries: None,
            retry_delay: None,
            exponential_backoff: None,
        };

        let result = executor
            .execute(
                &make_execute_workflow_tool(),
                &serde_json::json!({
                    "workflow_id": "wf-a",
                    "input": { "text": "hi" },
                    "wait": false,
                }),
                &options,
                &ctx,
            )
            .await
            .unwrap();
        assert!(result.success);
        assert_eq!(
            callback
                .sync_workflow_calls
                .load(std::sync::atomic::Ordering::SeqCst),
            0
        );
        assert_eq!(
            callback
                .spawn_workflow_calls
                .load(std::sync::atomic::Ordering::SeqCst),
            1
        );
        let value = result.result.unwrap();
        assert_eq!(value["execution_id"], "spawned-wf");
        assert_eq!(value["status"], "started");
    }

    #[test]
    fn test_execute_workflow_schema_covers_every_read_parameter() {
        use std::collections::HashSet;
        let tool = make_execute_workflow_tool();
        let schema = tool
            .parameters
            .expect("execute_workflow must declare a schema");
        let declared: HashSet<&str> = schema.properties.keys().map(|k| k.as_str()).collect();
        for param in ["workflow_id", "input", "wait"] {
            assert!(
                declared.contains(param),
                "execute_workflow schema must declare '{param}'"
            );
        }
    }

    #[tokio::test]
    async fn test_unknown_builtin_tool_returns_not_found() {
        let executor = BuiltinExecutor::new();
        let ctx = ToolExecutionContext::new("exec-1".into());
        let result = executor
            .dispatch("no_such_builtin", &serde_json::json!({}), &ctx)
            .await;
        assert!(matches!(result, Err(ToolError::NotFound(_))));
    }

    #[tokio::test]
    async fn test_register_injected_handler_reaches_execution() {
        struct EchoHandler;

        #[async_trait]
        impl BuiltinToolHandler for EchoHandler {
            fn tool_name(&self) -> &'static str {
                "echo_builtin"
            }

            async fn handle(
                &self,
                parameters: &Value,
                _context: &ToolExecutionContext,
                _resources: &BuiltinHandlerResources,
            ) -> ToolResult<Value> {
                Ok(parameters.clone())
            }
        }

        let executor = BuiltinExecutor::new();
        executor.register_builtin_handler("echo_builtin", Arc::new(EchoHandler));

        let tool = wf_types::tool::Tool {
            id: "echo_builtin".into(),
            name: "echo_builtin".into(),
            description: "Echo".into(),
            tool_type: wf_types::tool::ToolType::BuiltIn,
            parameters: None,
            metadata: None,
            config: None,
            enabled: Some(true),
            strict: None,
            default_timeout_ms: None,
        };
        let ctx = ToolExecutionContext::new("exec-1".into());
        let options = wf_types::tool::ToolExecutionOptions {
            timeout: None,
            retries: None,
            retry_delay: None,
            exponential_backoff: None,
        };
        let result = executor
            .execute(&tool, &serde_json::json!({"x": 1}), &options, &ctx)
            .await
            .unwrap();
        assert!(result.success);
        assert_eq!(result.result.unwrap(), serde_json::json!({"x": 1}));
    }

    #[test]
    fn test_default_handlers_registered() {
        let executor = BuiltinExecutor::new();
        for name in [
            "call_agent",
            "execute_workflow",
            "query_workflow_status",
            "cancel_workflow",
            "skill",
        ] {
            assert!(
                executor.handlers.contains_key(name),
                "default handler '{}' must be registered",
                name
            );
        }
    }

    #[test]
    fn test_call_agent_handler_type_parses_typed_params() {
        let value = serde_json::json!({
            "agent_id": "a",
            "agent_profile_id": "p",
            "prompt": "hi",
            "wait": false,
            "max_iterations": 5,
            "available_tool_names": ["read_file"],
            "enable_token_tracking": true,
        });
        let params: crate::executor::builtin_handlers::CallAgentParams =
            serde_json::from_value(value).unwrap();
        assert_eq!(params.agent_id, "a");
        assert!(!params.wait);
        assert_eq!(params.max_iterations, Some(5));
        assert_eq!(params.available_tool_names, vec!["read_file".to_string()]);
        assert!(params.enable_token_tracking.is_some());
    }

    #[test]
    fn test_builtin_handler_register_defaults_injects_call_agent() {
        let map = Arc::new(DashMap::new());
        register_default_builtin_handlers(&map);
        let handler = map.get("call_agent").unwrap();
        assert_eq!(handler.tool_name(), "call_agent");
        assert!(map.contains_key("execute_workflow"));
        assert!(map.contains_key("query_workflow_status"));
        assert!(map.contains_key("cancel_workflow"));
        assert!(map.contains_key("skill"));
    }
}
