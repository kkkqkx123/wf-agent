//! Runtime assembly for the event-driven trigger listener and the hook
//! receiver registry.
//!
//! Implements the wf-workflow listener traits over the runtime's own pieces:
//!
//! - [`ResourceTriggerRegistry`]: user trigger templates from the wf-resource
//!   registrar;
//! - [`WorkflowRunner`]: triggered sub-workflows executed through the
//!   `WorkflowCoordinator` (predefined `llm_summary_workflow`);
//! - [`SubworkflowActionRunner`]: the user-template sub-workflow action —
//!   parse the triggering event, run the summary workflow over its message
//!   snapshot, write the compressed array back through the
//!   [`ExecutionContextRegistry`] and publish the completed event;
//! - [`CompressionService`]: the engine's builtin hook receiver for the
//!   `CONTEXT_COMPRESSION_REQUESTED` signal. Registered into the shared
//!   [`HookRegistry`] at runtime assembly; the engine dispatches the signal
//!   synchronously and the service takes over immediately (idempotency
//!   check + spawn of the summary sub-workflow);
//! - write-back registry: wf-workflow's [`ExecutionContextRegistry`], into
//!   which every started workflow execution registers its variable map
//!   (register at start, unregister at end — see [`WorkflowRunner::run`]).
//!
//! `start_trigger_listener` wires the listener traits together and spawns
//! the listener background task; the returned handle's shutdown token stops
//! the loop.

mod agent_runner;
mod compression;
mod context_runner;
mod router;
mod workflow_runner;

pub use agent_runner::AgentTriggerRunner;
pub use context_runner::{ContextTriggerRunner, ContextTriggerRunnerConfig};
pub use router::TriggerActionRouter;
pub use workflow_runner::{
    template_to_graph, ResourceTriggerRegistry, SubworkflowActionRunner, WorkflowRunner,
};
pub use wf_workflow::execution_context::ExecutionContextRegistry;

/// The engine's builtin hook receiver for the `CONTEXT_COMPRESSION_REQUESTED`
/// signal.
pub use compression::CompressionService;
pub use compression::COMPRESSION_SERVICE_RECEIVER_NAME;

use std::sync::Arc;

use async_trait::async_trait;
use tokio_util::sync::CancellationToken;
use tracing::warn;
use wf_agent::registry::AgentLoopRegistry;
use wf_agent::trigger::AgentExecutorCallback;
use wf_core::EventBus;
use wf_execution_shared::hooks::HookRegistry;
use wf_llm::LlmGateway;
use wf_resource::registry::ResourceRegistries;
use wf_storage::adapter::trigger_execution::TriggerExecutionStorageAdapter;
use wf_types::events::BaseEvent;
use wf_types::message::{Message, MessageContent, MessageContentValue};
use wf_types::trigger::TriggerTemplate;
use wf_types::Id;
use wf_workflow::error::{WorkflowError, WorkflowResult};
use wf_workflow::trigger_listener::{
    SubworkflowRunner, TriggerActionRunner, TriggerEventListener, TriggerTemplateRegistry,
};

/// Default timeout applied to a triggered sub-workflow when the action does
/// not configure one. Shared by the sub-workflow action runner
/// (`workflow_runner.rs`) and the compression service (`compression.rs`).
const DEFAULT_TRIGGER_TIMEOUT_MS: u64 = 60000;

/// Minimal object-safe write point for trigger execution records.
///
/// The wf-storage adapter traits use RPITIT (not dyn-compatible); this
/// narrow trait keeps the runners decoupled from the concrete storage
/// backend while staying usable through `dyn`.
#[async_trait]
pub trait TriggerExecutionRecorder: Send + Sync {
    async fn record(
        &self,
        metadata: wf_types::TriggerExecutionStorageMetadata,
    ) -> Result<(), wf_storage::error::StorageError>;
}

#[async_trait]
impl<S> TriggerExecutionRecorder for S
where
    S: TriggerExecutionStorageAdapter,
{
    async fn record(
        &self,
        metadata: wf_types::TriggerExecutionStorageMetadata,
    ) -> Result<(), wf_storage::error::StorageError> {
        self.save(&metadata).await
    }
}

/// Record a trigger execution in the optional durable ledger (management
/// surface). Best-effort: storage failures are logged, never propagated.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn record_trigger_execution(
    storage: &Option<Arc<dyn TriggerExecutionRecorder>>,
    template: &TriggerTemplate,
    event: &BaseEvent,
    action_type: &str,
    success: bool,
    error: Option<String>,
    execution_time_ms: i64,
    child_execution_id: Option<Id>,
) {
    let Some(storage) = storage else {
        return;
    };
    let metadata = wf_types::TriggerExecutionStorageMetadata {
        id: Id::new(),
        trigger_name: template.name.clone(),
        trigger_type: "event".to_string(),
        event: event.r#type.as_str().to_string(),
        execution_id: child_execution_id.or_else(|| event.execution_id.clone()),
        workflow_id: event.workflow_id.clone(),
        success,
        result: None,
        error,
        action_type: Some(action_type.to_string()),
        execution_time_ms,
        triggered_at: event.timestamp,
    };
    if let Err(e) = storage.record(metadata).await {
        warn!(
            "Failed to record trigger execution '{}' ({}): {}",
            template.name, action_type, e
        );
    }
}

/// Write the compressed output back to the emitting execution and publish
/// the CONTEXT_COMPRESSION_COMPLETED event.
pub(crate) async fn handle_subworkflow_output(
    contexts: &Arc<ExecutionContextRegistry>,
    bus: &Arc<EventBus>,
    execution_id: &str,
    agent_loop_id: Option<&str>,
    target_context_id: &str,
    expected_version: u64,
    output: &serde_json::Value,
) -> WorkflowResult<()> {
    let messages: Vec<Message> = serde_json::from_value(output.clone()).unwrap_or_default();
    if messages.is_empty() {
        return Err(WorkflowError::TriggerError(
            "Compression sub-workflow returned no messages".to_string(),
        ));
    }
    // Agent conversations are consumed by the agent engine itself (it
    // subscribes to the completed event and version-checks its session), so
    // only workflow variable-map targets are written back through the
    // registry.
    if agent_loop_id.is_none() {
        if let Err(error) = contexts
            .write_context(
                execution_id,
                target_context_id,
                messages.clone(),
                expected_version,
            )
            .await
        {
            warn!(
                "Context write-back failed for execution {} context {}: {}",
                execution_id, target_context_id, error
            );
        }
    }
    let completed = build_compression_completed_event(
        execution_id,
        agent_loop_id,
        target_context_id,
        expected_version,
        &messages,
    );
    let _ = bus.publish(completed);
    Ok(())
}

/// Build the CONTEXT_COMPRESSION_COMPLETED event from the compressed message
/// array (messageOutputs of the summary workflow): the array itself, the
/// summary text, the estimated token count and the array version the
/// compression was produced from (the REQUESTED event's version).
fn build_compression_completed_event(
    execution_id: &str,
    agent_loop_id: Option<&str>,
    target_context_id: &str,
    array_version: u64,
    messages: &[Message],
) -> BaseEvent {
    let summary = messages.last().and_then(|message| match &message.content {
        MessageContentValue::Text(text) => Some(text.clone()),
        MessageContentValue::Rich(parts) => parts.iter().find_map(|part| match part {
            MessageContent::Text { text } => Some(text.clone()),
            _ => None,
        }),
    });
    let tokens_after = wf_llm::estimate_messages(messages) as u64;
    wf_llm::build_context_compression_completed_event(
        execution_id,
        agent_loop_id,
        target_context_id,
        array_version,
        summary.as_deref(),
        tokens_after,
        Some(messages),
    )
}

