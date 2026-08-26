use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use dashmap::DashMap;
use serde_json::Value;
use tokio_util::sync::CancellationToken;
use wf_core::internal_signal::InternalSignalBus;
use wf_core::EventBus;
use wf_metrics::MetricsRegistry;
use wf_sandbox::SandboxRuntime;
use wf_types::events::{BaseEvent, EventType};
use wf_types::node::StaticNodeType;
use wf_types::trigger::{TriggerAction, TriggerExecutionResult};
use wf_types::Id;

use crate::coordinator::WorkflowCoordinator;
use crate::error::{WorkflowError, WorkflowResult};
use crate::handler::NodeHandler;
use crate::handler::{variable_mapping, HandlerRegistry};
use crate::registry::{lookup_graph, lookup_script, ScriptRegistry};
use crate::trigger_internal;
use crate::WorkflowExecutionEntity;
use wf_execution_shared::context::ExecutorContext;
use wf_tools::registry::ToolRegistry;
use wf_types::script::sandbox::{SandboxConfig, ScriptExecutionResult};
use wf_types::workflow_execution::WorkflowExecutionOptions;

/// Executes a script inside a sandbox.
///
/// Production uses the wf-sandbox runtime; unit tests inject a mock so
/// handler tests stay hermetic (no real interpreter subprocess, no load
/// sensitivity).
#[async_trait]
pub trait ScriptRunner: Send + Sync {
    async fn execute(
        &self,
        language: &str,
        code: &str,
        config: &SandboxConfig,
    ) -> ScriptExecutionResult;
}

/// wf-sandbox-backed [`ScriptRunner`].
pub struct SandboxScriptRunner {
    sandbox: Arc<SandboxRuntime>,
}

impl SandboxScriptRunner {
    pub fn new() -> Self {
        Self {
            sandbox: Arc::new(SandboxRuntime::new()),
        }
    }
}

impl Default for SandboxScriptRunner {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ScriptRunner for SandboxScriptRunner {
    async fn execute(
        &self,
        language: &str,
        code: &str,
        config: &SandboxConfig,
    ) -> ScriptExecutionResult {
        self.sandbox.execute(language, code, config).await
    }
}

#[derive(Clone)]
pub struct TriggerContext {
    pub execution_id: Id,
    pub workflow_id: Id,
    pub variables: Arc<DashMap<String, Value>>,
    pub event_bus: Option<Arc<EventBus>>,
    /// Typed signal bus for internal workflow/agent signals
    /// (replaces the `__`-prefixed variable protocol).
    pub signal_bus: Option<Arc<InternalSignalBus>>,
    pub handlers: Option<Arc<HashMap<StaticNodeType, Box<dyn NodeHandler>>>>,
    pub tool_registry: Option<Arc<ToolRegistry>>,
    pub metrics: Option<Arc<MetricsRegistry>>,
    pub script_runner: Option<Arc<dyn ScriptRunner>>,
    pub script_registry: Option<Arc<ScriptRegistry>>,
    /// Abort signal of the owning execution; background triggered
    /// sub-workflows race against it so a cancelled parent stops them.
    pub cancellation: Option<CancellationToken>,
    /// Optional session-level cache shared across multiple trigger actions
    /// within the same message node. Allows actions to share intermediate
    /// state without resorting to global variables.
    pub session_cache: Option<Arc<Mutex<HashMap<String, Value>>>>,
}

impl TriggerContext {
    pub fn new(execution_id: Id, workflow_id: Id) -> Self {
        Self {
            execution_id,
            workflow_id,
            variables: Arc::new(DashMap::new()),
            event_bus: None,
            signal_bus: None,
            handlers: None,
            tool_registry: None,
            metrics: None,
            script_runner: None,
            script_registry: None,
            cancellation: None,
            session_cache: None,
        }
    }

    pub fn with_variables(mut self, variables: Arc<DashMap<String, Value>>) -> Self {
        self.variables = variables;
        self
    }

    pub fn with_event_bus(mut self, bus: Arc<EventBus>) -> Self {
        self.event_bus = Some(bus);
        self
    }

    /// Inject a typed signal bus for internal signals.
    pub fn with_signal_bus(mut self, bus: Arc<InternalSignalBus>) -> Self {
        self.signal_bus = Some(bus);
        self
    }

    pub fn with_handlers(
        mut self,
        handlers: Arc<HashMap<StaticNodeType, Box<dyn NodeHandler>>>,
    ) -> Self {
        self.handlers = Some(handlers);
        self
    }

    pub fn with_tool_registry(mut self, registry: Arc<ToolRegistry>) -> Self {
        self.tool_registry = Some(registry);
        self
    }

    pub fn with_metrics(mut self, metrics: Arc<MetricsRegistry>) -> Self {
        self.metrics = Some(metrics);
        self
    }

    pub fn with_script_runner(mut self, runner: Arc<dyn ScriptRunner>) -> Self {
        self.script_runner = Some(runner);
        self
    }

    pub fn with_script_registry(mut self, registry: Arc<ScriptRegistry>) -> Self {
        self.script_registry = Some(registry);
        self
    }

    pub fn with_cancellation(mut self, token: CancellationToken) -> Self {
        self.cancellation = Some(token);
        self
    }

