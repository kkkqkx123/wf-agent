use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use dashmap::DashMap;
use serde_json::Value;
use wf_core::EventBus;
use wf_execution_shared::context::{NodeExecutionContext, NodeExecutionResult};
use wf_metrics::MetricsRegistry;
use wf_sandbox::SandboxRuntime;
use wf_types::events::{BaseEvent, EventType};
use wf_types::message::Message;
use wf_types::node::StaticNodeType;
use wf_types::trigger::{TriggerAction, TriggerExecutionResult};
use wf_types::Id;

use crate::coordinator::WorkflowCoordinator;
use crate::error::{WorkflowError, WorkflowResult};
use crate::handler::NodeHandler;
use crate::handler::{variable_mapping, HandlerRegistry};
use crate::message_context;
use crate::registry::{lookup_graph, lookup_script, ScriptRegistry};
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
    pub handlers: Option<Arc<HashMap<StaticNodeType, Arc<dyn NodeHandler>>>>,
    pub tool_registry: Option<Arc<ToolRegistry>>,
    pub metrics: Option<Arc<MetricsRegistry>>,
    pub script_runner: Option<Arc<dyn ScriptRunner>>,
    pub script_registry: Option<Arc<ScriptRegistry>>,
}

impl TriggerContext {
    pub fn new(execution_id: Id, workflow_id: Id) -> Self {
        Self {
            execution_id,
            workflow_id,
            variables: Arc::new(DashMap::new()),
            event_bus: None,
            handlers: None,
            tool_registry: None,
            metrics: None,
            script_runner: None,
            script_registry: None,
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

    pub fn with_handlers(
        mut self,
        handlers: Arc<HashMap<StaticNodeType, Arc<dyn NodeHandler>>>,
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
}

pub struct TriggerCoordinator;

/// Everything needed to run a triggered sub-workflow.
struct TriggeredSubworkflowRun {
    triggered_workflow_id: String,
    graph: wf_types::workflow_execution::WorkflowGraphStructure,
    handlers: Arc<HashMap<StaticNodeType, Arc<dyn NodeHandler>>>,
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
        ctx.variables
            .insert("__trigger_stop".to_string(), Value::Bool(true));
        Self::emit(
            ctx,
            EventType::ExecutionStopped,
            "workflow_stopped_by_trigger",
        )
        .await;
        Ok(Value::String("workflow_stopped".to_string()))
    }

    async fn handle_pause_workflow(ctx: &TriggerContext) -> WorkflowResult<Value> {
        ctx.variables
            .insert("__trigger_pause".to_string(), Value::Bool(true));
        Self::emit(
            ctx,
            EventType::WorkflowExecutionPaused,
            "workflow_paused_by_trigger",
        )
        .await;
        Ok(Value::String("workflow_paused".to_string()))
    }

    async fn handle_resume_workflow(ctx: &TriggerContext) -> WorkflowResult<Value> {
        ctx.variables.remove("__trigger_pause");
        Self::emit(
            ctx,
            EventType::WorkflowExecutionResumed,
            "workflow_resumed_by_trigger",
        )
        .await;
        Ok(Value::String("workflow_resumed".to_string()))
    }

    async fn handle_skip_node(node_id: &str, ctx: &TriggerContext) -> WorkflowResult<Value> {
        ctx.variables
            .insert(format!("__skipped_{}", node_id), Value::Bool(true));
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
            let execution_id = Id::new();
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
                let _ = Self::run_triggered_subworkflow(&tctx, run).await;
            });
            return Ok(serde_json::json!({
                "submitted": true,
                "workflow_id": triggered_workflow_id,
                "execution_id": exec_id,
            }));
        }