/// Running trigger listener plus its shutdown token and task handle.
pub struct TriggerListenerHandle {
    pub listener: Arc<TriggerEventListener>,
    pub shutdown: CancellationToken,
    pub handle: tokio::task::JoinHandle<()>,
}

/// Wire the listener traits together and spawn the listener loop.
pub fn start_trigger_listener(
    event_bus: Arc<EventBus>,
    registries: Arc<ResourceRegistries>,
    gateway: Arc<LlmGateway>,
    contexts: Arc<ExecutionContextRegistry>,
) -> TriggerListenerHandle {
    start_trigger_listener_with_skills(event_bus, registries, gateway, contexts, None)
}

/// Like `start_trigger_listener`, but injects the runtime skill loader into
/// the builtin tool executor of triggered sub-workflows.
pub fn start_trigger_listener_with_skills(
    event_bus: Arc<EventBus>,
    registries: Arc<ResourceRegistries>,
    gateway: Arc<LlmGateway>,
    contexts: Arc<ExecutionContextRegistry>,
    skill_loader: Option<Arc<wf_tools::SkillLoader>>,
) -> TriggerListenerHandle {
    let runner: Arc<dyn SubworkflowRunner> = Arc::new(WorkflowRunner::with_skill_loader(
        registries.clone(),
        event_bus.clone(),
        gateway.clone(),
        contexts.clone(),
        skill_loader,
    ));
    spawn_listener(
        event_bus,
        registries,
        contexts,
        runner,
        gateway,
        None,
        None,
        None,
        None,
        None,
        None,
        CancellationToken::new(),
    )
}

/// Like `start_trigger_listener`, but uses a caller-provided shared tool
/// registry (builtin handlers + skills + MCP tools) and shared sandbox
/// runtime for every triggered sub-workflow run.
///
/// `agent_executor`, when present, wires the nested-agent-execution trigger
/// action ([`AgentTriggerRunner`]); `storage` records trigger executions in
/// the durable management ledger; `trigger_state_registry` feeds the
/// checkpoint `trigger_states` audit field.
#[allow(clippy::too_many_arguments)]
pub fn start_trigger_listener_with_registry(
    event_bus: Arc<EventBus>,
    registries: Arc<ResourceRegistries>,
    gateway: Arc<LlmGateway>,
    contexts: Arc<ExecutionContextRegistry>,
    tool_registry: Option<Arc<wf_tools::registry::ToolRegistry>>,
    sandbox: Option<Arc<wf_sandbox::SandboxRuntime>>,
    agent_executor: Option<Arc<wf_agent::executor::AgentLoopExecutor>>,
    storage: Option<Arc<dyn TriggerExecutionRecorder>>,
    trigger_state_registry: Option<Arc<wf_workflow::TriggerStateRegistry>>,
) -> TriggerListenerHandle {
    let runner: Arc<dyn SubworkflowRunner> = Arc::new(WorkflowRunner::with_tool_registry(
        registries.clone(),
        event_bus.clone(),
        gateway.clone(),
        contexts.clone(),
        tool_registry.clone(),
        sandbox.clone(),
    ));
    start_trigger_listener_with_parts(
        event_bus,
        registries,
        contexts,
        runner,
        gateway,
        tool_registry,
        sandbox,
        agent_executor,
        storage,
        trigger_state_registry,
        None,
        CancellationToken::new(),
    )
}

/// Like `start_trigger_listener_with_registry`, but with a caller-provided
/// sub-workflow runner and shutdown token. The runtime bootstrap uses this so
/// the compression service registered on the hook registry shares the same
/// runner and shutdown lifecycle as the listener.
#[allow(clippy::too_many_arguments)]
pub fn start_trigger_listener_with_parts(
    event_bus: Arc<EventBus>,
    registries: Arc<ResourceRegistries>,
    contexts: Arc<ExecutionContextRegistry>,
    runner: Arc<dyn SubworkflowRunner>,
    gateway: Arc<LlmGateway>,
    tool_registry: Option<Arc<wf_tools::registry::ToolRegistry>>,
    sandbox: Option<Arc<wf_sandbox::SandboxRuntime>>,
    agent_executor: Option<Arc<wf_agent::executor::AgentLoopExecutor>>,
    storage: Option<Arc<dyn TriggerExecutionRecorder>>,
    trigger_state_registry: Option<Arc<wf_workflow::TriggerStateRegistry>>,
    hook_registry: Option<Arc<HookRegistry>>,
    shutdown: CancellationToken,
) -> TriggerListenerHandle {
    spawn_listener(
        event_bus,
        registries,
        contexts,
        runner,
        gateway,
        tool_registry,
        sandbox,
        agent_executor,
        storage,
        trigger_state_registry,
        hook_registry,
        shutdown,
    )
}

#[allow(clippy::too_many_arguments)]
fn spawn_listener(
    event_bus: Arc<EventBus>,
    registries: Arc<ResourceRegistries>,
    contexts: Arc<ExecutionContextRegistry>,
    runner: Arc<dyn SubworkflowRunner>,
    gateway: Arc<LlmGateway>,
    tool_registry: Option<Arc<wf_tools::registry::ToolRegistry>>,
    sandbox: Option<Arc<wf_sandbox::SandboxRuntime>>,
    agent_executor: Option<Arc<wf_agent::executor::AgentLoopExecutor>>,
    storage: Option<Arc<dyn TriggerExecutionRecorder>>,
    trigger_state_registry: Option<Arc<wf_workflow::TriggerStateRegistry>>,
    hook_registry: Option<Arc<HookRegistry>>,
    shutdown: CancellationToken,
) -> TriggerListenerHandle {
    let registry: Arc<dyn TriggerTemplateRegistry> =
        Arc::new(ResourceTriggerRegistry::new(registries));
    let compression = SubworkflowActionRunner::with_storage(
        event_bus.clone(),
        runner,
        contexts.clone(),
        shutdown.clone(),
        storage.clone(),
    );
    let compression = match trigger_state_registry {
        Some(registry) => Arc::new(compression.with_trigger_state_registry(registry)),
        None => Arc::new(compression),
    };
    let compression: Arc<dyn TriggerActionRunner> = compression;
    let agent = agent_executor.map(|executor| {
        let runner = AgentTriggerRunner::new(
            agent_callback(executor.clone()),
            executor_agent_registry(&executor),
            shutdown.clone(),
            storage.clone(),
        )
        .with_hook_context(hook_registry.clone(), event_bus.clone());
        Arc::new(runner)
    });
    let action_runner: Arc<dyn TriggerActionRunner> = Arc::new(TriggerActionRouter::new(
        compression,
        agent,
        context_runner(
            &event_bus,
            &contexts,
            &shutdown,
            &gateway,
            &sandbox,
            &tool_registry,
        ),
    ));
    let listener = Arc::new(TriggerEventListener::new(
        event_bus,
        registry,
        action_runner,
        shutdown.clone(),
    ));
    let handle = tokio::spawn({
        let listener = listener.clone();
        async move { listener.run().await }
    });
    TriggerListenerHandle {
        listener,
        shutdown,
        handle,
    }
}

