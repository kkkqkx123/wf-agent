//! User trigger template registry and the sub-workflow runners that execute
//! triggered actions (`TriggerAction::ExecuteTriggeredSubworkflow`).

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};
use wf_core::registry::Registry;
use wf_core::EventBus;
use wf_core::internal_signal::InternalSignalBus;
use wf_execution_shared::context::ExecutorContext;
use wf_llm::{ContextCompressionRequestedMeta, LlmGateway};
use wf_resource::registry::ResourceRegistries;
use wf_types::events::BaseEvent;
use wf_types::node::StaticNodeType;
use wf_types::trigger::{TriggerAction, TriggerTemplate};
use wf_types::workflow::WorkflowTemplate;
use wf_types::workflow_execution::{
    WorkflowEdge, WorkflowExecutionOptions, WorkflowGraphStructure, WorkflowNode,
};
use wf_workflow::error::{WorkflowError, WorkflowResult};
use wf_workflow::handler::NodeHandler;
use wf_workflow::trigger_listener::{
    SubworkflowRunner, TriggerActionRunner, TriggerTemplateRegistry,
};
use wf_workflow::{WorkflowCoordinator, WorkflowExecutionEntity};

use super::{
    handle_subworkflow_output, record_trigger_execution, ExecutionContextRegistry,
    TriggerExecutionRecorder, DEFAULT_TRIGGER_TIMEOUT_MS,
};

/// Trigger template registry backed by the wf-resource registrar.
pub struct ResourceTriggerRegistry {
    registries: Arc<ResourceRegistries>,
}

impl ResourceTriggerRegistry {
    pub fn new(registries: Arc<ResourceRegistries>) -> Self {
        Self { registries }
    }
}

impl TriggerTemplateRegistry for ResourceTriggerRegistry {
    fn templates(&self) -> Vec<TriggerTemplate> {
        self.registries
            .trigger_templates
            .list()
            .iter()
            .filter_map(|key| {
                self.registries
                    .trigger_templates
                    .get(key)
                    .map(|template| template.as_ref().clone())
            })
            .collect()
    }
}

/// Convert a workflow template into an executable graph structure.
///
/// The predefined templates are flat (no subgraph expansion): nodes map
/// directly, the first node is the start (START_FROM_MESSAGE) and the last
/// node the end (CONTINUE_FROM_MESSAGE) of the summary workflow.
pub fn template_to_graph(template: &WorkflowTemplate) -> WorkflowGraphStructure {
    let nodes: Vec<WorkflowNode> = template
        .definition
        .nodes
        .iter()
        .map(|node| WorkflowNode {
            id: node.id.clone(),
            name: node.name.clone(),
            node_type: serde_json::to_string(&node.node_type)
                .map(|s| s.trim_matches('"').to_string())
                .unwrap_or_default(),
            inner: node.config.clone().unwrap_or(Value::Null),
        })
        .collect();
    let edges: Vec<WorkflowEdge> = template
        .definition
        .edges
        .iter()
        .map(|edge| WorkflowEdge {
            id: edge.id.clone(),
            source_node_id: edge.source_node_id.clone(),
            target_node_id: edge.target_node_id.clone(),
            r#type: edge.r#type.clone(),
            condition: edge.condition.clone(),
            label: edge.label.clone(),
            description: edge.description.clone(),
        })
        .collect();
    WorkflowGraphStructure {
        start_node_id: nodes.first().map(|node| node.id.clone()),
        end_node_ids: nodes
            .last()
            .map(|node| vec![node.id.clone()])
            .unwrap_or_default(),
        nodes,
        edges,
        adjacency_list: HashMap::new(),
        reverse_adjacency_list: HashMap::new(),
    }
}

