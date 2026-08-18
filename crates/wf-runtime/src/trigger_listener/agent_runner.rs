//! The nested-agent-execution trigger action: the concrete
//! [`TriggerActionRunner`] behind `TriggerAction::ExecuteTriggeredAgentExecution`.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};
use wf_agent::entity::AgentLoopEntity;
use wf_agent::registry::AgentLoopRegistry;
use wf_agent::trigger::{
    AgentExecutorCallback, TriggeredAgentExecutionConfig, TriggeredAgentExecutionManager,
};
use wf_core::scheduler::{TaskCallback, TaskPriority, TaskScheduler};
use wf_core::EventBus;
use wf_execution_shared::hooks::HookRegistry;
use wf_tools::callback::{AgentLoopConfig, AgentLoopInput};
use wf_types::events::BaseEvent;
use wf_types::trigger::{ConversationAnchor, TriggerAction, TriggerAgentInputMode, TriggerTemplate};
use wf_types::Id;
use wf_workflow::error::{WorkflowError, WorkflowResult};
use wf_workflow::trigger_listener::TriggerActionRunner;

use super::{record_trigger_execution, TriggerExecutionRecorder};

/// The nested-agent-execution trigger action: the concrete
/// [`TriggerActionRunner`] behind `TriggerAction::ExecuteTriggeredAgentExecution`.
///
/// Resolves the parent agent loop from the event ids (`agent_loop_id`
/// first, then `execution_id`), captures the turn anchor from the event
/// metadata (`message_count` / `array_version`), feeds the child a snapshot
/// of the parent conversation up to the anchor (`input_mode`) and submits
/// the child through the [`TriggeredAgentExecutionManager`]. On completion
/// the result is written back per the configured `writeback` mode: the
/// parent variable snapshot always, plus a version-checked conversation
/// write-back (`replace` / `append`) when configured — the parent session
/// consumer applies it only while the conversation is still at the anchor
/// version, so the parent loop reads it on its next LLM request (async
/// injection). Events without a resolvable parent loop run fire-and-forget
/// without a write-back target — a missing parent is never a hard error.
pub struct AgentTriggerRunner {
    manager: Arc<TriggeredAgentExecutionManager>,
    executor: AgentExecutorCallback,
    agent_registry: Arc<AgentLoopRegistry>,
    shutdown: CancellationToken,
    storage: Option<Arc<dyn TriggerExecutionRecorder>>,
    /// Shared task scheduler for fire-and-forget agent executions.
    scheduler: Option<Arc<TaskScheduler>>,
}

impl AgentTriggerRunner {
    pub fn new(
        executor: AgentExecutorCallback,
        agent_registry: Arc<AgentLoopRegistry>,
        shutdown: CancellationToken,
        storage: Option<Arc<dyn TriggerExecutionRecorder>>,
    ) -> Self {
        Self {
            manager: Arc::new(TriggeredAgentExecutionManager::new(executor.clone())),
            executor,
            agent_registry,
            shutdown,
            storage,
            scheduler: None,
        }
    }

    /// Wire the shared hook receiver registry and event bus so
    /// `SUBAGENT_START` / `SUBAGENT_STOP` dispatch against the parent
    /// entity's hook configuration (audit copies land on the bus). The
    /// manager is always freshly built by `new`, so the rebuild is cheap.
    pub fn with_hook_context(
        mut self,
        registry: Option<Arc<HookRegistry>>,
        bus: Arc<EventBus>,
    ) -> Self {
        let mut manager = match Arc::try_unwrap(self.manager) {
            Ok(manager) => manager,
            Err(_) => {
                warn!("Rebuilding triggered-agent manager (Arc was shared)");
                TriggeredAgentExecutionManager::new(self.executor.clone())
            }
        };
        if let Some(registry) = registry {
            manager = manager.with_hook_registry(registry);
        }
        manager = manager.with_event_bus(bus);
        self.manager = Arc::new(manager);
        self
    }

    pub fn with_scheduler(mut self, scheduler: Arc<TaskScheduler>) -> Self {
        self.scheduler = Some(scheduler);
        self
    }

    /// Resolve the parent agent loop execution from the triggering event.
    ///
    /// Priority: `agent_loop_id` (precise agent loop reference) >
    /// `execution_id` (fallback, may be a workflow execution id).
    /// When both are present but `agent_loop_id` is the more specific one,
    /// it is always preferred. A warning is logged on fallback so operators
    /// can detect misconfigured triggers.
    fn resolve_parent(
        &self,
        event: &BaseEvent,
    ) -> Option<Arc<AgentLoopEntity>> {
        if let Some(agent_loop_id) = event.agent_loop_id.as_ref() {
            if let Some(parent) = self.agent_registry.get(&Id::from(agent_loop_id.clone())) {
                debug!("Using agent_loop_id as parent: {}", agent_loop_id);
                return Some(parent);
            }
            warn!(
                "agent_loop_id '{}' not found in registry, falling back to execution_id",
                agent_loop_id
            );
        }
        event
            .execution_id
            .as_ref()
            .and_then(|id| {
                let parent = self.agent_registry.get(&Id::from(id.clone()));
                if parent.is_none() {
                    debug!("execution_id '{}' not found in agent registry", id);
                }
                parent
            })
    }
}