    /// Attach a session-level cache shared across multiple trigger actions
    /// within the same message node. The cache is a simple key-value store
    /// scoped to the session; it is not persisted or checkpointed.
    pub fn with_session_cache(mut self, cache: Arc<Mutex<HashMap<String, Value>>>) -> Self {
        self.session_cache = Some(cache);
        self
    }
}

pub struct TriggerCoordinator;

/// Everything needed to run a triggered sub-workflow.
struct TriggeredSubworkflowRun {
    triggered_workflow_id: String,
    graph: wf_types::workflow_execution::WorkflowGraphStructure,
    handlers: Arc<HashMap<StaticNodeType, Box<dyn NodeHandler>>>,
    tool_registry: Arc<ToolRegistry>,
    input_mapping: HashMap<String, Value>,
    output_mapping: HashMap<String, Value>,
    timeout: u64,
}

impl TriggerCoordinator {
    pub async fn execute(
        action: &TriggerAction,
        trigger_id: &str,
        ctx: &TriggerContext,
    ) -> TriggerExecutionResult {
        let start = wf_common::now();
        let result = match action {
            TriggerAction::StopWorkflowExecution { .. } => Self::handle_stop_workflow(ctx).await,
            TriggerAction::PauseWorkflowExecution { .. } => Self::handle_pause_workflow(ctx).await,
            TriggerAction::ResumeWorkflowExecution { .. } => {
                Self::handle_resume_workflow(ctx).await
            }
            TriggerAction::SkipNode { node_id } => {
                Self::handle_skip_node(node_id.as_deref().unwrap_or(""), ctx).await
            }
            TriggerAction::SetVariable {
                variable_name,
                value,
            } => Self::handle_set_variable(variable_name, value.clone(), ctx).await,
            TriggerAction::SendNotification { message } => {
                Self::handle_send_notification(message, ctx).await
            }
            TriggerAction::ExecuteTriggeredSubworkflow { .. } => {
                Self::handle_execute_subworkflow(action, ctx).await
            }
            TriggerAction::ExecuteScript { .. } => Self::handle_execute_script(action, ctx).await,
            TriggerAction::SetMessageContext {
                context_id,
                messages,
            } => Self::handle_set_message_context(context_id, messages.clone(), ctx).await,
            TriggerAction::AppendMessageContext {
                context_id,
                messages,
            } => Self::handle_append_message_context(context_id, messages.clone(), ctx).await,
            // Nested agent executions are an event-driven trigger feature
            // (wf-runtime `AgentTriggerRunner`): message nodes have no parent
            // `AgentLoopEntity` / conversation session / `AgentLoopRegistry`,
            // so no anchored input context or write-back target can be
            // constructed for the child. Kept rejected with an explicit
            // error (never silently degraded); the support matrix documents
            // this as `❌ rejected with an explicit error`.
            TriggerAction::ExecuteTriggeredAgentExecution { .. } => {
                Err(WorkflowError::TriggerError(
                    "ExecuteTriggeredAgentExecution is only supported by the event-driven trigger \
                     listener; message nodes reject this action"
                        .to_string(),
                ))
            }
        };

        let (result_val, error_val) = match result {
            Ok(val) => (Some(val), None),
            Err(e) => (None, Some(e.to_string())),
        };

        TriggerExecutionResult {
            trigger_id: Id::from(trigger_id),
            success: error_val.is_none(),
            execution_id: Some(ctx.execution_id.clone()),
            result: result_val,
            error: error_val,
            execution_time: wf_common::now() - start,
        }
    }

    async fn handle_stop_workflow(ctx: &TriggerContext) -> WorkflowResult<Value> {
        // Legacy variable protocol (backward compatible).
        trigger_internal::set_flag(&ctx.variables, trigger_internal::TRIGGER_STOP);
        // Typed signal bus (new code path).
        if let Some(bus) = &ctx.signal_bus {
            trigger_internal::publish_stop_signal(
                bus,
                ctx.execution_id.clone(),
                ctx.execution_id.clone(),
                None,
            );
        }
        Self::emit(
            ctx,
            EventType::ExecutionStopped,
            "workflow_stopped_by_trigger",
        )
        .await;
        Ok(Value::String("workflow_stopped".to_string()))
    }

    async fn handle_pause_workflow(ctx: &TriggerContext) -> WorkflowResult<Value> {
        // Legacy variable protocol (backward compatible).
        trigger_internal::set_flag(&ctx.variables, trigger_internal::TRIGGER_PAUSE);
        // Typed signal bus (new code path).
        if let Some(bus) = &ctx.signal_bus {
            trigger_internal::publish_pause_signal(
                bus,
                ctx.execution_id.clone(),
                ctx.execution_id.clone(),
                None,
            );
        }
        Self::emit(
            ctx,
            EventType::WorkflowExecutionPaused,
            "workflow_paused_by_trigger",
        )
        .await;
        Ok(Value::String("workflow_paused".to_string()))
    }

    async fn handle_resume_workflow(ctx: &TriggerContext) -> WorkflowResult<Value> {
        // Legacy variable protocol (backward compatible).
        trigger_internal::clear_flag(&ctx.variables, trigger_internal::TRIGGER_PAUSE);
        // Typed signal bus (new code path).
        if let Some(bus) = &ctx.signal_bus {
            trigger_internal::publish_resume_signal(
                bus,
                ctx.execution_id.clone(),
                ctx.execution_id.clone(),
            );
        }
        Self::emit(
            ctx,
            EventType::WorkflowExecutionResumed,
            "workflow_resumed_by_trigger",
        )
        .await;
        Ok(Value::String("workflow_resumed".to_string()))
    }