        let execution_id = Id::new();
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
        };

        let execution_id = Id::new();
        let sub_workflow_id = Id::new();
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
                ctx.variables
                    .insert("__trigger_subworkflow_result".to_string(), output.clone());
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
                    .insert("__trigger_script_result".to_string(), result.clone());
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
            .insert("__trigger_script_result".to_string(), parsed.clone());

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
        if let Some(ref bus) = ctx.event_bus {
            let mut metadata = std::collections::HashMap::from([(
                "trigger_message".to_string(),
                Value::String(message.to_string()),
            )]);
            for (key, value) in extra {
                metadata.insert(key.to_string(), value.clone());
            }
            let event = BaseEvent {
                id: wf_common::generate_id(),
                r#type: event_type,
                timestamp: wf_common::now(),
                workflow_id: Some(ctx.workflow_id.clone()),
                execution_id: Some(ctx.execution_id.clone()),
                agent_loop_id: None,
                metadata: Some(metadata),
            };
            let _ = bus.publish(event);
        }
    }
}

/// Parse a TriggerAction from a node config value.
///
/// Accepted layouts:
/// - `config.trigger` / `config.action` holding the tagged action object
/// - the config itself holding the tagged action object
pub fn parse_trigger_action(config: &Value) -> Option<TriggerAction> {
    let candidate = config
        .get("trigger")
        .or_else(|| config.get("action"))
        .unwrap_or(config);
    candidate.get("action_type")?;
    serde_json::from_value(candidate.clone()).ok()
}

/// Build a TriggerContext from a NodeExecutionContext.
fn build_trigger_context(ctx: &NodeExecutionContext) -> TriggerContext {
    let mut tctx = TriggerContext::new(
        ctx.execution_id.clone(),
        wf_types::Id::from(ctx.execution_id.clone()),
    )
    .with_variables(ctx.variables.clone());
    if let Some(bus) = &ctx.event_bus {
        tctx = tctx.with_event_bus(bus.clone());
    }
    if let Some(handlers) = &ctx.handler_registry {
        if let Some(handlers) =
            handlers.downcast_ref::<Arc<HashMap<StaticNodeType, Arc<dyn NodeHandler>>>>()
        {
            tctx = tctx.with_handlers(handlers.clone());
        }
    }
    if let Some(registry) = &ctx.tool_registry {
        tctx = tctx.with_tool_registry(registry.clone());
    }
    if let Some(metrics) = &ctx.metrics {
        tctx = tctx.with_metrics(metrics.clone());
    }
    tctx
}

/// Execute the trigger action and return the next node ids to navigate to.
async fn execute_action(
    action: &TriggerAction,
    ctx: &mut NodeExecutionContext,
) -> WorkflowResult<Vec<String>> {
    let tctx = build_trigger_context(ctx);
    let result = TriggerCoordinator::execute(action, "node_trigger", &tctx).await;
    if let Some(err) = &result.error {
        return Err(WorkflowError::TriggerError(err.clone()));
    }

    if let TriggerAction::SkipNode { node_id } = action {
        if let (Some(target), Some(graph)) = (
            node_id,
            ctx.graph_structure.as_ref().and_then(|any| {
                any.downcast_ref::<wf_types::workflow_execution::WorkflowGraphStructure>()
            }),
        ) {
            let next = graph
                .edges
                .iter()
                .find(|e| e.source_node_id == *target)
                .map(|e| e.target_node_id.clone())
                .or_else(|| Some(target.clone()));
            if let Some(next) = next {
                return Ok(vec![next]);
            }
        }
    }
    Ok(Vec::new())
}