#[async_trait]
impl TriggerActionRunner for AgentTriggerRunner {
    async fn run(&self, template: &TriggerTemplate, event: &BaseEvent) -> WorkflowResult<()> {
        let Some(TriggerAction::ExecuteTriggeredAgentExecution {
            agent_id,
            prompt,
            model,
            result_variable,
            wait_for_completion,
            timeout,
            input_mode,
            writeback,
        }) = &template.action
        else {
            return Ok(());
        };
        let input_mode = input_mode.unwrap_or_default();
        let writeback = writeback.unwrap_or_default();

        let child_config = AgentLoopConfig {
            agent_id: Id::from(agent_id.clone()),
            model: model.clone().unwrap_or_else(|| "DEFAULT".to_string()),
            max_iterations: None,
            max_execution_time: None,
            hooks: Vec::new(),
            available_tool_names: Vec::new(),
            initial_tool_names: Vec::new(),
            discoverable_tool_names: Vec::new(),
            enable_general_tool: None,
            activated_tool_names: Vec::new(),
            hidden_tool_names: Vec::new(),
            tool_call_format: None,
            token_limit: None,
            token_warning_threshold: None,
            enable_token_tracking: None,
            general_description: None,
            discoverable_metadata_block: None,
        };
        let start = wf_common::now();
        let child_execution_id = Id::new();
        let action_type = "execute_triggered_agent_execution";

        // Turn anchor: the parent conversation position/version captured at
        // the trigger point (iteration events carry message_count /
        // array_version in their metadata).
        let anchor = event
            .metadata
            .as_ref()
            .and_then(ConversationAnchor::from_event_metadata);

        // Parent loop resolution from the event ids: the child registers on
        // the parent and its result lands in the parent's variable snapshot
        // (and, per the write-back mode, in the parent conversation).
        let parent = self.resolve_parent(event);

        let (success, error) = match parent {
            Some(parent) => {
                // Child input: the parent conversation snapshot up to the
                // anchor (PrefixToAnchor; full snapshot when the anchor is
                // missing) or the full conversation (FullSnapshot).
                let conversation = {
                    let conv = parent.conversation().read().await;
                    match (input_mode, anchor) {
                        (TriggerAgentInputMode::PrefixToAnchor, Some(anchor))
                            if anchor.is_positional() =>
                        {
                            conv.messages()
                                .iter()
                                .take(anchor.message_count)
                                .cloned()
                                .collect()
                        }
                        (TriggerAgentInputMode::PrefixToAnchor, _)
                        | (TriggerAgentInputMode::FullSnapshot, _) => conv.messages().to_vec(),
                    }
                };
                let child_input = AgentLoopInput {
                    message: prompt.clone().unwrap_or_else(|| template.name.clone()),
                    context: HashMap::new(),
                    conversation,
                };
                let config = TriggeredAgentExecutionConfig {
                    parent,
                    result_variable: result_variable
                        .clone()
                        .unwrap_or_else(|| wf_workflow::trigger_internal::AGENT_RESULT.to_string()),
                    wait_for_completion: wait_for_completion.unwrap_or(true),
                    timeout_ms: *timeout,
                    anchor,
                    input_mode,
                    writeback,
                };
                match self
                    .manager
                    .submit_triggered_execution(config, child_config, child_input)
                    .await
                {
                    Ok(_) => (true, None),
                    Err(e) => (false, Some(e.to_string())),
                }
            }
            None => {
                // No parent execution: fire-and-forget without a write-back
                // target, aborted at listener shutdown.
                debug!(
                    "Trigger '{}' matched but no parent agent loop for event {:?}, running fire-and-forget",
                    template.name, event.id
                );
                let child_input = AgentLoopInput {
                    message: prompt.clone().unwrap_or_else(|| template.name.clone()),
                    context: HashMap::new(),
                    conversation: Vec::new(),
                };
                let executor = self.executor.clone();
                let shutdown = self.shutdown.clone();

                let callback: TaskCallback = Box::new(move || Box::pin(async move {
                    let run = executor(child_config, child_input);
                    tokio::select! {
                        output = run => {
                            if let Err(e) = output {
                                warn!("Triggered agent execution failed: {}", e);
                            }
                        }
                        _ = shutdown.cancelled() => {}
                    }
                }));

                if let Some(scheduler) = &self.scheduler {
                    let _ = scheduler.submit_and_forget(
                        format!("agent-trigger-{}", child_execution_id),
                        "trigger_agent".to_string(),
                        callback,
                        TaskPriority::Normal,
                        None,
                    );
                } else {
                    tokio::spawn(async move {
                        callback().await;
                    });
                }
                (true, None)
            }
        };

        record_trigger_execution(
            &self.storage,
            template,
            event,
            action_type,
            success,
            error.clone(),
            wf_common::now() - start,
            Some(child_execution_id),
        )
        .await;
        if success {
            Ok(())
        } else {
            Err(WorkflowError::TriggerError(error.unwrap_or_default()))
        }
    }
}