    async fn handle_skip_node(node_id: &str, ctx: &TriggerContext) -> WorkflowResult<Value> {
        // Legacy variable protocol (backward compatible).
        trigger_internal::set_flag(&ctx.variables, &trigger_internal::skip_marker(node_id));
        // Typed signal bus (new code path).
        if let Some(bus) = &ctx.signal_bus {
            trigger_internal::publish_skip_signal(
                bus,
                ctx.execution_id.clone(),
                ctx.execution_id.clone(),
                node_id.to_string(),
            );
        }
        Self::emit_with_metadata(
            ctx,
            EventType::NodeSkipped,
            &format!("node_skipped:{}", node_id),
            &[("node_id", Value::String(node_id.to_string()))],
        )
        .await;
        Ok(serde_json::json!({"skipped_node": node_id}))
    }

    async fn handle_set_variable(
        var_name: &str,
        var_value: Value,
        ctx: &TriggerContext,
    ) -> WorkflowResult<Value> {
        // Engine-internal state (loop stacks, message contexts, fork
        // handovers, interaction markers) lives under the reserved `__`
        // prefix; refusing it here keeps SetVariable from corrupting that
        // state (e.g. writing `__msg_ctx__*` directly bypasses the token
        // ledger). Message-context updates must use the dedicated
        // `SetMessageContext` / `AppendMessageContext` actions instead.
        if var_name.starts_with("__") {
            return Err(WorkflowError::TriggerError(format!(
                "SetVariable refuses to write internal variable '{}' (reserved '__' prefix); use SetMessageContext/AppendMessageContext for message contexts",
                var_name
            )));
        }
        ctx.variables
            .insert(var_name.to_string(), var_value.clone());
        Self::emit(
            ctx,
            EventType::VariableChanged,
            &format!("variable_set:{}", var_name),
        )
        .await;
        Ok(serde_json::json!({"variable": var_name, "value": var_value}))
    }

    /// Replace the full content of a named message context (ledger-safe:
    /// goes through `message_context::register_context`, which marks the
    /// token ledger dirty so the next read recomputes the estimate).
    async fn handle_set_message_context(
        context_id: &str,
        messages: Vec<wf_types::message::Message>,
        ctx: &TriggerContext,
    ) -> WorkflowResult<Value> {
        crate::message_context::register_context(&ctx.variables, context_id, messages.clone());
        Self::emit_with_metadata(
            ctx,
            EventType::MessageContextUpdated,
            &format!("message_context_set:{}", context_id),
            &[
                ("context_id", Value::String(context_id.to_string())),
                (
                    "message_count",
                    Value::Number(serde_json::Number::from(messages.len() as u64)),
                ),
            ],
        )
        .await;
        Ok(serde_json::json!({
            "context_id": context_id,
            "message_count": messages.len(),
        }))
    }

    /// Append messages to a named message context, creating it when absent
    /// (ledger-safe incremental append).
    async fn handle_append_message_context(
        context_id: &str,
        messages: Vec<wf_types::message::Message>,
        ctx: &TriggerContext,
    ) -> WorkflowResult<Value> {
        if messages.is_empty() {
            return Ok(serde_json::json!({
                "context_id": context_id,
                "message_count": 0,
            }));
        }
        crate::message_context::append_context(&ctx.variables, context_id, messages.clone());
        Self::emit_with_metadata(
            ctx,
            EventType::MessageContextUpdated,
            &format!("message_context_appended:{}", context_id),
            &[
                ("context_id", Value::String(context_id.to_string())),
                (
                    "message_count",
                    Value::Number(serde_json::Number::from(messages.len() as u64)),
                ),
            ],
        )
        .await;
        Ok(serde_json::json!({
            "context_id": context_id,
            "appended": messages.len(),
        }))
    }

    async fn handle_send_notification(
        message: &str,
        ctx: &TriggerContext,
    ) -> WorkflowResult<Value> {
        Self::emit_with_metadata(
            ctx,
            EventType::NotificationSent,
            &format!("notification:{}", message),
            &[("message", Value::String(message.to_string()))],
        )
        .await;
        Ok(serde_json::json!({"sent": true, "message": message}))
    }