/// Sub-workflow runner over the workflow coordinator.
pub struct WorkflowRunner {
    registries: Arc<ResourceRegistries>,
    event_bus: Arc<EventBus>,
    handlers: Arc<HashMap<StaticNodeType, Box<dyn NodeHandler>>>,
    /// Write-back registry of live workflow executions: every execution
    /// registers its variable map at start and unregisters at end.
    contexts: Arc<ExecutionContextRegistry>,
    /// Optional skill loader injected into builtin tool executors.
    skill_loader: Option<Arc<wf_tools::SkillLoader>>,
    /// Shared tool registry (builtin handlers + skills + MCP tools). When
    /// absent, a fresh registry is created per run.
    tool_registry: Option<Arc<wf_tools::registry::ToolRegistry>>,
    /// Typed signal bus for internal workflow/agent signals.
    signal_bus: Option<Arc<InternalSignalBus>>,
    /// Resolved resource limits (workflow caps and execution defaults)
    /// applied to every sub-workflow this runner starts. `None` keeps the
    /// engine's built-in defaults.
    limits: Option<wf_types::config::limits::LimitsConfig>,
}

impl WorkflowRunner {
    pub fn new(
        registries: Arc<ResourceRegistries>,
        event_bus: Arc<EventBus>,
        gateway: Arc<LlmGateway>,
        contexts: Arc<ExecutionContextRegistry>,
    ) -> Self {
        Self::with_skill_loader(registries, event_bus, gateway, contexts, None)
    }

    pub fn with_skill_loader(
        registries: Arc<ResourceRegistries>,
        event_bus: Arc<EventBus>,
        gateway: Arc<LlmGateway>,
        contexts: Arc<ExecutionContextRegistry>,
        skill_loader: Option<Arc<wf_tools::SkillLoader>>,
    ) -> Self {
        Self {
            registries,
            event_bus,
            handlers: wf_workflow::create_default_handlers(gateway, None),
            contexts,
            skill_loader,
            tool_registry: None,
            signal_bus: None,
            limits: None,
        }
    }

    /// Inject the typed signal bus: control signals from trigger actions
    /// reach the executed sub-workflow's coordinator through it.
    pub fn with_signal_bus(mut self, bus: Arc<InternalSignalBus>) -> Self {
        self.signal_bus = Some(bus);
        self
    }

    /// Inject resolved resource limits: workflow iteration caps, navigation
    /// multiplier and execution defaults (node timeout / max execution
    /// time) applied to every sub-workflow started here.
    pub fn with_limits(mut self, limits: wf_types::config::limits::LimitsConfig) -> Self {
        self.limits = Some(limits);
        self
    }

    /// Like [`WorkflowRunner::with_skill_loader`], but uses a caller-provided
    /// shared tool registry (skills and MCP tools pre-wired) for every run.
    pub fn with_tool_registry(
        registries: Arc<ResourceRegistries>,
        event_bus: Arc<EventBus>,
        gateway: Arc<LlmGateway>,
        contexts: Arc<ExecutionContextRegistry>,
        tool_registry: Option<Arc<wf_tools::registry::ToolRegistry>>,
        sandbox: Option<Arc<wf_sandbox::SandboxRuntime>>,
    ) -> Self {
        Self {
            registries,
            event_bus,
            handlers: wf_workflow::create_default_handlers(gateway, sandbox),
            contexts,
            skill_loader: None,
            tool_registry,
            signal_bus: None,
            limits: None,
        }
    }
}