/// Build the in-context action runner: executes variable/stop/pause/skip/
/// notification/script actions against the emitting execution.
///
/// `handlers`/`tool_registry` come from the same wiring used for triggered
/// sub-workflows: a default handler set with the shared gateway/sandbox and
/// the shared tool registry when present.
fn context_runner(
    event_bus: &Arc<EventBus>,
    contexts: &Arc<ExecutionContextRegistry>,
    shutdown: &CancellationToken,
    gateway: &Arc<LlmGateway>,
    sandbox: &Option<Arc<wf_sandbox::SandboxRuntime>>,
    tool_registry: &Option<Arc<wf_tools::registry::ToolRegistry>>,
) -> Arc<ContextTriggerRunner> {
    Arc::new(ContextTriggerRunner::new(
        event_bus.clone(),
        contexts.clone(),
        wf_workflow::create_default_handlers(gateway.clone(), sandbox.clone()),
        tool_registry.clone(),
        shutdown.clone(),
    ))
}

/// Wrap an [`AgentLoopExecutor`] into the child-agent callback consumed by
/// the [`TriggeredAgentExecutionManager`].
fn agent_callback(executor: Arc<wf_agent::executor::AgentLoopExecutor>) -> AgentExecutorCallback {
    Arc::new(move |config, input| {
        let executor = executor.clone();
        Box::pin(async move { executor.execute(config, input).await })
    })
}

/// The registry an executor registers its loops into (`AgentLoopRegistry`).
fn executor_agent_registry(
    executor: &Arc<wf_agent::executor::AgentLoopExecutor>,
) -> Arc<AgentLoopRegistry> {
    executor.agent_registry().clone()
}

/// Build the builtin context-compression hook receiver and register it on
/// the shared hook registry under the `CONTEXT_COMPRESSION_REQUESTED` signal
/// point.
///
/// Returns the registered service (kept alive by the registry; the returned
/// handle is optional). The service shares the listener's shutdown token so
/// in-flight summary sub-workflows are stopped at runtime shutdown.
#[allow(clippy::too_many_arguments)]
pub fn register_compression_receiver(
    registry: &HookRegistry,
    event_bus: Arc<EventBus>,
    runner: Arc<dyn SubworkflowRunner>,
    contexts: Arc<ExecutionContextRegistry>,
    summary_workflow_id: String,
    shutdown: CancellationToken,
    storage: Option<Arc<dyn TriggerExecutionRecorder>>,
    trigger_state_registry: Option<Arc<wf_workflow::TriggerStateRegistry>>,
) -> Arc<CompressionService> {
    let mut service = CompressionService::with_storage(
        event_bus,
        runner,
        contexts,
        summary_workflow_id,
        shutdown,
        storage,
    );
    if let Some(registry) = trigger_state_registry {
        service = service.with_trigger_state_registry(registry);
    }
    let service = Arc::new(service);
    // The builtin receiver runs first (weight above any user receiver): the
    // takeover must be immediate once the engine dispatches.
    if !registry.register(
        wf_llm::token_events::COMPRESSION_SIGNAL_HOOK_TYPE,
        service.clone(),
        1000,
    ) {
        warn!("Compression receiver registration skipped: name already registered");
    }
    service
}

/// Stop the listener loop and await its task.
pub async fn stop_trigger_listener(handle: TriggerListenerHandle) {
    handle.shutdown.cancel();
    let _ = tokio::time::timeout(std::time::Duration::from_secs(2), handle.handle).await;
    let _ = handle.listener;
}