    async fn handle_execute_subworkflow(
        action: &TriggerAction,
        ctx: &TriggerContext,
    ) -> WorkflowResult<Value> {
        let (triggered_workflow_id, wait_for_completion, input_mapping, output_mapping, timeout) =
            match action {
                TriggerAction::ExecuteTriggeredSubworkflow {
                    triggered_workflow_id,
                    wait_for_completion,
                    input_mapping,
                    output_mapping,
                    timeout,
                } => (
                    triggered_workflow_id.clone(),
                    wait_for_completion.unwrap_or(true),
                    input_mapping.clone().unwrap_or_default(),
                    output_mapping.clone().unwrap_or_default(),
                    timeout.unwrap_or(0),
                ),
                _ => return Err(WorkflowError::Internal("Invalid action type".to_string())),
            };

        let graph = lookup_graph(&triggered_workflow_id).ok_or_else(|| {
            WorkflowError::TriggerError(format!(
                "Triggered workflow '{}' not found in graph registry",
                triggered_workflow_id
            ))
        })?;

        Self::emit(
            ctx,
            EventType::TriggeredSubgraphStarted,
            &format!("triggered_subworkflow:{}", triggered_workflow_id),
        )
        .await;

        let handlers = ctx.handlers.clone().unwrap_or_else(|| {
            let mut registry = HandlerRegistry::new();
            registry.register_defaults(std::sync::Arc::new(wf_llm::LlmGateway::new()));
            registry.into_arc()
        });
        let tool_registry = ctx
            .tool_registry
            .clone()
            .unwrap_or_else(|| Arc::new(ToolRegistry::new()));

        if !wait_for_completion {
            let tctx = ctx.clone();
            let cancellation = tctx.cancellation.clone();
            let execution_id = wf_common::generate_id();
            let run = TriggeredSubworkflowRun {
                triggered_workflow_id: triggered_workflow_id.clone(),
                graph,
                handlers,
                tool_registry,
                input_mapping: input_mapping.clone(),
                output_mapping: output_mapping.clone(),
                timeout,
            };
            let exec_id = execution_id.clone();
            tokio::spawn(async move {
                let subworkflow = Self::run_triggered_subworkflow(&tctx, run);
                match cancellation {
                    Some(token) => {
                        // A cancelled parent aborts the background sub-workflow.
                        tokio::select! {
                            _ = subworkflow => {}
                            _ = token.cancelled() => {}
                        }
                    }
                    None => {
                        let _ = subworkflow.await;
                    }
                }
            });
            return Ok(serde_json::json!({
                "submitted": true,
                "workflow_id": triggered_workflow_id,
                "execution_id": exec_id,
            }));
        }

        let execution_id = wf_common::generate_id();
        let run = TriggeredSubworkflowRun {
            triggered_workflow_id: triggered_workflow_id.clone(),
            graph,
            handlers,
            tool_registry,
            input_mapping,
            output_mapping,
            timeout,
        };
        match Self::run_triggered_subworkflow(ctx, run).await {
            Ok(result) => Ok(serde_json::json!({
                "submitted": true,
                "workflow_id": triggered_workflow_id,
                "execution_id": execution_id,
                "result": result,
            })),
            Err(err) => Err(err),
        }
    }

    async fn run_triggered_subworkflow(
        ctx: &TriggerContext,
        run: TriggeredSubworkflowRun,
    ) -> WorkflowResult<Value> {
        let triggered_workflow_id = run.triggered_workflow_id;
        let variables = Arc::new(DashMap::new());
        variable_mapping::inherit_all_variables(&ctx.variables, &variables);
        for (key, value) in &run.input_mapping {
            variables.insert(key.clone(), value.clone());
        }

        let options = WorkflowExecutionOptions {
            input: Some(Value::Object(
                run.input_mapping.clone().into_iter().collect(),
            )),
            max_steps: None,
            timeout: None,
            max_execution_time: (run.timeout > 0).then_some(run.timeout),
            enable_checkpoints: Some(false),
            node_timeout: None,
            max_pause_duration: None,
            retry_budget: None,
            on_failure: None,
            max_retries: None,
            retry_delay_ms: None,
            exponential_backoff: None,
            fallback_output: None,
            max_navigation_multiplier: None,
        };

        let execution_id = wf_common::generate_id();
        let sub_workflow_id = wf_common::generate_id();
        let entity = WorkflowExecutionEntity::new(execution_id.clone(), sub_workflow_id.clone());
        let mut exec_ctx = ExecutorContext::new(
            execution_id,
            sub_workflow_id,
            ctx.event_bus.clone(),
            run.tool_registry,
            options,
        )
        .with_parent_execution(ctx.execution_id.clone());
        if let Some(metrics) = &ctx.metrics {
            exec_ctx = exec_ctx.with_metrics(metrics.clone());
        }
        exec_ctx.variables = variables.clone();

        let mut coordinator = match WorkflowCoordinator::new(exec_ctx, run.graph, run.handlers) {
            Ok(coordinator) => coordinator.with_entity(entity),
            Err(err) => {
                Self::emit(
                    ctx,
                    EventType::TriggeredSubgraphFailed,
                    &format!("triggered_subworkflow_failed:{}", triggered_workflow_id),
                )
                .await;
                return Err(err);
            }
        };

        match coordinator.execute().await {
            Ok(output) => {
                Self::apply_subworkflow_output_mapping(
                    &run.output_mapping,
                    &variables,
                    &output,
                    &ctx.variables,
                );
                ctx.variables.insert(
                    trigger_internal::SUBWORKFLOW_RESULT.to_string(),
                    output.clone(),
                );
                if let Some(bus) = &ctx.signal_bus {
                    trigger_internal::publish_subworkflow_result(
                        bus,
                        ctx.execution_id.clone(),
                        ctx.execution_id.clone(),
                        output.clone(),
                    );
                }
                Self::emit(
                    ctx,
                    EventType::TriggeredSubgraphCompleted,
                    &format!("triggered_subworkflow_completed:{}", triggered_workflow_id),
                )
                .await;
                Ok(output)
            }
            Err(err) => {
                Self::emit(
                    ctx,
                    EventType::TriggeredSubgraphFailed,
                    &format!("triggered_subworkflow_failed:{}", triggered_workflow_id),
                )
                .await;
                Err(err)
            }
        }
    }

    fn apply_subworkflow_output_mapping(
        output_mapping: &HashMap<String, Value>,
        sub_variables: &DashMap<String, Value>,
        result: &Value,
        parent_variables: &DashMap<String, Value>,
    ) {
        for (target, source) in output_mapping {
            let value = match source {
                Value::String(name) => sub_variables
                    .get(name)
                    .map(|entry| entry.value().clone())
                    .unwrap_or_else(|| result.clone()),
                _ => source.clone(),
            };
            parent_variables.insert(target.clone(), value);
        }
    }