#[async_trait]
impl SubworkflowRunner for WorkflowRunner {
    async fn run(&self, workflow_id: &str, input: Value) -> WorkflowResult<Value> {
        let template = self
            .registries
            .workflows
            .get(workflow_id)
            .ok_or_else(|| {
                WorkflowError::TriggerError(format!(
                    "Triggered workflow '{}' not found in resource registries",
                    workflow_id
                ))
            })?
            .as_ref()
            .clone();
        let graph = template_to_graph(&template);

        let max_execution_time = template
            .definition
            .triggered_subworkflow_config
            .as_ref()
            .and_then(|config| config.timeout)
            // Template-level timeout wins; fall back to the configured
            // execution default (0 = unlimited, matching the engine).
            .or_else(|| {
                self.limits
                    .as_ref()
                    .and_then(|l| l.execution_defaults.as_ref())
                    .and_then(|d| d.max_execution_time_ms)
            });
        // Checkpoints follow the template's `triggered_subworkflow_config`:
        // a triggered sub-workflow may opt into checkpoint
        // capture; the pre-refactor default (false) stays when unconfigured.
        let enable_checkpoints = template
            .definition
            .triggered_subworkflow_config
            .as_ref()
            .and_then(|config| config.enable_checkpoints)
            .unwrap_or(false);
        let limits = self.limits.clone().unwrap_or_default();
        let options = WorkflowExecutionOptions {
            input: Some(input),
            max_steps: None,
            timeout: None,
            max_execution_time,
            enable_checkpoints: Some(enable_checkpoints),
            node_timeout: limits
                .execution_defaults
                .as_ref()
                .and_then(|d| d.node_timeout_ms),
            max_pause_duration: None,
            retry_budget: None,
            on_failure: None,
            max_retries: None,
            retry_delay_ms: None,
            exponential_backoff: None,
            fallback_output: None,
            max_navigation_multiplier: limits
                .workflow
                .as_ref()
                .and_then(|w| w.max_navigation_multiplier),
            loop_max_iterations_cap: limits
                .workflow
                .as_ref()
                .and_then(|w| w.loop_max_iterations_cap),
        };

        let tool_registry = match &self.tool_registry {
            Some(shared) => shared.clone(),
            None => {
                let fresh = Arc::new(wf_tools::create_default_tool_registry());
                if let Some(loader) = &self.skill_loader {
                    fresh.set_skill_loader(loader.clone());
                }
                fresh
            }
        };

        let exec_ctx = ExecutorContext::new(
            wf_common::generate_id(),
            wf_common::generate_id(),
            Some(self.event_bus.clone()),
            tool_registry,
            options,
        )
        .with_resource_registries(self.registries.clone());
        let exec_ctx = match &self.signal_bus {
            Some(bus) => exec_ctx.with_signal_bus(bus.clone()),
            None => exec_ctx,
        };
        // Lifecycle wiring (compression chain closure): the execution's
        // variable map is the write-back target of its named message arrays;
        // registered at start and unregistered at end, so the trigger
        // listener can write compressed arrays back even while the execution
        // continues (or after it finished, harmlessly).
        let execution_id = exec_ctx.execution_id.clone();
        self.contexts
            .register_workflow(execution_id.clone(), exec_ctx.variables.clone());
        let entity = WorkflowExecutionEntity::new(
            exec_ctx.execution_id.clone(),
            exec_ctx.workflow_id.clone(),
        );
        let mut coordinator =
            WorkflowCoordinator::new(exec_ctx, graph, self.handlers.clone())?.with_entity(entity);
        let outcome = coordinator.execute().await;
        self.contexts.unregister(&execution_id);
        outcome
    }
}

/// The user-template sub-workflow action: the concrete
/// [`TriggerActionRunner`] for `TriggerAction::ExecuteTriggeredSubworkflow`.
///
/// Runs the configured summary sub-workflow over the message snapshot carried
/// by the triggering event, writes the compressed array back into the
/// emitting execution's named context (workflow targets through the
/// [`ExecutionContextRegistry`]; agent conversations self-consume the
/// completed event) and publishes `CONTEXT_COMPRESSION_COMPLETED`.
///
/// The engine-internal compression chain is served by the
/// [`CompressionService`] hook receiver instead; this runner exists for user
/// trigger templates. Other action types are executed synchronously by the
/// message node handlers (wf-workflow), not by the event listener.
#[derive(Clone)]
pub struct SubworkflowActionRunner {
    bus: Arc<EventBus>,
    runner: Arc<dyn SubworkflowRunner>,
    contexts: Arc<ExecutionContextRegistry>,
    /// Listener shutdown token; fire-and-forget sub-workflows race against
    /// it so in-flight runs are stopped at shutdown.
    shutdown: CancellationToken,
    /// Optional durable trigger-execution ledger: every triggered
    /// sub-workflow run is recorded here for the management surface.
    storage: Option<Arc<dyn TriggerExecutionRecorder>>,
    /// Optional trigger runtime state registry: records the
    /// fired trigger and its in-flight status, captured into checkpoints.
    trigger_states: Option<Arc<wf_workflow::TriggerStateRegistry>>,
}