/// Best-effort shutdown of an optional listener; used by the runtime teardown.
pub async fn shutdown_trigger_listener(handle: Option<TriggerListenerHandle>) {
    if let Some(handle) = handle {
        warn!("Stopping event-driven trigger listener");
        stop_trigger_listener(handle).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dashmap::DashMap;
    use serde_json::Value;
    use std::collections::HashMap;
    use std::time::Duration;
    use wf_execution_shared::context::ExecutorContext;
    use wf_execution_shared::hooks::HookContext;
    use wf_llm::mock::{LlmResponseSpec, MockLlmClient};
    use wf_resource::registry::RegisterOptions;
    use wf_types::events::EventType;
    use wf_types::message::{Message, MessageContentValue, MessageRole};
    use wf_types::node::StaticNodeType;
    use wf_types::trigger::{TriggerAction, TriggerTemplate};
    use wf_types::workflow::EdgeType;
    use wf_types::workflow::WorkflowTemplate;
    use wf_types::workflow_execution::{
        WorkflowEdge, WorkflowExecutionOptions, WorkflowGraphStructure, WorkflowNode,
    };
    use wf_workflow::{WorkflowCoordinator, WorkflowExecutionEntity};

    fn text_message(role: MessageRole, text: &str) -> Message {
        Message {
            id: wf_common::generate_id(),
            role,
            content: MessageContentValue::Text(text.to_string()),
            timestamp: wf_common::now(),
            tool_call_id: None,
            tool_name: None,
            tool_calls: None,
            thinking: None,
            metadata: None,
        }
    }

    /// Wait until the bus sees the expected number of receivers (the
    /// listener subscribes on its first poll). Bounded: a wrong expectation
    /// must fail loudly instead of spinning forever.
    async fn wait_for_listener(bus: &EventBus, expected_receivers: usize) {
        for _ in 0..200 {
            if bus.receiver_count() >= expected_receivers {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!(
            "expected {} receivers within 2s, got {}",
            expected_receivers,
            bus.receiver_count()
        );
    }

    /// Poll a condition until it holds (2s budget).
    async fn wait_until(cond: impl Fn() -> bool) {
        for _ in 0..200 {
            if cond() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("condition not reached within budget");
    }

    fn node(id: &str, node_type: &str, inner: Value) -> WorkflowNode {
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

    fn workflow_options() -> WorkflowExecutionOptions {
        WorkflowExecutionOptions {
            input: None,
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
            max_navigation_multiplier: None,
        }
    }

    #[tokio::test]
    async fn agent_hook_trigger_runs_nested_agent_and_writes_back() {
        use wf_agent::entity::AgentLoopEntity;
        use wf_types::trigger::{TriggerCondition, TriggerTemplate};

        let bus = Arc::new(EventBus::new(64));
        let registries = Arc::new(ResourceRegistries::new());

        // Mock LLM drives the nested agent loop.
        let gateway = Arc::new(LlmGateway::new());
        let child_mock = Arc::new(MockLlmClient::new());
        child_mock.default(LlmResponseSpec::text("hook result"));
        gateway.register_mock("mock", child_mock.clone());

        // Register a hook-event trigger template with the nested-agent action
        // (metadata condition: the hook event carries `hook` metadata).
        let ts = wf_common::now();
        let _ = wf_core::registry::MutableRegistry::register(
            &registries.trigger_templates,
            "on_agent_hook".to_string(),
            Arc::new(TriggerTemplate {
                name: "on_agent_hook".to_string(),
                description: Some("run a nested agent on hook".to_string()),
                condition: Some(TriggerCondition {
                    event_type: "HOOK_TRIGGERED".to_string(),
                    event_name: None,
                    condition: None,
                    metadata: Some(HashMap::from([(
                        "hook".to_string(),
                        serde_json::json!("pre_tool"),
                    )])),
                    metadata_exists: None,
                    execution_prefix: None,
                }),
                action: Some(TriggerAction::ExecuteTriggeredAgentExecution {
                    agent_id: "child-agent".to_string(),
                    prompt: Some("summarize the hook".to_string()),
                    model: Some("mock".to_string()),
                    result_variable: Some("hook_agent_result".to_string()),
                    wait_for_completion: Some(true),
                    timeout: Some(5000),
                    input_mode: None,
                    writeback: None,
                }),
                enabled: Some(true),
                max_triggers: None,
                priority: Some(10),
                metadata: None,
                created_at: ts,
                updated_at: ts,
                create_checkpoint: None,
                checkpoint_description_template: None,
            }),
        );

        let contexts = Arc::new(ExecutionContextRegistry::new());
        let agent_executor = Arc::new(wf_agent::executor::AgentLoopExecutor::new(
            gateway.clone(),
            Arc::new(wf_tools::create_default_tool_registry()),
        ));
        let listener = start_trigger_listener_with_registry(
            bus.clone(),
            registries.clone(),
            gateway,
            contexts,
            None,
            None,
            Some(agent_executor.clone()),
            None,
            None,
        );
        wait_for_listener(&bus, 1).await;

        // A live parent agent loop the event points at.
        let parent = Arc::new(AgentLoopEntity::new(Id::from("parent-loop".to_string())));
        let _ = agent_executor.agent_registry().register(parent.clone());

        bus.publish(BaseEvent {
            id: wf_common::generate_id(),
            r#type: EventType::HookTriggered,
            timestamp: wf_common::now(),
            event_name: None,
            workflow_id: None,
            execution_id: Some(Id::from("parent-loop".to_string())),
            agent_loop_id: Some(Id::from("parent-loop".to_string())),
            metadata: Some(HashMap::from([(
                "hook".to_string(),
                serde_json::json!("pre_tool"),
            )])),
        })
        .unwrap();

        // The child ran against the mock and its result was written back
        // into the parent's variable snapshot.
        wait_until(|| child_mock.recorded_count() >= 1).await;
        {
            let state = parent.state.read().await;
            let snapshots = state.variable_snapshots();
            assert_eq!(
                snapshots.get("hook_agent_result"),
                Some(&Value::from("hook result"))
            );
        }

        stop_trigger_listener(listener).await;
    }

    #[tokio::test]
    async fn nested_agent_uses_anchor_snapshot_and_publishes_conversation_writeback() {
        use wf_agent::entity::AgentLoopEntity;
        use wf_types::trigger::{TriggerCondition, TriggerTemplate};

        let bus = Arc::new(EventBus::new(64));
        let registries = Arc::new(ResourceRegistries::new());

        let gateway = Arc::new(LlmGateway::new());
        let child_mock = Arc::new(MockLlmClient::new());
        child_mock.default(LlmResponseSpec::text("anchored child result"));
        gateway.register_mock("mock", child_mock.clone());

        let ts = wf_common::now();
        let _ = wf_core::registry::MutableRegistry::register(
            &registries.trigger_templates,
            "on_iteration".to_string(),
            Arc::new(TriggerTemplate {
                name: "on_iteration".to_string(),
                description: Some("run a nested agent on an iteration boundary".to_string()),
                condition: Some(TriggerCondition {
                    event_type: "AGENT_ITERATION_COMPLETED".to_string(),
                    event_name: None,
                    condition: None,
                    metadata: None,
                    metadata_exists: None,
                    execution_prefix: None,
                }),
                action: Some(TriggerAction::ExecuteTriggeredAgentExecution {
                    agent_id: "child-agent".to_string(),
                    prompt: Some("continue from the parent context".to_string()),
                    model: Some("mock".to_string()),
                    result_variable: Some("iter_agent_result".to_string()),
                    wait_for_completion: Some(true),
                    timeout: Some(5000),
                    input_mode: Some(wf_types::trigger::TriggerAgentInputMode::PrefixToAnchor),
                    writeback: Some(wf_types::trigger::TriggerAgentWriteback::ConversationAppend),
                }),
                enabled: Some(true),
                max_triggers: None,
                priority: Some(10),
                metadata: None,
                created_at: ts,
                updated_at: ts,
                create_checkpoint: None,
                checkpoint_description_template: None,
            }),
        );

        let contexts = Arc::new(ExecutionContextRegistry::new());
        let agent_executor = Arc::new(wf_agent::executor::AgentLoopExecutor::new(
            gateway.clone(),
            Arc::new(wf_tools::create_default_tool_registry()),
        ));
        let listener = start_trigger_listener_with_registry(
            bus.clone(),
            registries.clone(),
            gateway,
            contexts,
            None,
            None,
            Some(agent_executor.clone()),
            None,
            None,
        );
        wait_for_listener(&bus, 1).await;
        // The listener's subscription is live only after `wait_for_listener`
        // above (the listener is the first receiver); subscribing now keeps
        // the write-back assertion below deterministic.
        let mut sub = bus.subscribe();

        // A live parent agent loop with a seeded conversation; the trigger
        // event anchors at its current position.
        let parent = Arc::new(AgentLoopEntity::new(Id::from("anchor-loop".to_string())));
        parent
            .conversation()
            .write()
            .await
            .add_message(text_message(MessageRole::User, "parent context message"));
        let (message_count, array_version) = {
            let conv = parent.conversation().read().await;
            (conv.messages().len(), conv.conversation_version())
        };
        let _ = agent_executor.agent_registry().register(parent.clone());

        bus.publish(BaseEvent {
            id: wf_common::generate_id(),
            r#type: EventType::AgentIterationCompleted,
            timestamp: wf_common::now(),
            event_name: None,
            workflow_id: None,
            execution_id: Some(Id::from("anchor-loop".to_string())),
            agent_loop_id: Some(Id::from("anchor-loop".to_string())),
            metadata: Some(HashMap::from([
                ("iteration".to_string(), serde_json::json!(1)),
                (
                    "message_count".to_string(),
                    serde_json::json!(message_count),
                ),
                (
                    "array_version".to_string(),
                    serde_json::json!(array_version),
                ),
            ])),
        })
        .unwrap();

        // The child ran with the parent snapshot: its first request carries
        // the anchored parent message before the child's own prompt.
        wait_until(|| child_mock.recorded_count() >= 1).await;
        let child_request = child_mock.last_request().unwrap();
        assert!(
            child_request.messages.iter().any(|m| matches!(
                &m.content,
                MessageContentValue::Text(t) if t.contains("parent context message")
            )),
            "child must receive the anchored parent conversation snapshot"
        );

        // The variable fall-back write-back happened.
        for _ in 0..200 {
            if parent
                .state
                .read()
                .await
                .variable_snapshots()
                .contains_key("iter_agent_result")
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        {
            let state = parent.state.read().await;
            assert_eq!(
                state.variable_snapshots().get("iter_agent_result"),
                Some(&Value::from("anchored child result"))
            );
        }

        // The versioned conversation write-back event was published with the
        // anchor version.
        let writeback_event = loop {
            match sub.recv().await {
                Ok(event) if event.r#type == EventType::ConversationWritebackCompleted => {
                    break event
                }
                Ok(_) => continue,
                Err(_) => panic!("event bus closed"),
            }
        };
        let meta = wf_llm::ConversationWritebackCompletedMeta::try_from(&writeback_event).unwrap();
        assert_eq!(meta.array_version, array_version);
        assert_eq!(meta.operation, wf_llm::WRITEBACK_OPERATION_APPEND);
        assert_eq!(meta.messages.len(), 1);

        stop_trigger_listener(listener).await;
    }

    #[tokio::test]
    async fn context_compression_chain_end_to_end() {
        // 1. Components: bus + predefined resources. Only the summary
        // workflow template is registered here: the compression chain is
        // served by the hook-registry dispatch (the preset trigger template
        // was removed from the trigger system).
        let bus = Arc::new(EventBus::new(256));
        let mut sub = bus.subscribe();

        let registries = Arc::new(ResourceRegistries::new());
        let opts = RegisterOptions::default();
        wf_resource::predefined::workflow::register(&registries, &opts);

        // 2. Mock LLM: "main" for the emitting node, "DEFAULT" for the
        // llm_summary_workflow node.
        let gateway = Arc::new(LlmGateway::new());
        let main_mock = Arc::new(MockLlmClient::new());
        main_mock.default(LlmResponseSpec::text("main answer").with_usage(100, 20));
        gateway.register_mock("main", main_mock);
        let summary_mock = Arc::new(MockLlmClient::new());
        summary_mock.default(LlmResponseSpec::text("compressed summary").with_usage(50, 30));
        gateway.register_mock("DEFAULT", summary_mock.clone());

        // 3. Wire the hook registry with the builtin compression receiver:
        // the LLM handler dispatches the compression signal synchronously
        // and the service spawns the summary sub-workflow immediately.
        let contexts = Arc::new(ExecutionContextRegistry::new());
        let hook_registry = Arc::new(HookRegistry::new());
        let runner: Arc<dyn SubworkflowRunner> = Arc::new(WorkflowRunner::with_tool_registry(
            registries.clone(),
            bus.clone(),
            gateway.clone(),
            contexts.clone(),
            None,
            None,
        ));
        register_compression_receiver(
            &hook_registry,
            bus.clone(),
            runner,
            contexts.clone(),
            wf_resource::predefined::workflow::LLM_SUMMARY_WORKFLOW_ID.to_string(),
            CancellationToken::new(),
            None,
            None,
        );

        // 4. Main workflow: an LLM node reading the "chat" named context
        // whose estimated token count exceeds the node-level limit.
        let execution_id = wf_common::generate_id();
        let variables = Arc::new(DashMap::new());
        let mut chat_messages = Vec::new();
        for i in 0..40 {
            chat_messages.push(text_message(
                MessageRole::User,
                &format!("long message {} {}", i, "x".repeat(200)),
            ));
        }
        wf_workflow::append_context(&variables, "chat", chat_messages);
        contexts.register_workflow(execution_id.clone(), variables.clone());

        let graph = WorkflowGraphStructure {
            nodes: vec![
                node("start", "START", Value::Null),
                node(
                    "llm",
                    "LLM",
                    serde_json::json!({
                        "profile_id": "main",
                        "context_id": "chat",
                        "token_limit": 1000,
                        "output_context": "chat_output",
                    }),
                ),
                node("end", "END", Value::Null),
            ],
            edges: vec![edge("start", "llm"), edge("llm", "end")],
            adjacency_list: HashMap::new(),
            reverse_adjacency_list: HashMap::new(),
            start_node_id: Some("start".to_string()),
            end_node_ids: vec!["end".to_string()],
        };
        let handlers = wf_workflow::create_default_handlers(gateway.clone(), None);
        let exec_ctx = ExecutorContext::new(
            execution_id.clone(),
            wf_common::generate_id(),
            Some(bus.clone()),
            Arc::new(wf_tools::create_default_tool_registry()),
            workflow_options(),
        );
        let mut exec_ctx = exec_ctx;
        exec_ctx.variables = variables.clone();
        exec_ctx = exec_ctx.with_hook_registry(hook_registry.clone());
        let entity = WorkflowExecutionEntity::new(
            exec_ctx.execution_id.clone(),
            exec_ctx.workflow_id.clone(),
        );
        let mut coordinator = WorkflowCoordinator::new(exec_ctx, graph, handlers)
            .unwrap()
            .with_entity(entity);
        assert!(coordinator.execute().await.is_ok());

        // 5a. CONTEXT_COMPRESSION_REQUESTED names the "chat" array and
        // carries its message snapshot.
        let requested = loop {
            match sub.recv().await {
                Ok(event) if event.r#type == EventType::ContextCompressionRequested => break event,
                Ok(_) => continue,
                Err(_) => panic!("event bus closed"),
            }
        };
        assert_eq!(
            requested.execution_id.as_deref(),
            Some(execution_id.as_str())
        );
        let requested_meta = wf_llm::ContextCompressionRequestedMeta::try_from(&requested).unwrap();
        assert_eq!(requested_meta.target_context_id, "chat");
        assert_eq!(
            requested_meta.messages.len(),
            40,
            "event must carry the array snapshot"
        );

        // 5b. The summary workflow ran over the conversation payload.
        wait_until(|| summary_mock.recorded_count() >= 1).await;
        let summary_request = summary_mock.last_request().unwrap();
        assert!(
            summary_request.messages.len() >= 40,
            "summary workflow must receive the full conversation"
        );

        // 5c. The compressed array was written back to the named context.
        wait_until(|| {
            let written = wf_workflow::get_context(&variables, "chat");
            written.len() == 1
        })
        .await;
        let written = wf_workflow::get_context(&variables, "chat");
        assert_eq!(written[0].role, MessageRole::Assistant);
        assert_eq!(
            written[0].content,
            MessageContentValue::Text("compressed summary".to_string())
        );

        // 5d. CONTEXT_COMPRESSION_COMPLETED carries the compressed array.
        let completed = loop {
            match sub.recv().await {
                Ok(event) if event.r#type == EventType::ContextCompressionCompleted => break event,
                Ok(_) => continue,
                Err(_) => panic!("event bus closed"),
            }
        };
        let completed_meta = wf_llm::ContextCompressionCompletedMeta::try_from(&completed).unwrap();
        assert_eq!(completed_meta.target_context_id, "chat");
        assert_eq!(completed_meta.messages.len(), 1);
        assert_eq!(
            completed_meta.messages[0].content,
            MessageContentValue::Text("compressed summary".to_string())
        );
        assert!(
            completed_meta.tokens_after < 1000,
            "compressed array must be far below the limit"
        );

        contexts.unregister(&execution_id);
    }

    /// In-memory test double for the trigger execution ledger.
    #[derive(Default)]
    struct TestRecorder {
        records: std::sync::Mutex<Vec<wf_types::TriggerExecutionStorageMetadata>>,
    }

    #[async_trait]
    impl TriggerExecutionRecorder for TestRecorder {
        async fn record(
            &self,
            metadata: wf_types::TriggerExecutionStorageMetadata,
        ) -> Result<(), wf_storage::error::StorageError> {
            self.records.lock().unwrap().push(metadata);
            Ok(())
        }
    }

    #[tokio::test]
    async fn trigger_execution_recorded_and_trigger_states_snapshotted() {
        // A direct agent-trigger run through the runner: the durable ledger
        // gets a record and the checkpoint trigger-state registry captures
        // the fired trigger.
        let bus = Arc::new(EventBus::new(64));
        let registries = Arc::new(ResourceRegistries::new());
        let gateway = Arc::new(LlmGateway::new());
        let child_mock = Arc::new(MockLlmClient::new());
        child_mock.default(LlmResponseSpec::text("child done"));
        gateway.register_mock("mock", child_mock.clone());

        let ts = wf_common::now();
        let _ = wf_core::registry::MutableRegistry::register(
            &registries.trigger_templates,
            "audit-trigger".to_string(),
            Arc::new(TriggerTemplate {
                name: "audit-trigger".to_string(),
                description: None,
                condition: Some(wf_types::trigger::TriggerCondition {
                    event_type: "HOOK_TRIGGERED".to_string(),
                    event_name: None,
                    condition: None,
                    metadata: None,
                    metadata_exists: None,
                    execution_prefix: None,
                }),
                action: Some(TriggerAction::ExecuteTriggeredAgentExecution {
                    agent_id: "child".to_string(),
                    prompt: Some("run".to_string()),
                    model: Some("mock".to_string()),
                    result_variable: Some("audited".to_string()),
                    wait_for_completion: Some(true),
                    timeout: Some(5000),
                    input_mode: None,
                    writeback: None,
                }),
                enabled: Some(true),
                max_triggers: None,
                priority: Some(10),
                metadata: None,
                created_at: ts,
                updated_at: ts,
                create_checkpoint: None,
                checkpoint_description_template: None,
            }),
        );

        let recorder = Arc::new(TestRecorder::default());
        let trigger_states = Arc::new(wf_workflow::TriggerStateRegistry::new());
        let contexts = Arc::new(ExecutionContextRegistry::new());
        let agent_executor = Arc::new(wf_agent::executor::AgentLoopExecutor::new(
            gateway.clone(),
            Arc::new(wf_tools::create_default_tool_registry()),
        ));
        let listener = start_trigger_listener_with_registry(
            bus.clone(),
            registries.clone(),
            gateway,
            contexts,
            None,
            None,
            Some(agent_executor.clone()),
            Some(recorder.clone() as Arc<dyn TriggerExecutionRecorder>),
            Some(trigger_states.clone()),
        );
        wait_for_listener(&bus, 1).await;

        let parent = Arc::new(wf_agent::entity::AgentLoopEntity::new(Id::from(
            "audit-loop".to_string(),
        )));
        let _ = agent_executor.agent_registry().register(parent.clone());
        bus.publish(BaseEvent {
            id: wf_common::generate_id(),
            r#type: EventType::HookTriggered,
            timestamp: wf_common::now(),
            event_name: None,
            workflow_id: None,
            execution_id: Some(Id::from("audit-loop".to_string())),
            agent_loop_id: Some(Id::from("audit-loop".to_string())),
            metadata: None,
        })
        .unwrap();

        wait_until(|| {
            recorder
                .records
                .lock()
                .unwrap()
                .iter()
                .any(|r| r.trigger_name == "audit-trigger")
        })
        .await;
        let record = recorder
            .records
            .lock()
            .unwrap()
            .iter()
            .find(|r| r.trigger_name == "audit-trigger")
            .expect("recorded")
            .clone();
        assert_eq!(record.trigger_type, "event");
        assert_eq!(record.event, "HOOK_TRIGGERED");
        assert_eq!(
            record.action_type.as_deref(),
            Some("execute_triggered_agent_execution")
        );
        assert!(record.success);

        stop_trigger_listener(listener).await;
    }

    #[tokio::test]
    async fn no_compression_event_when_named_array_within_limit() {
        let bus = Arc::new(EventBus::new(64));
        let mut sub = bus.subscribe();

        let registries = Arc::new(ResourceRegistries::new());
        let opts = RegisterOptions::default();
        wf_resource::predefined::workflow::register(&registries, &opts);

        let gateway = Arc::new(LlmGateway::new());
        let main_mock = Arc::new(MockLlmClient::new());
        main_mock.default(LlmResponseSpec::text("main answer").with_usage(100, 20));
        gateway.register_mock("main", main_mock);
        let summary_mock = Arc::new(MockLlmClient::new());
        summary_mock.default(LlmResponseSpec::text("compressed summary"));
        gateway.register_mock("DEFAULT", summary_mock.clone());

        let contexts = Arc::new(ExecutionContextRegistry::new());
        let hook_registry = Arc::new(HookRegistry::new());
        let runner: Arc<dyn SubworkflowRunner> = Arc::new(WorkflowRunner::with_tool_registry(
            registries.clone(),
            bus.clone(),
            gateway.clone(),
            contexts.clone(),
            None,
            None,
        ));
        register_compression_receiver(
            &hook_registry,
            bus.clone(),
            runner,
            contexts.clone(),
            wf_resource::predefined::workflow::LLM_SUMMARY_WORKFLOW_ID.to_string(),
            CancellationToken::new(),
            None,
            None,
        );

        // A short array stays within the limit: no compression requested.
        let execution_id = wf_common::generate_id();
        let variables = Arc::new(DashMap::new());
        wf_workflow::append_context(
            &variables,
            "chat",
            vec![text_message(MessageRole::User, "short")],
        );
        contexts.register_workflow(execution_id.clone(), variables.clone());

        let graph = WorkflowGraphStructure {
            nodes: vec![
                node("start", "START", Value::Null),
                node(
                    "llm",
                    "LLM",
                    serde_json::json!({
                        "profile_id": "main",
                        "context_id": "chat",
                        "token_limit": 1000,
                        "output_context": "chat_output",
                    }),
                ),
                node("end", "END", Value::Null),
            ],
            edges: vec![edge("start", "llm"), edge("llm", "end")],
            adjacency_list: HashMap::new(),
            reverse_adjacency_list: HashMap::new(),
            start_node_id: Some("start".to_string()),
            end_node_ids: vec!["end".to_string()],
        };
        let handlers = wf_workflow::create_default_handlers(gateway.clone(), None);
        let exec_ctx = ExecutorContext::new(
            execution_id.clone(),
            wf_common::generate_id(),
            Some(bus.clone()),
            Arc::new(wf_tools::create_default_tool_registry()),
            workflow_options(),
        );
        let mut exec_ctx = exec_ctx;
        exec_ctx.variables = variables.clone();
        exec_ctx = exec_ctx.with_hook_registry(hook_registry.clone());
        let entity = WorkflowExecutionEntity::new(
            exec_ctx.execution_id.clone(),
            exec_ctx.workflow_id.clone(),
        );
        let mut coordinator = WorkflowCoordinator::new(exec_ctx, graph, handlers)
            .unwrap()
            .with_entity(entity);
        assert!(coordinator.execute().await.is_ok());

        // No compression request within the observation window.
        let compression_observed = tokio::time::timeout(Duration::from_millis(300), async {
            loop {
                match sub.recv().await {
                    Ok(event) if event.r#type == EventType::ContextCompressionRequested => {
                        return true
                    }
                    Ok(_) => continue,
                    Err(_) => return false,
                }
            }
        })
        .await
        .unwrap_or(false);
        assert!(
            !compression_observed,
            "in-limit array must not trigger compression"
        );
        assert_eq!(summary_mock.recorded_count(), 0);

        contexts.unregister(&execution_id);
    }

    #[tokio::test]
    async fn compression_dispatch_takes_over_immediately() {
        use std::sync::atomic::{AtomicBool, Ordering};
        use wf_execution_shared::hooks::dispatch;
        use wf_llm::token_events::{
            KEY_ARRAY_VERSION, KEY_MESSAGES, KEY_MESSAGE_COUNT, KEY_TARGET_CONTEXT_ID,
            KEY_TOKENS_USED, KEY_TOKEN_LIMIT,
        };

        let bus = Arc::new(EventBus::new(64));
        let registries = Arc::new(ResourceRegistries::new());
        wf_resource::predefined::workflow::register(&registries, &RegisterOptions::default());

        // The stub summary runner: records the takeover and then blocks far
        // beyond the dispatch timeout — the engine must not wait for it.
        struct StuckRunner(Arc<AtomicBool>);
        #[async_trait]
        impl SubworkflowRunner for StuckRunner {
            async fn run(&self, _workflow_id: &str, _input: Value) -> WorkflowResult<Value> {
                self.0.store(true, Ordering::SeqCst);
                tokio::time::sleep(Duration::from_secs(10)).await;
                Ok(Value::Null)
            }
        }

        let contexts = Arc::new(ExecutionContextRegistry::new());
        let hook_registry = Arc::new(HookRegistry::new());
        let started = Arc::new(AtomicBool::new(false));
        register_compression_receiver(
            &hook_registry,
            bus.clone(),
            Arc::new(StuckRunner(started.clone())),
            contexts.clone(),
            wf_resource::predefined::workflow::LLM_SUMMARY_WORKFLOW_ID.to_string(),
            CancellationToken::new(),
            None,
            None,
        );

        // A valid compression payload (message snapshot present).
        let messages: Vec<Message> = vec![text_message(MessageRole::User, "long message")];
        let mut data = HashMap::new();
        data.insert(KEY_TARGET_CONTEXT_ID.to_string(), Value::from("chat"));
        data.insert(KEY_TOKENS_USED.to_string(), Value::from(900u64));
        data.insert(KEY_TOKEN_LIMIT.to_string(), Value::from(1000u64));
        data.insert(KEY_MESSAGE_COUNT.to_string(), Value::from(messages.len()));
        data.insert(KEY_ARRAY_VERSION.to_string(), Value::from(1u64));
        data.insert(
            KEY_MESSAGES.to_string(),
            serde_json::to_value(messages).unwrap(),
        );
        let ctx = HookContext {
            execution_id: Id::from("wf-run".to_string()),
            hook_type: wf_llm::token_events::COMPRESSION_SIGNAL_HOOK_TYPE.to_string(),
            data,
        };

        // Dispatch returns as soon as the summary sub-workflow is spawned,
        // never after it completes (the stub blocks 10s).
        let elapsed = std::time::Instant::now();
        dispatch(
            &hook_registry,
            &[],
            wf_llm::token_events::COMPRESSION_SIGNAL_HOOK_TYPE,
            &ctx,
            Some(&bus),
        )
        .await;
        let dispatch_ms = elapsed.elapsed().as_millis();
        assert!(
            dispatch_ms < 2000,
            "dispatch must return at takeover, not after compression (took {dispatch_ms}ms)"
        );
        // The spawned summary task is observable as soon as the runtime
        // schedules it: dispatch did not await it (it is still blocked in
        // the stub for the rest of the test).
        wait_until(|| started.load(Ordering::SeqCst)).await;
    }

    fn template_with_nodes() -> WorkflowTemplate {
        use wf_types::node::BaseStaticNode;
        use wf_types::workflow::{
            Edge, TriggeredSubworkflowConfig, WorkflowDefinition, WorkflowMetadata,
        };
        WorkflowTemplate {
            id: "t_flow".to_string(),
            name: "T Flow".to_string(),
            description: "test".to_string(),
            definition: WorkflowDefinition {
                id: "t_flow".to_string(),
                name: "T Flow".to_string(),
                description: Some("test".to_string()),
                r#type: None,
                version: None,
                nodes: vec![
                    BaseStaticNode {
                        id: "start".into(),
                        node_type: StaticNodeType::StartFromMessage,
                        name: Some("Start".into()),
                        description: None,
                        config: None,
                        execution_config: None,
                    },
                    BaseStaticNode {
                        id: "llm".into(),
                        node_type: StaticNodeType::Llm,
                        name: Some("LLM".into()),
                        description: None,
                        config: None,
                        execution_config: None,
                    },
                    BaseStaticNode {
                        id: "end".into(),
                        node_type: StaticNodeType::ContinueFromMessage,
                        name: Some("End".into()),
                        description: None,
                        config: None,
                        execution_config: None,
                    },
                ],
                edges: vec![
                    Edge {
                        id: "e1".into(),
                        source_node_id: "start".into(),
                        target_node_id: "llm".into(),
                        r#type: EdgeType::Default,
                        condition: None,
                        label: None,
                        description: None,
                        weight: None,
                        metadata: None,
                    },
                    Edge {
                        id: "e2".into(),
                        source_node_id: "llm".into(),
                        target_node_id: "end".into(),
                        r#type: EdgeType::Default,
                        condition: None,
                        label: None,
                        description: None,
                        weight: None,
                        metadata: None,
                    },
                ],
                config: None,
                variables: None,
                triggered_subworkflow_config: Some(TriggeredSubworkflowConfig {
                    enable_checkpoints: Some(false),
                    timeout: Some(5000),
                    max_retries: Some(0),
                }),
                metadata: Some(WorkflowMetadata {
                    author: None,
                    tags: None,
                    category: None,
                }),
                available_tools: None,
                hooks: None,
                created_at: 0,
                updated_at: 0,
            },
            template_category: None,
            template_tags: None,
            is_public: None,
            enabled: None,
        }
    }

    #[test]
    fn template_to_graph_maps_nodes_edges_and_endpoints() {
        let template = template_with_nodes();
        let graph = template_to_graph(&template);

        assert_eq!(graph.nodes.len(), 3);
        assert_eq!(graph.nodes[0].node_type, "START_FROM_MESSAGE");
        assert_eq!(graph.nodes[1].node_type, "LLM");
        assert_eq!(graph.nodes[2].node_type, "CONTINUE_FROM_MESSAGE");
        assert_eq!(graph.start_node_id.as_deref(), Some("start"));
        assert_eq!(graph.end_node_ids, vec!["end".to_string()]);
        assert_eq!(graph.edges.len(), 2);
        assert_eq!(graph.edges[0].source_node_id, "start");
        assert_eq!(graph.edges[1].target_node_id, "end");
    }

    #[tokio::test]
    async fn execution_context_registry_writes_back_to_registered_execution() {
        use wf_workflow::execution_context::WriteBackError;

        let registry = ExecutionContextRegistry::new();
        assert!(!registry.registered("exec-1"));
        assert!(matches!(
            registry.write_context("exec-1", "chat", vec![], 0).await,
            Err(WriteBackError::NotRegistered)
        ));

        let variables = Arc::new(DashMap::new());
        registry.register_workflow("exec-1", variables.clone());
        assert!(registry.registered("exec-1"));

        let msg = Message {
            id: wf_common::generate_id(),
            role: wf_types::message::MessageRole::Assistant,
            content: wf_types::message::MessageContentValue::Text("summary".to_string()),
            timestamp: wf_common::now(),
            tool_call_id: None,
            tool_name: None,
            tool_calls: None,
            thinking: None,
            metadata: None,
        };
        // An array the execution never created is not writable.
        assert!(matches!(
            registry
                .write_context("exec-1", "chat", vec![msg.clone()], 0)
                .await,
            Err(WriteBackError::ContextNotFound)
        ));
        // Versioned write-back of a tracked array succeeds.
        wf_workflow::append_context(&variables, "chat", vec![msg.clone()]);
        let version = wf_workflow::message_context::array_version(&variables, "chat");
        assert!(registry
            .write_context("exec-1", "chat", vec![msg.clone()], version)
            .await
            .is_ok());
        let written = wf_workflow::get_context(&variables, "chat");
        assert_eq!(written.len(), 1);
        assert_eq!(written[0].content, msg.content);

        registry.unregister("exec-1");
        assert!(!registry.registered("exec-1"));
        assert!(matches!(
            registry.write_context("exec-1", "chat", vec![], 0).await,
            Err(WriteBackError::NotRegistered)
        ));
    }

    #[tokio::test]
    async fn versioned_write_back_discards_stale_compression() {
        use wf_workflow::execution_context::WriteBackError;

        let registry = ExecutionContextRegistry::new();
        let variables = Arc::new(DashMap::new());
        wf_workflow::append_context(
            &variables,
            "chat",
            vec![Message {
                id: wf_common::generate_id(),
                role: wf_types::message::MessageRole::User,
                content: wf_types::message::MessageContentValue::Text("old".to_string()),
                timestamp: wf_common::now(),
                tool_call_id: None,
                tool_name: None,
                tool_calls: None,
                thinking: None,
                metadata: None,
            }],
        );
        registry.register_workflow("exec-2", variables.clone());
        let emitted_version = wf_workflow::message_context::array_version(&variables, "chat");
        assert!(emitted_version > 0);

        // New messages appended after the event was emitted: the array moved
        // past the event version, the compressed result must be discarded.
        wf_workflow::append_context(
            &variables,
            "chat",
            vec![Message {
                id: wf_common::generate_id(),
                role: wf_types::message::MessageRole::User,
                content: wf_types::message::MessageContentValue::Text("newer".to_string()),
                timestamp: wf_common::now(),
                tool_call_id: None,
                tool_name: None,
                tool_calls: None,
                thinking: None,
                metadata: None,
            }],
        );
        assert!(matches!(
            registry
                .write_context(
                    "exec-2",
                    "chat",
                    vec![Message {
                        id: wf_common::generate_id(),
                        role: wf_types::message::MessageRole::Assistant,
                        content: wf_types::message::MessageContentValue::Text(
                            "summary".to_string()
                        ),
                        timestamp: wf_common::now(),
                        tool_call_id: None,
                        tool_name: None,
                        tool_calls: None,
                        thinking: None,
                        metadata: None,
                    }],
                    emitted_version,
                )
                .await,
            Err(WriteBackError::VersionMismatch { .. })
        ));
        assert_eq!(wf_workflow::get_context(&variables, "chat").len(), 2);

        // At the current version the write-back succeeds.
        let current = wf_workflow::message_context::array_version(&variables, "chat");
        assert!(registry
            .write_context(
                "exec-2",
                "chat",
                vec![Message {
                    id: wf_common::generate_id(),
                    role: wf_types::message::MessageRole::Assistant,
                    content: wf_types::message::MessageContentValue::Text("summary".to_string()),
                    timestamp: wf_common::now(),
                    tool_call_id: None,
                    tool_name: None,
                    tool_calls: None,
                    thinking: None,
                    metadata: None,
                }],
                current,
            )
            .await
            .is_ok());
        assert_eq!(wf_workflow::get_context(&variables, "chat").len(), 1);
    }

    #[tokio::test]
    async fn context_trigger_sets_variable_on_live_execution() {
        use wf_types::trigger::{TriggerCondition, TriggerTemplate};

        let bus = Arc::new(EventBus::new(64));
        let registries = Arc::new(ResourceRegistries::new());
        let gateway = Arc::new(LlmGateway::new());
        let contexts = Arc::new(ExecutionContextRegistry::new());
        let listener =
            start_trigger_listener(bus.clone(), registries.clone(), gateway, contexts.clone());
        wait_for_listener(&bus, 1).await;

        // A SetVariable trigger template matching custom events.
        let ts = wf_common::now();
        let _ = wf_core::registry::MutableRegistry::register(
            &registries.trigger_templates,
            "on_flag".to_string(),
            Arc::new(TriggerTemplate {
                name: "on_flag".to_string(),
                description: Some("set a variable from an event".to_string()),
                condition: Some(TriggerCondition {
                    event_type: "NODE_CUSTOM_EVENT".to_string(),
                    event_name: Some("flag_raised".to_string()),
                    condition: None,
                    metadata: None,
                    metadata_exists: None,
                    execution_prefix: None,
                }),
                action: Some(TriggerAction::SetVariable {
                    variable_name: "event_flag".to_string(),
                    value: Value::Bool(true),
                }),
                enabled: Some(true),
                max_triggers: None,
                priority: Some(10),
                metadata: None,
                created_at: ts,
                updated_at: ts,
                create_checkpoint: None,
                checkpoint_description_template: None,
            }),
        );

        // A live workflow execution registered in the write-back registry.
        let execution_id = "exec-live-1".to_string();
        let variables = Arc::new(DashMap::new());
        contexts.register_workflow(execution_id.clone(), variables.clone());

        bus.publish(BaseEvent {
            id: wf_common::generate_id(),
            r#type: EventType::NodeCustomEvent,
            timestamp: wf_common::now(),
            event_name: Some("flag_raised".to_string()),
            workflow_id: Some(Id::from("wf-live-1".to_string())),
            execution_id: Some(execution_id.clone()),
            agent_loop_id: None,
            metadata: None,
        })
        .unwrap();

        // The action ran against the live execution's variable map.
        wait_until(|| variables.contains_key("event_flag")).await;
        assert_eq!(
            variables.get("event_flag").map(|v| v.value().clone()),
            Some(Value::Bool(true))
        );

        stop_trigger_listener(listener).await;
    }

    #[tokio::test]
    async fn context_trigger_skips_events_without_live_context() {
        use wf_types::trigger::{TriggerCondition, TriggerTemplate};

        let bus = Arc::new(EventBus::new(64));
        let registries = Arc::new(ResourceRegistries::new());
        let gateway = Arc::new(LlmGateway::new());
        let contexts = Arc::new(ExecutionContextRegistry::new());
        let listener =
            start_trigger_listener(bus.clone(), registries.clone(), gateway, contexts.clone());
        wait_for_listener(&bus, 1).await;

        let ts = wf_common::now();
        let _ = wf_core::registry::MutableRegistry::register(
            &registries.trigger_templates,
            "on_flag_2".to_string(),
            Arc::new(TriggerTemplate {
                name: "on_flag_2".to_string(),
                description: None,
                condition: Some(TriggerCondition {
                    event_type: "NODE_CUSTOM_EVENT".to_string(),
                    event_name: Some("flag_raised".to_string()),
                    condition: None,
                    metadata: None,
                    metadata_exists: None,
                    execution_prefix: None,
                }),
                action: Some(TriggerAction::SetVariable {
                    variable_name: "event_flag".to_string(),
                    value: Value::Bool(true),
                }),
                enabled: Some(true),
                max_triggers: None,
                priority: Some(10),
                metadata: None,
                created_at: ts,
                updated_at: ts,
                create_checkpoint: None,
                checkpoint_description_template: None,
            }),
        );

        // No execution registered: the runner must skip (agent sessions are
        // not registered), never fail the listener loop.
        bus.publish(BaseEvent {
            id: wf_common::generate_id(),
            r#type: EventType::NodeCustomEvent,
            timestamp: wf_common::now(),
            event_name: Some("flag_raised".to_string()),
            workflow_id: None,
            execution_id: Some("no-such-execution".to_string()),
            agent_loop_id: None,
            metadata: None,
        })
        .unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;

        stop_trigger_listener(listener).await;
    }
}