    async fn handle_execute_script(
        action: &TriggerAction,
        ctx: &TriggerContext,
    ) -> WorkflowResult<Value> {
        let (script_name, parameters, timeout, ignore_error) = match action {
            TriggerAction::ExecuteScript {
                script_name,
                parameters,
                timeout,
                ignore_error,
            } => (
                script_name.clone(),
                parameters.clone(),
                timeout.unwrap_or(0),
                ignore_error.unwrap_or(false),
            ),
            _ => return Err(WorkflowError::Internal("Invalid action type".to_string())),
        };

        let script = match &ctx.script_registry {
            Some(registry) => registry.get(&script_name),
            None => lookup_script(&script_name),
        }
        .ok_or_else(|| {
            WorkflowError::TriggerError(format!(
                "Script '{}' not found in script registry",
                script_name
            ))
        })?;

        Self::emit(
            ctx,
            EventType::ScriptStarted,
            &format!("trigger_script:{}", script_name),
        )
        .await;

        let mut code = String::new();
        if let Some(params) = parameters {
            let serialized = serde_json::to_string(&params).unwrap_or_else(|_| "null".to_string());
            code.push_str(&format!("const parameters = {};\n", serialized));
        }
        code.push_str(&script.code);

        let sandbox_config = SandboxConfig {
            mode: Some(wf_types::script::sandbox::SandboxMode::Strict),
            policy: None,
            shell_strategy: None,
            python_strategy: None,
            javascript_strategy: None,
            lua_strategy: None,
            vfs: None,
            workdir: None,
            env: None,
            legacy_type: None,
            resource_limits: None,
            skip_gate_check: None,
        };
        let runner = ctx
            .script_runner
            .clone()
            .unwrap_or_else(|| Arc::new(SandboxScriptRunner::new()) as Arc<dyn ScriptRunner>);
        let execution = runner.execute(&script.language, &code, &sandbox_config);

        let execution_result = if timeout > 0 {
            match tokio::time::timeout(std::time::Duration::from_millis(timeout), execution).await {
                Ok(result) => result,
                Err(_) => {
                    Self::emit(
                        ctx,
                        EventType::ScriptFailed,
                        &format!("trigger_script_failed:{}", script_name),
                    )
                    .await;
                    return Err(WorkflowError::TriggerError(format!(
                        "Script '{}' timed out after {}ms",
                        script_name, timeout
                    )));
                }
            }
        } else {
            execution.await
        };

        if !execution_result.success {
            let stderr = execution_result
                .stderr
                .as_deref()
                .unwrap_or("unknown error")
                .to_string();
            if ignore_error {
                let result = serde_json::json!({
                    "success": false,
                    "error": stderr,
                    "script_name": script_name,
                    "execution_time": execution_result.execution_time,
                });
                ctx.variables
                    .insert(trigger_internal::SCRIPT_RESULT.to_string(), result.clone());
                Self::emit(
                    ctx,
                    EventType::ScriptCompleted,
                    &format!("trigger_script_completed:{}", script_name),
                )
                .await;
                return Ok(result);
            }
            Self::emit(
                ctx,
                EventType::ScriptFailed,
                &format!("trigger_script_failed:{}", script_name),
            )
            .await;
            return Err(WorkflowError::TriggerError(format!(
                "Script '{}' execution failed: {}",
                script_name, stderr
            )));
        }

        let output = execution_result
            .stdout
            .clone()
            .map(Value::String)
            .unwrap_or(Value::Null);
        let parsed = execution_result
            .stdout
            .as_deref()
            .and_then(|s| serde_json::from_str::<Value>(s).ok())
            .unwrap_or(output);

        ctx.variables
            .insert(trigger_internal::SCRIPT_RESULT.to_string(), parsed.clone());
        if let Some(bus) = &ctx.signal_bus {
            trigger_internal::publish_script_result(
                bus,
                ctx.execution_id.clone(),
                ctx.execution_id.clone(),
                parsed.clone(),
            );
        }

        Self::emit(
            ctx,
            EventType::ScriptCompleted,
            &format!("trigger_script_completed:{}", script_name),
        )
        .await;

        Ok(serde_json::json!({
            "success": true,
            "result": parsed,
            "script_name": script_name,
            "execution_time": execution_result.execution_time,
        }))
    }

    async fn emit(ctx: &TriggerContext, event_type: EventType, message: &str) {
        Self::emit_with_metadata(ctx, event_type, message, &[]).await;
    }