impl SubworkflowActionRunner {
    pub fn new(
        bus: Arc<EventBus>,
        runner: Arc<dyn SubworkflowRunner>,
        contexts: Arc<ExecutionContextRegistry>,
        shutdown: CancellationToken,
    ) -> Self {
        Self::with_storage(bus, runner, contexts, shutdown, None)
    }

    pub fn with_storage(
        bus: Arc<EventBus>,
        runner: Arc<dyn SubworkflowRunner>,
        contexts: Arc<ExecutionContextRegistry>,
        shutdown: CancellationToken,
        storage: Option<Arc<dyn TriggerExecutionRecorder>>,
    ) -> Self {
        Self {
            bus,
            runner,
            contexts,
            shutdown,
            storage,
            trigger_states: None,
        }
    }

    pub fn with_trigger_state_registry(
        mut self,
        registry: Arc<wf_workflow::TriggerStateRegistry>,
    ) -> Self {
        self.trigger_states = Some(registry);
        self
    }
}

impl SubworkflowActionRunner {
    /// Accessors for the compression service (`compression.rs`), which
    /// reuses the same runner/contexts/bus as the listener.
    pub(crate) fn runner(&self) -> Arc<dyn SubworkflowRunner> {
        self.runner.clone()
    }

    pub(crate) fn contexts(&self) -> &Arc<ExecutionContextRegistry> {
        &self.contexts
    }

    pub(crate) fn bus(&self) -> &Arc<EventBus> {
        &self.bus
    }
}