/// Map `message_inputs` (or camelCase `messageInputs`) entries from the node
/// input object into workflow variables.
fn map_message_inputs(ctx: &mut NodeExecutionContext) -> WorkflowResult<()> {
    let Some(config) = &ctx.node_config else {
        return Ok(());
    };
    let Some(inputs) = config
        .get("message_inputs")
        .or_else(|| config.get("messageInputs"))
        .and_then(|v| v.as_array())
    else {
        return Ok(());
    };

    let input_obj = ctx.input.as_object().cloned().unwrap_or_default();
    for entry in inputs {
        let source = entry
            .get("source_context_id")
            .or_else(|| entry.get("sourceContextId"))
            .and_then(|v| v.as_str());
        let internal = entry
            .get("internal_name")
            .or_else(|| entry.get("internalName"))
            .and_then(|v| v.as_str());
        let required = entry
            .get("required")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let (Some(source), Some(internal)) = (source, internal) else {
            continue;
        };

        match input_obj.get(source) {
            Some(value) => {
                // Message inputs are registered as named message contexts
                // (not plain variables): LLM / downstream nodes read them via
                // `context_id`, mirroring the TS message-array semantics.
                if let Ok(messages) = serde_json::from_value::<Vec<Message>>(value.clone()) {
                    message_context::register_context(&ctx.variables, internal, messages);
                } else {
                    ctx.set_variable(internal, value.clone());
                }
            }
            None if required => {
                return Err(WorkflowError::TriggerError(format!(
                    "Required trigger input '{}' (mapped to '{}') is missing",
                    source, internal
                )));
            }
            None => {}
        }
    }
    Ok(())
}

/// Export `variable_outputs` (or camelCase) entries: copy the named values
/// from the node input object into workflow variables.
fn export_variable_outputs(ctx: &mut NodeExecutionContext) {
    let Some(config) = &ctx.node_config else {
        return;
    };
    let Some(outputs) = config
        .get("variable_outputs")
        .or_else(|| config.get("variableOutputs"))
        .and_then(|v| v.as_array())
    else {
        return;
    };

    let input_obj = ctx.input.as_object().cloned().unwrap_or_default();
    for entry in outputs {
        let internal = entry
            .get("internal_name")
            .or_else(|| entry.get("internalName"))
            .and_then(|v| v.as_str());
        let target = entry
            .get("target_variable")
            .or_else(|| entry.get("targetVariable"))
            .and_then(|v| v.as_str());
        if let (Some(internal), Some(target)) = (internal, target) {
            if let Some(value) = input_obj.get(internal) {
                ctx.set_variable(target, value.clone());
            }
        }
    }
}

/// Export `message_outputs` (or camelCase) entries: expose the named message
/// context as the node output (serialized `Vec<Message>`), so a triggered
/// sub-workflow's final output is the message array (e.g. the compressed
/// summary) that the event-driven trigger listener writes back. The first
/// entry with a non-empty context wins.
fn export_message_outputs(ctx: &mut NodeExecutionContext) -> Option<Value> {
    let config = ctx.node_config.as_ref()?;
    let outputs = config
        .get("message_outputs")
        .or_else(|| config.get("messageOutputs"))
        .and_then(|v| v.as_array())?;

    for entry in outputs {
        let internal = entry
            .get("internal_name")
            .or_else(|| entry.get("internalName"))
            .and_then(|v| v.as_str())?;
        let messages = message_context::get_context(&ctx.variables, internal);
        if messages.is_empty() {
            continue;
        }
        return serde_json::to_value(&messages).ok();
    }
    None
}

/// START_FROM_TRIGGER: initializes workflow state from trigger input and
/// optionally applies a trigger action from the node config.
pub struct StartFromTriggerHandler;

#[async_trait]
impl NodeHandler for StartFromTriggerHandler {
    fn node_type(&self) -> StaticNodeType {
        StaticNodeType::StartFromTrigger
    }

    async fn execute(&self, ctx: &mut NodeExecutionContext) -> WorkflowResult<NodeExecutionResult> {
        let already_executed = ctx
            .get_variable(&format!("__completed_{}", ctx.node_id))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if already_executed {
            return Ok(NodeExecutionResult::simple(ctx.input.clone()));
        }
        ctx.set_variable(format!("__completed_{}", ctx.node_id), Value::from(true));

        map_message_inputs(ctx)?;

        let next_node_ids = match ctx.node_config.as_ref().and_then(parse_trigger_action) {
            Some(action) => execute_action(&action, ctx).await?,
            None => Vec::new(),
        };

        Ok(NodeExecutionResult::with_next_nodes(
            ctx.input.clone(),
            next_node_ids,
        ))
    }
}