    async fn emit_with_metadata(
        ctx: &TriggerContext,
        event_type: EventType,
        message: &str,
        extra: &[(&str, Value)],
    ) {
        match &ctx.event_bus {
            Some(bus) => {
                let mut metadata = std::collections::HashMap::from([(
                    "trigger_message".to_string(),
                    Value::String(message.to_string()),
                )]);
                for (key, value) in extra {
                    metadata.insert(key.to_string(), value.clone());
                }
                bus.publish_logged(
                    BaseEvent {
                        id: wf_common::generate_id(),
                        r#type: event_type,
                        timestamp: wf_common::now(),
                        workflow_id: Some(ctx.workflow_id.clone()),
                        execution_id: Some(ctx.execution_id.clone()),
                        agent_loop_id: None,

                        event_name: None,
                        metadata: Some(metadata),
                    },
                    &format!("workflow={} trigger", ctx.execution_id),
                )
                .ok();
            }
            None => {
                tracing::debug!(
                    execution_id = %ctx.execution_id,
                    ?event_type,
                    "no event bus, skipping trigger event"
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::Duration;
    use wf_types::workflow::EdgeType;
    use wf_types::workflow_execution::{WorkflowEdge, WorkflowGraphStructure, WorkflowNode};

    use crate::handler::HandlerRegistry;
    use crate::register_graph;
    use crate::ScriptRegistry;

    fn node(id: &str, node_type: &str, inner: serde_json::Value) -> WorkflowNode {
        WorkflowNode {
            id: id.to_string(),
            name: Some(id.to_string()),
            node_type: node_type.to_string(),
            inner,
        }
    }

    fn edge(source: &str, target: &str) -> WorkflowEdge {
        WorkflowEdge {
            id: format!("{}-{}", source, target),
            source_node_id: source.to_string(),
            target_node_id: target.to_string(),
            r#type: EdgeType::Default,
            condition: None,
            label: None,
            description: None,
        }
    }

    fn build_graph(nodes: Vec<WorkflowNode>, edges: Vec<WorkflowEdge>) -> WorkflowGraphStructure {
        WorkflowGraphStructure {
            nodes,
            edges,
            adjacency_list: HashMap::new(),
            reverse_adjacency_list: HashMap::new(),
            start_node_id: Some("start".to_string()),
            end_node_ids: vec!["end".to_string()],
        }
    }

    /// Hermetic script runner for trigger-script unit tests: no real
    /// interpreter subprocess, no load sensitivity.
    struct MockScriptRunner {
        stdout: Option<String>,
        stderr: Option<String>,
        success: bool,
        delay_ms: u64,
        calls: Arc<AtomicU32>,
    }

    #[async_trait]
    impl ScriptRunner for MockScriptRunner {
        async fn execute(
            &self,
            _language: &str,
            _code: &str,
            _config: &SandboxConfig,
        ) -> ScriptExecutionResult {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if self.delay_ms > 0 {
                tokio::time::sleep(Duration::from_millis(self.delay_ms)).await;
            }
            ScriptExecutionResult {
                success: self.success,
                script_name: "mock".to_string(),
                stdout: self.stdout.clone(),
                stderr: self.stderr.clone(),
                exit_code: Some(if self.success { 0 } else { 1 }),
                execution_time: 0,
                error: if self.success {
                    None
                } else {
                    self.stderr.clone().or(Some("mock failure".to_string()))
                },
                sandbox_mode: Some("Strict".to_string()),
                strategy_id: Some("mock".to_string()),
                violations: None,
            }
        }
    }

    fn mock_runner(stdout: Option<&str>, success: bool) -> Arc<dyn ScriptRunner> {
        Arc::new(MockScriptRunner {
            stdout: stdout.map(|s| s.to_string()),
            stderr: if success {
                None
            } else {
                Some("mock failure".to_string())
            },
            success,
            delay_ms: 0,
            calls: Arc::new(AtomicU32::new(0)),
        })
    }

    fn script_context(
        registry: &Arc<ScriptRegistry>,
        runner: Arc<dyn ScriptRunner>,
    ) -> TriggerContext {
        TriggerContext::new(Id::new(), Id::new())
            .with_script_registry(registry.clone())
            .with_script_runner(runner)
    }

    #[tokio::test]
    async fn test_trigger_execute_script() {
        let registry = Arc::new(ScriptRegistry::new());
        registry.register_script(
            "hello",
            "javascript",
            "console.log(JSON.stringify({greeting: 'Hello, ' + parameters.name}));",
        );
        let ctx = script_context(
            &registry,
            mock_runner(Some("{\"greeting\":\"Hello, world\"}"), true),
        );

        let result = TriggerCoordinator::execute(
            &TriggerAction::ExecuteScript {
                script_name: "hello".to_string(),
                parameters: Some(serde_json::json!({"name": "world"})),
                timeout: Some(5000),
                ignore_error: Some(false),
            },
            "t1",
            &ctx,
        )
        .await;

        assert!(result.success, "script should succeed: {:?}", result.error);
        let value = result.result.unwrap();
        assert_eq!(value["success"], serde_json::json!(true));
        assert_eq!(
            value["result"]["greeting"],
            serde_json::json!("Hello, world")
        );
        let stored = ctx
            .variables
            .get(trigger_internal::SCRIPT_RESULT)
            .expect("result should be stored")
            .value()
            .clone();
        assert_eq!(stored["greeting"], serde_json::json!("Hello, world"));
    }

    #[tokio::test]
    async fn test_trigger_execute_script_missing() {
        let registry = Arc::new(ScriptRegistry::new());
        let ctx = script_context(&registry, mock_runner(None, true));
        let result = TriggerCoordinator::execute(
            &TriggerAction::ExecuteScript {
                script_name: "not_registered".to_string(),
                parameters: None,
                timeout: None,
                ignore_error: None,
            },
            "t1",
            &ctx,
        )
        .await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("not found"));
    }

    #[tokio::test]
    async fn test_trigger_execute_script_ignore_error() {
        let registry = Arc::new(ScriptRegistry::new());
        registry.register_script("boom", "javascript", "throw new Error('kaboom');");
        let ctx = script_context(&registry, mock_runner(None, false));
        let result = TriggerCoordinator::execute(
            &TriggerAction::ExecuteScript {
                script_name: "boom".to_string(),
                parameters: None,
                timeout: Some(5000),
                ignore_error: Some(true),
            },
            "t1",
            &ctx,
        )
        .await;
        assert!(result.success, "ignore_error should swallow failures");
        assert_eq!(result.result.unwrap()["success"], serde_json::json!(false));
    }

    #[tokio::test]
    async fn test_trigger_execute_script_timeout() {
        let registry = Arc::new(ScriptRegistry::new());
        registry.register_script("slow", "javascript", "while (true) {}");
        let runner: Arc<dyn ScriptRunner> = Arc::new(MockScriptRunner {
            stdout: None,
            stderr: None,
            success: true,
            delay_ms: 10_000,
            calls: Arc::new(AtomicU32::new(0)),
        });
        let ctx = script_context(&registry, runner);
        let result = TriggerCoordinator::execute(
            &TriggerAction::ExecuteScript {
                script_name: "slow".to_string(),
                parameters: None,
                timeout: Some(50),
                ignore_error: Some(false),
            },
            "t1",
            &ctx,
        )
        .await;
        assert!(!result.success, "timeout must fail the trigger");
        assert!(result.error.unwrap().contains("timed out after 50ms"));
    }

    #[tokio::test]
    async fn test_trigger_events_use_dedicated_types() {
        let bus = Arc::new(wf_core::EventBus::new(16));
        let mut sub = bus.subscribe();
        let ctx = TriggerContext::new(Id::new(), Id::new()).with_event_bus(bus);

        let skip = TriggerCoordinator::execute(
            &TriggerAction::SkipNode {
                node_id: Some("n-42".to_string()),
            },
            "t1",
            &ctx,
        )
        .await;
        assert!(skip.success);

        let notify = TriggerCoordinator::execute(
            &TriggerAction::SendNotification {
                message: "hello".to_string(),
            },
            "t1",
            &ctx,
        )
        .await;
        assert!(notify.success);

        let first = sub.recv().await.unwrap();
        assert_eq!(first.r#type, EventType::NodeSkipped);
        assert_eq!(
            first.metadata.as_ref().unwrap().get("node_id"),
            Some(&serde_json::json!("n-42"))
        );

        let second = sub.recv().await.unwrap();
        assert_eq!(second.r#type, EventType::NotificationSent);
        assert_eq!(
            second.metadata.as_ref().unwrap().get("message"),
            Some(&serde_json::json!("hello"))
        );
    }

    #[tokio::test]
    async fn test_trigger_execute_subworkflow() {
        let child = build_graph(
            vec![
                node("start", "START", serde_json::json!({})),
                node(
                    "set",
                    "VARIABLE",
                    serde_json::json!({"variable_name": "sum", "expression": "7"}),
                ),
                node("end", "END", serde_json::json!({})),
            ],
            vec![edge("start", "set"), edge("set", "end")],
        );
        register_graph("child_flow", child);

        let handlers = {
            let mut reg = HandlerRegistry::new();
            reg.register_defaults(std::sync::Arc::new(wf_llm::LlmGateway::new()));
            reg.into_arc()
        };
        let ctx = TriggerContext::new(Id::new(), Id::new()).with_handlers(handlers);

        let mut output_mapping = HashMap::new();
        output_mapping.insert("mapped_sum".to_string(), serde_json::json!("sum"));
        let mut input_mapping = HashMap::new();
        input_mapping.insert("a".to_string(), serde_json::json!(1));

        let result = TriggerCoordinator::execute(
            &TriggerAction::ExecuteTriggeredSubworkflow {
                triggered_workflow_id: "child_flow".to_string(),
                wait_for_completion: Some(true),
                timeout: Some(5000),
                input_mapping: Some(input_mapping),
                output_mapping: Some(output_mapping),
            },
            "t1",
            &ctx,
        )
        .await;

        assert!(result.success, "subworkflow should run: {:?}", result.error);
        let value = result.result.unwrap();
        assert_eq!(value["submitted"], serde_json::json!(true));
        let stored = ctx
            .variables
            .get(trigger_internal::SUBWORKFLOW_RESULT)
            .expect("result should be stored")
            .value()
            .clone();
        assert_eq!(stored, value["result"]);
        let mapped = ctx
            .variables
            .get("mapped_sum")
            .expect("output mapping should write back")
            .value()
            .clone();
        assert_eq!(mapped, serde_json::json!("7"));
    }

    #[tokio::test]
    async fn test_trigger_execute_subworkflow_missing() {
        let ctx = TriggerContext::new(Id::new(), Id::new());
        let result = TriggerCoordinator::execute(
            &TriggerAction::ExecuteTriggeredSubworkflow {
                triggered_workflow_id: "ghost_flow".to_string(),
                wait_for_completion: Some(true),
                timeout: None,
                input_mapping: None,
                output_mapping: None,
            },
            "t1",
            &ctx,
        )
        .await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("not found"));
    }

    #[tokio::test]
    async fn test_set_variable_rejects_reserved_prefix() {
        let ctx = TriggerContext::new(Id::new(), Id::new());
        let result = TriggerCoordinator::execute(
            &TriggerAction::SetVariable {
                variable_name: "__msg_ctx__chat".to_string(),
                value: serde_json::json!([{"role": "user", "content": "hi"}]),
            },
            "t1",
            &ctx,
        )
        .await;
        assert!(!result.success, "reserved-prefix write must be rejected");
        let error = result.error.unwrap();
        assert!(error.contains("reserved '__' prefix"), "error: {error}");
        assert!(
            !ctx.variables.contains_key("__msg_ctx__chat"),
            "rejected write must not touch the variable map"
        );
    }

    fn msg(role: wf_types::message::MessageRole, text: &str) -> wf_types::message::Message {
        wf_types::message::Message {
            id: Id::new(),
            role,
            content: wf_types::message::MessageContentValue::Text(text.to_string()),
            timestamp: wf_common::now(),
            tool_call_id: None,
            tool_name: None,
            tool_calls: None,
            thinking: None,
            metadata: None,
        }
    }

    #[tokio::test]
    async fn test_set_message_context_writes_context_and_ledger() {
        let ctx = TriggerContext::new(Id::new(), Id::new());
        let result = TriggerCoordinator::execute(
            &TriggerAction::SetMessageContext {
                context_id: "chat".to_string(),
                messages: vec![
                    msg(wf_types::message::MessageRole::User, "hello"),
                    msg(wf_types::message::MessageRole::Assistant, "hi"),
                ],
            },
            "t1",
            &ctx,
        )
        .await;
        assert!(result.success, "set must succeed: {:?}", result.error);
        assert_eq!(
            result.result.unwrap()["message_count"],
            serde_json::json!(2)
        );

        let stored = crate::message_context::get_context(&ctx.variables, "chat");
        assert_eq!(stored.len(), 2);
        assert_eq!(stored[0].role, wf_types::message::MessageRole::User);

        // Ledger consistency: replacement marks the entry dirty; the read
        // above recomputed it, so count and estimate now match the array.
        assert_eq!(
            crate::message_context::ledger_message_count(&ctx.variables, "chat"),
            2
        );
        assert!(
            crate::message_context::ledger_estimated_tokens(&ctx.variables, "chat") > 0,
            "estimate must be recomputed after replacement"
        );
        assert_eq!(
            crate::message_context::array_version(&ctx.variables, "chat"),
            1
        );
    }

    #[tokio::test]
    async fn test_append_message_context_accumulates_in_ledger() {
        let ctx = TriggerContext::new(Id::new(), Id::new());
        let first = TriggerCoordinator::execute(
            &TriggerAction::AppendMessageContext {
                context_id: "chat".to_string(),
                messages: vec![msg(wf_types::message::MessageRole::User, "one")],
            },
            "t1",
            &ctx,
        )
        .await;
        assert!(
            first.success,
            "first append must succeed: {:?}",
            first.error
        );
        assert_eq!(first.result.unwrap()["appended"], serde_json::json!(1));

        let second = TriggerCoordinator::execute(
            &TriggerAction::AppendMessageContext {
                context_id: "chat".to_string(),
                messages: vec![
                    msg(wf_types::message::MessageRole::User, "two"),
                    msg(wf_types::message::MessageRole::Assistant, "three"),
                ],
            },
            "t1",
            &ctx,
        )
        .await;
        assert!(
            second.success,
            "second append must succeed: {:?}",
            second.error
        );

        let stored = crate::message_context::get_context(&ctx.variables, "chat");
        assert_eq!(stored.len(), 3);
        assert_eq!(
            stored[0].content,
            wf_types::message::MessageContentValue::Text("one".into())
        );
        assert_eq!(
            crate::message_context::ledger_message_count(&ctx.variables, "chat"),
            3
        );
        assert_eq!(
            crate::message_context::array_version(&ctx.variables, "chat"),
            2
        );
    }

    #[tokio::test]
    async fn test_message_context_actions_emit_updated_event() {
        let bus = Arc::new(wf_core::EventBus::new(16));
        let mut sub = bus.subscribe();
        let ctx = TriggerContext::new(Id::new(), Id::new()).with_event_bus(bus);

        let set = TriggerCoordinator::execute(
            &TriggerAction::SetMessageContext {
                context_id: "chat".to_string(),
                messages: vec![msg(wf_types::message::MessageRole::User, "hello")],
            },
            "t1",
            &ctx,
        )
        .await;
        assert!(set.success, "set must succeed: {:?}", set.error);

        let append = TriggerCoordinator::execute(
            &TriggerAction::AppendMessageContext {
                context_id: "chat".to_string(),
                messages: vec![msg(wf_types::message::MessageRole::Assistant, "hi")],
            },
            "t1",
            &ctx,
        )
        .await;
        assert!(append.success, "append must succeed: {:?}", append.error);

        let set_event = sub.recv().await.unwrap();
        assert_eq!(set_event.r#type, EventType::MessageContextUpdated);
        let meta = set_event.metadata.as_ref().unwrap();
        assert_eq!(meta.get("context_id"), Some(&serde_json::json!("chat")));
        assert_eq!(meta.get("message_count"), Some(&serde_json::json!(1)));

        let append_event = sub.recv().await.unwrap();
        assert_eq!(append_event.r#type, EventType::MessageContextUpdated);
        let meta = append_event.metadata.as_ref().unwrap();
        assert_eq!(meta.get("context_id"), Some(&serde_json::json!("chat")));
        assert_eq!(meta.get("message_count"), Some(&serde_json::json!(1)));
        assert_eq!(
            meta.get("trigger_message"),
            Some(&serde_json::json!("message_context_appended:chat"))
        );

        assert_eq!(
            EventType::MessageContextUpdated.as_str(),
            "MESSAGE_CONTEXT_UPDATED"
        );
    }
}