#[async_trait]
impl TriggerActionRunner for SubworkflowActionRunner {
    async fn run(&self, template: &TriggerTemplate, event: &BaseEvent) -> WorkflowResult<()> {
        let Some(action) = &template.action else {
            return Ok(());
        };
        let TriggerAction::ExecuteTriggeredSubworkflow {
            triggered_workflow_id,
            wait_for_completion,
            timeout,
            ..
        } = action
        else {
            // Other action types are executed synchronously by the trigger
            // node handlers, not by the event listener.
            return Ok(());
        };

        // The event must name the message array to compress and carry its
        // snapshot; anything else is skipped (best-effort compression). The
        // typed parse validates both the event type and the required keys,
        // so a schema drift surfaces as a logged skip instead of a silent
        // empty-array degradation.
        let meta = match ContextCompressionRequestedMeta::try_from(event) {
            Ok(meta) => meta,
            Err(e) => {
                debug!(
                    "Trigger '{}' matched but the event is not a valid compression request: {}",
                    template.name, e
                );
                return Ok(());
            }
        };
        let Some(execution_id) = event.execution_id.clone() else {
            return Ok(());
        };
        let target_context_id = meta.target_context_id;
        if meta.messages.is_empty() {
            debug!(
                "Trigger '{}' matched but the event carries no named message array, skipping",
                template.name
            );
            return Ok(());
        }
        let messages = meta.messages;
        // Versioned write-back: the compressed array is written back only if
        // the target array is still at the version the event snapshot was
        // taken from; concurrent appends discard stale results.
        let expected_version = meta.array_version;

        let input = serde_json::json!({ "conversationHistory": messages });
        let wait = wait_for_completion.unwrap_or(true);
        let timeout_ms = timeout.unwrap_or(DEFAULT_TRIGGER_TIMEOUT_MS);

        // Trigger runtime state (checkpoint audit): the trigger fired for the
        // emitting execution and its run is now in flight.
        if let Some(registry) = &self.trigger_states {
            registry.record_start(
                &execution_id,
                wf_workflow::TriggerStateRecord::running(
                    template.name.clone(),
                    event.id.to_string(),
                    event.r#type.as_str().to_string(),
                    wf_common::now(),
                ),
            );
        }

        let start = wf_common::now();
        let result = if wait {
            let outcome = tokio::time::timeout(
                std::time::Duration::from_millis(timeout_ms),
                self.run_subworkflow(
                    triggered_workflow_id,
                    input,
                    event,
                    &target_context_id,
                    expected_version,
                ),
            )
            .await;
            match outcome {
                Ok(result) => result,
                Err(_) => Err(WorkflowError::TriggerError(format!(
                    "Triggered subworkflow '{}' timed out after {}ms",
                    triggered_workflow_id, timeout_ms
                ))),
            }
        } else {
            // Fire-and-forget: the emitting execution must not wait. Aborted
            // at listener shutdown so in-flight sub-workflows are stopped.
            let runner = self.runner.clone();
            let contexts = self.contexts.clone();
            let bus = self.bus.clone();
            let shutdown = self.shutdown.clone();
            let storage = self.storage.clone();
            let trigger_states = self.trigger_states.clone();
            let parent_execution_id = execution_id.clone();
            let event_id = event.id.to_string();
            let workflow_id = triggered_workflow_id.clone();
            let template = template.clone();
            let event = event.clone();
            let target_context_id = target_context_id.clone();
            let action_type = "execute_triggered_subworkflow";

            let callback = async move {
                let run = runner.run(&workflow_id, input);
                let (success, error) = tokio::select! {
                    output = run => {
                        match output {
                            Ok(output) => {
                                if let Err(e) = handle_subworkflow_output(
                                    &contexts,
                                    &bus,
                                    event.execution_id.as_deref().unwrap_or_default(),
                                    event.agent_loop_id.as_deref(),
                                    &target_context_id,
                                    expected_version,
                                    &output,
                                )
                                .await
                                {
                                    warn!(
                                        "Triggered subworkflow '{}' completed but write-back failed: {}",
                                        workflow_id, e
                                    );
                                    (false, Some(e.to_string()))
                                } else {
                                    (true, None)
                                }
                            }
                            Err(e) => {
                                warn!("Triggered subworkflow '{}' failed: {}", workflow_id, e);
                                (false, Some(e.to_string()))
                            }
                        }
                    }
                    _ = shutdown.cancelled() => {
                        debug!("Triggered subworkflow '{}' aborted at shutdown", workflow_id);
                        (false, Some("aborted at listener shutdown".to_string()))
                    }
                };
                if let Some(registry) = &trigger_states {
                    registry.record_end(
                        &parent_execution_id,
                        &event_id,
                        if success { "completed" } else { "failed" },
                    );
                }
                record_trigger_execution(
                    &storage,
                    &template,
                    &event,
                    action_type,
                    success,
                    error,
                    wf_common::now() - start,
                    None,
                )
                .await;
            };

            tokio::spawn(callback);
            Ok(())
        };
        if let Some(registry) = &self.trigger_states {
            registry.record_end(
                &execution_id,
                &event.id.to_string(),
                if result.is_ok() {
                    "completed"
                } else {
                    "failed"
                },
            );
        }
        let (success, error) = match &result {
            Ok(()) => (true, None),
            Err(e) => (false, Some(e.to_string())),
        };
        record_trigger_execution(
            &self.storage,
            template,
            event,
            "execute_triggered_subworkflow",
            success,
            error,
            wf_common::now() - start,
            None,
        )
        .await;
        result
    }
}

impl SubworkflowActionRunner {
    /// Run the sub-workflow, write the compressed array back to the named
    /// context of the emitting execution and emit the completed event.
    async fn run_subworkflow(
        &self,
        workflow_id: &str,
        input: serde_json::Value,
        event: &BaseEvent,
        target_context_id: &str,
        expected_version: u64,
    ) -> WorkflowResult<()> {
        let output = self.runner.run(workflow_id, input).await?;
        handle_subworkflow_output(
            &self.contexts,
            &self.bus,
            event.execution_id.as_deref().unwrap_or_default(),
            event.agent_loop_id.as_deref(),
            target_context_id,
            expected_version,
            &output,
        )
        .await
    }
}