/// CONTINUE_FROM_TRIGGER: hands data back to the main workflow and applies
/// a trigger action from the node config when present.
pub struct ContinueFromTriggerHandler;

#[async_trait]
impl NodeHandler for ContinueFromTriggerHandler {
    fn node_type(&self) -> StaticNodeType {
        StaticNodeType::ContinueFromTrigger
    }

    async fn execute(&self, ctx: &mut NodeExecutionContext) -> WorkflowResult<NodeExecutionResult> {
        let already_executed = ctx
            .get_variable(&format!("__completed_{}", ctx.node_id))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if already_executed {
            return Ok(NodeExecutionResult::simple(ctx.input.clone()));
        }
        ctx.set_variable(format!("__completed_{}", ctx.node_id), Value::from(true));

        export_variable_outputs(ctx);
        let message_output = export_message_outputs(ctx);

        let next_node_ids = match ctx.node_config.as_ref().and_then(parse_trigger_action) {
            Some(action) => execute_action(&action, ctx).await?,
            None => Vec::new(),
        };

        // The node output is the exported message array when present (the
        // compression sub-workflow's final output), the node input otherwise.
        let output = message_output.unwrap_or_else(|| ctx.input.clone());
        Ok(NodeExecutionResult::with_next_nodes(output, next_node_ids))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::Duration;
    use wf_execution_shared::context::ExecutorContext;
    use wf_tools::registry::ToolRegistry;
    use wf_types::workflow::EdgeType;
    use wf_types::workflow_execution::{
        WorkflowEdge, WorkflowExecutionOptions, WorkflowGraphStructure, WorkflowNode,
    };

    use crate::coordinator::WorkflowCoordinator;
    use crate::entity::WorkflowExecutionEntity;
    use crate::handler::HandlerRegistry;
    use crate::register_graph;
    use crate::ScriptRegistry;

    fn options_with_input(input: serde_json::Value) -> WorkflowExecutionOptions {
        WorkflowExecutionOptions {
            input: Some(input),
            max_steps: None,
            timeout: None,
            max_execution_time: None,
            enable_checkpoints: Some(false),
            node_timeout: None,
            max_pause_duration: None,
            retry_budget: None,
            on_failure: None,
            max_retries: None,
            retry_delay_ms: None,
            exponential_backoff: None,
            fallback_output: None,
        }
    }

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

    async fn run(
        graph: WorkflowGraphStructure,
        options: WorkflowExecutionOptions,
    ) -> WorkflowResult<Value> {
        let handlers = {
            let mut reg = HandlerRegistry::new();
            reg.register_defaults(std::sync::Arc::new(wf_llm::LlmGateway::new()));
            reg.into_arc()
        };
        let exec_ctx = ExecutorContext::new(
            wf_common::generate_id(),
            wf_common::generate_id(),
            None,
            Arc::new(ToolRegistry::new()),
            options,
        );
        let entity = WorkflowExecutionEntity::new(
            exec_ctx.execution_id.clone(),
            exec_ctx.workflow_id.clone(),
        );
        let mut coordinator =
            WorkflowCoordinator::new(exec_ctx, graph, handlers)?.with_entity(entity);
        coordinator.execute().await
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

    #[test]
    fn test_parse_trigger_action() {
        let config = serde_json::json!({
            "trigger": {
                "action_type": "set_variable",
                "variable_name": "x",
                "value": 42
            }
        });
        let action = parse_trigger_action(&config).expect("action should parse");
        assert!(matches!(
            action,
            TriggerAction::SetVariable { ref variable_name, .. } if variable_name == "x"
        ));

        assert!(parse_trigger_action(&serde_json::json!({"id": "n1"})).is_none());
    }

    fn msg(role: wf_types::message::MessageRole, text: &str) -> wf_types::message::Message {
        wf_types::message::Message {
            id: wf_types::Id::new(),
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
    async fn continue_from_trigger_exports_message_outputs() {
        let vars = std::sync::Arc::new(DashMap::new());
        crate::message_context::append_context(
            &vars,
            "compressed",
            vec![msg(wf_types::message::MessageRole::Assistant, "summary")],
        );

        let mut ctx = NodeExecutionContext::new(
            wf_types::Id::new(),
            "llm-summary-end".to_string(),
            StaticNodeType::ContinueFromTrigger,
            Value::Null,
            vars,
        )
        .with_node_config(serde_json::json!({
            "messageOutputs": [{
                "internalName": "compressed",
                "targetContextId": "current"
            }]
        }));

        let handler = ContinueFromTriggerHandler;
        let result = handler.execute(&mut ctx).await.unwrap();
        let messages: Vec<wf_types::message::Message> =
            serde_json::from_value(result.output).unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(
            messages[0].content,
            wf_types::message::MessageContentValue::Text("summary".to_string())
        );
    }

    #[tokio::test]
    async fn continue_from_trigger_falls_back_to_input_without_message_outputs() {
        let vars = std::sync::Arc::new(DashMap::new());
        let mut ctx = NodeExecutionContext::new(
            wf_types::Id::new(),
            "end-node".to_string(),
            StaticNodeType::ContinueFromTrigger,
            Value::String("plain".to_string()),
            vars,
        )
        .with_node_config(serde_json::json!({}));

        let handler = ContinueFromTriggerHandler;
        let result = handler.execute(&mut ctx).await.unwrap();
        assert_eq!(result.output, Value::String("plain".to_string()));
    }

    #[tokio::test]
    async fn test_start_from_trigger_maps_message_inputs() {
        let graph = build_graph(
            vec![
                node("start", "START", serde_json::json!({})),
                node(
                    "trigger",
                    "START_FROM_TRIGGER",
                    serde_json::json!({
                        "message_inputs": [{
                            "source_context_id": "user_name",
                            "internal_name": "greeting",
                            "required": true
                        }]
                    }),
                ),
                node("end", "END", serde_json::json!({})),
            ],
            vec![edge("start", "trigger"), edge("trigger", "end")],
        );

        let result = run(
            graph.clone(),
            options_with_input(serde_json::json!({"user_name": "alice"})),
        )
        .await;
        assert!(result.is_ok(), "workflow should run: {:?}", result.err());

        let missing = run(graph, options_with_input(serde_json::json!({}))).await;
        assert!(missing.is_err(), "missing required input must fail");
    }

    #[tokio::test]
    async fn test_trigger_action_pause_and_stop() {
        let graph = build_graph(
            vec![
                node("start", "START", serde_json::json!({})),
                node(
                    "trigger",
                    "CONTINUE_FROM_TRIGGER",
                    serde_json::json!({
                        "trigger": { "action_type": "pause_workflow_execution" }
                    }),
                ),
                node("end", "END", serde_json::json!({})),
            ],
            vec![edge("start", "trigger"), edge("trigger", "end")],
        );

        let paused = run(graph, options_with_input(Value::Null)).await;
        assert!(paused.is_err());
        assert!(paused.unwrap_err().to_string().contains("paused"));

        let stop_graph = build_graph(
            vec![
                node("start", "START", serde_json::json!({})),
                node(
                    "trigger",
                    "CONTINUE_FROM_TRIGGER",
                    serde_json::json!({
                        "trigger": { "action_type": "stop_workflow_execution" }
                    }),
                ),
                node("end", "END", serde_json::json!({})),
            ],
            vec![edge("start", "trigger"), edge("trigger", "end")],
        );

        let stopped = run(stop_graph, options_with_input(Value::Null)).await;
        assert!(stopped.is_err());
        assert!(stopped.unwrap_err().to_string().contains("stopped"));
    }

    #[tokio::test]
    async fn test_trigger_execute_script() {
        let registry = Arc::new(ScriptRegistry::new());
        registry.register(
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
            .get("__trigger_script_result")
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
        registry.register("boom", "javascript", "throw new Error('kaboom');");
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
        registry.register("slow", "javascript", "while (true) {}");
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
            .get("__trigger_subworkflow_result")
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
}
