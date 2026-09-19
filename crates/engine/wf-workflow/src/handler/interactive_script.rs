use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use wf_execution_shared::context::{NodeExecutionContext, NodeExecutionResult};
use wf_execution_shared::script_router::ScriptRouter;
use wf_sandbox::SandboxRuntime;
use wf_script::{InteractionMode, ScriptDefinition, ScriptEngine, ScriptEngineOptions};
use wf_types::node::StaticNodeType;

use crate::error::{WorkflowError, WorkflowResult};
use crate::handler::output_mapping;
use crate::handler::NodeHandler;
use crate::interactive_script_session::{
    drive_session, InteractiveScriptSessionConfig, InteractiveScriptSessionEntity,
    LlmSuggestionProvider, RoundCapture, SessionDriverContext, SessionRegistry, SuggestionProvider,
};

pub struct InteractiveScriptHandler {
    sandbox: Option<Arc<SandboxRuntime>>,
    router: Option<Arc<ScriptRouter>>,
    gateway: Option<Arc<wf_llm::LlmGateway>>,
    llm_profile_id: Option<String>,
    file_checkpoint: Option<wf_checkpoint::file::FileCheckpointManager>,
    sessions: Arc<SessionRegistry>,
}

impl InteractiveScriptHandler {
    pub fn new() -> Self {
        Self {
            sandbox: None,
            router: None,
            gateway: None,
            llm_profile_id: None,
            file_checkpoint: None,
            sessions: Arc::new(SessionRegistry::default()),
        }
    }

    pub fn with_sandbox(sandbox: Arc<SandboxRuntime>) -> Self {
        Self {
            sandbox: Some(sandbox),
            router: None,
            gateway: None,
            llm_profile_id: None,
            file_checkpoint: None,
            sessions: Arc::new(SessionRegistry::default()),
        }
    }

    pub fn with_sandbox_opt(sandbox: Option<Arc<SandboxRuntime>>) -> Self {
        Self {
            sandbox,
            router: None,
            gateway: None,
            llm_profile_id: None,
            file_checkpoint: None,
            sessions: Arc::new(SessionRegistry::default()),
        }
    }

    pub fn with_script_router(mut self, router: Arc<ScriptRouter>) -> Self {
        self.router = Some(router);
        self
    }

    pub fn with_script_router_opt(mut self, router: Option<Arc<ScriptRouter>>) -> Self {
        self.router = router;
        self
    }

    pub fn with_llm_gateway_opt(mut self, gateway: Option<Arc<wf_llm::LlmGateway>>) -> Self {
        self.gateway = gateway;
        self
    }

    pub fn with_llm_profile_id(mut self, profile_id: impl Into<String>) -> Self {
        self.llm_profile_id = Some(profile_id.into());
        self
    }

    pub fn with_file_checkpoint_opt(
        mut self,
        manager: Option<wf_checkpoint::file::FileCheckpointManager>,
    ) -> Self {
        self.file_checkpoint = manager;
        self
    }

    pub fn with_session_registry(mut self, registry: Arc<SessionRegistry>) -> Self {
        self.sessions = registry;
        self
    }

    pub fn session_registry(&self) -> &Arc<SessionRegistry> {
        &self.sessions
    }

    fn get_sandbox(&self) -> Arc<SandboxRuntime> {
        self.sandbox
            .clone()
            .unwrap_or_else(|| Arc::new(SandboxRuntime::new()))
    }
}

impl Default for InteractiveScriptHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl NodeHandler for InteractiveScriptHandler {
    fn node_type(&self) -> StaticNodeType {
        StaticNodeType::InteractiveScript
    }

    async fn execute(
        &self,
        ctx: &mut NodeExecutionContext,
    ) -> wf_execution_shared::error::ExecutionSharedResult<NodeExecutionResult> {
        self.execute_inner(ctx).await.map_err(Into::into)
    }
}

impl InteractiveScriptHandler {
    async fn execute_inner(
        &self,
        ctx: &mut NodeExecutionContext,
    ) -> WorkflowResult<NodeExecutionResult> {
        let config = ctx.node_config.clone().unwrap_or(Value::Null);
        let script_name = config
            .get("script_name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                WorkflowError::Internal(
                    "InteractiveScript node requires a 'script_name' config".to_string(),
                )
            })?
            .to_string();

        let definition = crate::registry::lookup_script(&script_name).ok_or_else(|| {
            WorkflowError::Internal(format!(
                "InteractiveScript node '{}': script '{}' is not registered",
                ctx.node_id, script_name
            ))
        })?;

        let provided = config
            .get("arguments")
            .and_then(|v| v.as_object())
            .map(|map| {
                map.iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect::<std::collections::HashMap<String, Value>>()
            })
            .unwrap_or_default();
        let context_variables = crate::handler::script::snapshot_variables(ctx);
        let command = match definition.template.clone() {
            Some(template) => crate::handler::script::render_blueprint_command(
                &format!("InteractiveScript node '{}'", ctx.node_id),
                &template,
                &definition.arguments.clone().unwrap_or_default(),
                &provided,
                &context_variables,
            )?,
            None => definition.content.clone().unwrap_or_default(),
        };

        let node_policy = match config.get("security_policy") {
            None | Some(Value::Null) => None,
            Some(v) => Some(
                serde_json::from_value::<wf_script::ScriptSecurityPolicy>(v.clone()).map_err(
                    |e| {
                        WorkflowError::Internal(format!(
                            "InteractiveScript node '{}': invalid 'security_policy': {e}",
                            ctx.node_id
                        ))
                    },
                )?,
            ),
        };
        if let Some(policy) = node_policy.or_else(|| definition.security_policy.clone()) {
            let gate = ScriptDefinition {
                name: script_name.clone(),
                content: Some(command.clone()),
                template: None,
                arguments: None,
                language: definition.language.clone(),
                executor_mode: None,
                interactive: None,
                security_policy: None,
                description: None,
                enabled: None,
            };
            ScriptEngine::check_security_policy(&gate, &policy).map_err(|e| {
                WorkflowError::Internal(format!("InteractiveScript node '{}': {e}", ctx.node_id))
            })?;
        }

        let language = config
            .get("executor")
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .or_else(|| definition.language.clone())
            .ok_or_else(|| {
                WorkflowError::Internal(format!(
                    "InteractiveScript node '{}': language undetermined (set node 'executor' or script language)",
                    ctx.node_id
                ))
            })?;

        if is_shell_language(&language) {
            return self
                .execute_session(ctx, &config, &script_name, &language, &command)
                .await;
        }

        self.execute_single_shot(ctx, &config, &script_name, &language, &command)
            .await
    }

    /// Stateful path: shell scripts run inside a PTY session entity that can
    /// pause for input across multiple rounds.
    async fn execute_session(
        &self,
        ctx: &mut NodeExecutionContext,
        config: &Value,
        script_name: &str,
        language: &str,
        code: &str,
    ) -> WorkflowResult<NodeExecutionResult> {
        let output_mapping = config.get("output_mapping");
        let mode = InteractiveScriptSessionConfig::mode_from_str(
            config.get("interaction_mode").and_then(|v| v.as_str()),
        );
        let max_rounds = config
            .get("max_rounds")
            .and_then(|v| v.as_u64())
            .map(|v| v.min(u64::from(u32::MAX)) as u32)
            .unwrap_or(10);
        let round_timeout_ms = config
            .get("round_timeout")
            .and_then(|v| v.as_u64())
            .unwrap_or(30_000);
        let session_timeout_ms = config.get("session_timeout").and_then(|v| v.as_u64());
        let debounce_ms = config
            .get("debounce_ms")
            .and_then(|v| v.as_u64())
            .unwrap_or(300);
        let hybrid_fallback_to_suggestion = config
            .get("hybrid_fallback_to_suggestion")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        let max_autonomous_rounds = config
            .get("max_autonomous_rounds")
            .and_then(|v| v.as_u64())
            .map(|v| v.min(u64::from(u32::MAX)) as u32);
        let max_output_bytes = config.get("max_output_bytes").and_then(|v| v.as_u64());
        let llm_profile_id = config
            .get("llm_profile_id")
            .or_else(|| config.get("profile_id"))
            .and_then(|v| v.as_str())
            .map(String::from)
            .or_else(|| self.llm_profile_id.clone());
        let prompt_patterns = config
            .get("prompt_patterns")
            .and_then(|v| v.as_array())
            .map(|items| {
                items
                    .iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        let environment = config
            .get("environment")
            .and_then(|v| v.as_object())
            .map(|map| {
                map.iter()
                    .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                    .collect()
            })
            .unwrap_or_default();

        let mut preset_inputs: Vec<Value> = config
            .get("interaction_inputs")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        let preset_user_input = ctx
            .get_variable("__interaction_input__")
            .unwrap_or(Value::Null);
        if preset_user_input != Value::Null {
            preset_inputs.insert(0, preset_user_input);
        }

        let working_directory = config
            .get("working_directory")
            .and_then(|v| v.as_str())
            .map(String::from)
            .or_else(|| {
                self.file_checkpoint
                    .as_ref()
                    .and_then(|manager| manager.workspace_root())
                    .map(|root| root.to_string_lossy().to_string())
            });
        let session_config = InteractiveScriptSessionConfig {
            script_name: script_name.to_string(),
            language: language.to_string(),
            command: code.to_string(),
            interaction_mode: mode.clone(),
            max_rounds,
            round_timeout_ms,
            prompt_patterns,
            working_directory: working_directory.clone(),
            environment,
            session_timeout_ms,
            debounce_ms,
            hybrid_fallback_to_suggestion,
            max_autonomous_rounds,
            max_output_bytes,
            llm_profile_id: llm_profile_id.clone(),
        };
        let session_id = wf_common::generate_id();
        let mut ancestors = ctx
            .parent_execution_id
            .clone()
            .into_iter()
            .collect::<Vec<_>>();
        ancestors.push(ctx.execution_id.clone());
        let entity = Arc::new(
            InteractiveScriptSessionEntity::new(session_id.clone(), session_config)
                .with_hierarchy_depth(ctx.depth.saturating_add(1))
                .with_ancestors(ancestors)
                .with_parent_execution_id(ctx.execution_id.clone()),
        );

        let router = self
            .router
            .clone()
            .unwrap_or_else(|| Arc::new(ScriptRouter::with_sandbox(self.get_sandbox())));
        let suggester: Option<Arc<dyn SuggestionProvider>> = self.gateway.clone().map(|gateway| {
            Arc::new(LlmSuggestionProvider {
                gateway,
                profile_id: llm_profile_id.unwrap_or_else(|| "default".to_string()),
                execution_id: ctx.execution_id.to_string(),
            }) as Arc<dyn SuggestionProvider>
        });
        let round_capture = self.file_checkpoint.clone().and_then(|manager| {
            let scope = working_directory.clone().into_iter().collect::<Vec<_>>();
            if scope.is_empty() {
                return None;
            }
            Some(RoundCapture {
                manager,
                entity_id: session_id.clone(),
                parent_execution_id: Some(ctx.execution_id.to_string()),
                scope,
            })
        });
        let driver = SessionDriverContext {
            execution_id: ctx.execution_id.to_string(),
            node_id: ctx.node_id.clone(),
            event_bus: ctx.event_bus.clone(),
            interaction_registry: None,
            suggester,
            round_capture,
        };
        let had_preset = !preset_inputs.is_empty();
        self.sessions.register(entity.clone());
        let outcome = drive_session(&entity, router.shell_store(), &driver, preset_inputs).await;
        self.sessions.remove(&session_id);
        if let Some(shell_session) = entity.shell_session() {
            let _ = router.shell_store().kill(&shell_session);
        }
        let outcome = outcome?;

        let output = Value::String(outcome.output.clone());
        let parsed_output = serde_json::from_str::<Value>(&outcome.output).unwrap_or(output);

        if let Some(mapping) = output_mapping {
            output_mapping::apply_output_mappings(ctx, &parsed_output, mapping)?;
        }

        ctx.set_internal_variable("__interaction_output__", parsed_output.clone());

        let mut metadata = std::collections::HashMap::new();
        metadata.insert("language".to_string(), Value::String(language.to_string()));
        metadata.insert(
            "interaction_mode".to_string(),
            Value::String(match mode {
                InteractionMode::Blocking => "blocking".to_string(),
                InteractionMode::LlmAssisted => "llm_assisted".to_string(),
                InteractionMode::Hybrid => "hybrid".to_string(),
            }),
        );
        metadata.insert(
            "completed_rounds".to_string(),
            Value::Number(outcome.completed_rounds.into()),
        );
        metadata.insert("had_input".to_string(), Value::Bool(had_preset));
        metadata.insert(
            "output_truncated".to_string(),
            Value::Bool(outcome.output_truncated),
        );
        metadata.insert("session_id".to_string(), Value::String(session_id.clone()));

        Ok(NodeExecutionResult {
            output: parsed_output,
            next_node_ids: Vec::new(),
            metadata,
        })
    }

    /// Single-shot path: non-shell languages keep the previous sandbox
    /// behavior; multi-round PTY interaction is a shell-only capability.
    async fn execute_single_shot(
        &self,
        ctx: &mut NodeExecutionContext,
        config: &Value,
        script_name: &str,
        language: &str,
        code: &str,
    ) -> WorkflowResult<NodeExecutionResult> {
        let interaction_mode = config.get("interaction_mode").and_then(|v| v.as_str());
        let output_mapping = config.get("output_mapping");
        let sandbox_config =
            crate::handler::script::sandbox_config_from_node(config, language, &ctx.node_id)?;

        let user_input = if interaction_mode.is_some() {
            ctx.get_variable("__interaction_input__")
                .unwrap_or(Value::Null)
        } else {
            Value::Null
        };

        let augmented_code = if user_input != Value::Null {
            format!("{}\n\n# User input: {}", code, user_input)
        } else {
            code.to_string()
        };

        let executor_mode = crate::handler::script::parse_executor_mode(
            config.get("executor_mode").and_then(|v| v.as_str()),
            language,
        )?;
        let script = ScriptDefinition {
            name: script_name.to_string(),
            content: Some(augmented_code),
            template: None,
            arguments: None,
            language: Some(language.to_string()),
            executor_mode: None,
            interactive: None,
            security_policy: None,
            description: None,
            enabled: None,
        };
        let mut options =
            crate::handler::script::script_execution_options_from_config(config, &ctx.node_id)?;
        options.executor_mode = Some(executor_mode);
        let provided = config
            .get("arguments")
            .and_then(|v| v.as_object())
            .map(|map| {
                map.iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect::<std::collections::HashMap<String, Value>>()
            })
            .unwrap_or_default();
        let engine_options = ScriptEngineOptions {
            args: provided,
            context_variables: crate::handler::script::snapshot_variables(ctx),
        };
        let router = self
            .router
            .clone()
            .unwrap_or_else(|| Arc::new(ScriptRouter::with_sandbox(self.get_sandbox())));
        let routed = router
            .run(
                &script,
                Some(&options),
                &engine_options,
                Some(sandbox_config),
            )
            .await;
        let result = routed.result;

        if result.requires_review {
            return Err(WorkflowError::Internal(format!(
                "Interactive script '{script_name}' requires human review before execution"
            )));
        }

        if !result.success {
            let stderr = result.error.as_deref().unwrap_or("unknown error");
            return Err(WorkflowError::Internal(format!(
                "Interactive script failed: {}",
                stderr
            )));
        }

        let output = result
            .stdout
            .clone()
            .map(Value::String)
            .unwrap_or(Value::Null);

        let parsed_output = result
            .stdout
            .as_deref()
            .and_then(|s| serde_json::from_str::<Value>(s).ok())
            .unwrap_or(output);

        if let Some(mapping) = output_mapping {
            output_mapping::apply_output_mappings(ctx, &parsed_output, mapping)?;
        }

        ctx.set_internal_variable("__interaction_output__", parsed_output.clone());

        let mut metadata = std::collections::HashMap::new();
        metadata.insert("language".to_string(), Value::String(language.to_string()));
        metadata.insert(
            "had_input".to_string(),
            Value::Bool(user_input != Value::Null),
        );
        if let Some(strategy) = routed.strategy_id {
            metadata.insert("strategy".to_string(), Value::String(strategy));
        }

        Ok(NodeExecutionResult {
            output: parsed_output,
            next_node_ids: Vec::new(),
            metadata,
        })
    }
}

fn is_shell_language(language: &str) -> bool {
    matches!(
        language.to_lowercase().as_str(),
        "shell" | "bash" | "sh" | "zsh"
    )
}
