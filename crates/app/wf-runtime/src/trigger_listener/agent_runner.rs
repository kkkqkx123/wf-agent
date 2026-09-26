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
use wf_core::EventBus;
use wf_execution_shared::hooks::HookHandlerRegistry;
use wf_tools::callback::{AgentLoopConfig, AgentLoopInput};
use wf_types::events::BaseEvent;
use wf_types::trigger::{
    ConversationAnchor, TriggerAction, TriggerAgentInputMode, TriggerTemplate,
};
use wf_types::Id;
use wf_workflow::error::{WorkflowError, WorkflowResult};
use wf_workflow::trigger::TriggerActionRunner;

use super::{record_trigger_execution, TriggerExecutionRecorder, TriggerOutcome};

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
        }
    }

    /// Wire the shared hook handler registry and event bus so
    /// `SUBAGENT_START` / `SUBAGENT_STOP` fire against the parent
    /// entity's hook configuration (audit copies land on the bus). The
    /// manager is always freshly built by `new`, so the rebuild is cheap.
    pub fn with_hook_context(
        mut self,
        registry: Option<Arc<HookHandlerRegistry>>,
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
            manager = manager.with_hook_handler_registry(registry);
        }
        manager = manager.with_event_bus(bus);
        self.manager = Arc::new(manager);
        self
    }

    /// Resolve the parent agent loop execution from the triggering event.
    ///
    /// Priority: `agent_loop_id` (precise agent loop reference) >
    /// `execution_id` (fallback, may be a workflow execution id).
    /// When both are present but `agent_loop_id` is the more specific one,
    /// it is always preferred. A warning is logged on fallback so operators
    /// can detect misconfigured triggers.
    fn resolve_parent(&self, event: &BaseEvent) -> Option<Arc<AgentLoopEntity>> {
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
        event.execution_id.as_ref().and_then(|id| {
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
        // Cold-start agent: no parent loop, no conversation anchor, no
        // write-back. Always fire-and-forget; the ledger entry is recorded
        // by the spawned task when the run settles, with its real outcome.
        if let Some(TriggerAction::ExecuteAgent { .. }) = &template.action {
            return self.run_cold(template, event).await;
        }
        let Some(TriggerAction::ExecuteTriggeredAgentExecution {
            agent_id,
            prompt,
            model,
            result_variable,
            wait_for_completion,
            timeout,
            input_mode,
            writeback,
            checkpoint_message_interval,
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
            tool_call_protocol: None,
            token_limit: None,
            token_warning_threshold: None,
            enable_token_tracking: None,
            general_description: None,
            discoverable_metadata_block: None,
            history_normalization: false,
            checkpoint_message_interval: checkpoint_message_interval
                .and_then(|n| (n > 0).then_some(n)),
        };
        let start = wf_common::now();
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

        // Degraded configurations are loud: a conversation write-back
        // without an anchor always falls back to variable-only inside the
        // manager, and a prefix input without a positional anchor falls
        // back to the full snapshot. Both are logged here where the
        // template and event ids are known.
        if parent.is_some()
            && anchor.is_none()
            && writeback != wf_types::trigger::TriggerAgentWriteback::Variable
        {
            warn!(
                "Trigger '{}' requests conversation write-back for event {} without a conversation anchor; falling back to variable-only write-back",
                template.name, event.id
            );
        }

        let (success, error) = match parent {
            Some(parent) => {
                // Child input via the shared helper so the engine and the
                // runtime never diverge on prefix/full-snapshot semantics.
                let conversation = {
                    let conv = parent.conversation().read().await;
                    if input_mode == TriggerAgentInputMode::PrefixToAnchor
                        && !anchor.is_some_and(|anchor| anchor.is_positional())
                    {
                        debug!(
                            "Trigger '{}' uses prefix input for event {} without a positional anchor; feeding the full snapshot",
                            template.name, event.id
                        );
                    }
                    wf_agent::trigger::snapshot_conversation_for_child(
                        conv.messages(),
                        input_mode,
                        anchor,
                    )
                };
                let child_input = AgentLoopInput {
                    message: prompt.clone().unwrap_or_else(|| template.name.clone()),
                    context: HashMap::new(),
                    conversation,
                };
                let config = TriggeredAgentExecutionConfig {
                    parent,
                    result_variable: result_variable.clone().unwrap_or_else(|| {
                        wf_workflow::trigger::internal::AGENT_RESULT.to_string()
                    }),
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
                // target, aborted at listener shutdown. Loud (warn) because
                // it usually means the trigger condition points at an
                // unknown agent loop id.
                warn!(
                    "Trigger '{}' matched but no parent agent loop for event {} (agent_loop_id={:?}, execution_id={:?}); running fire-and-forget without write-back",
                    template.name,
                    event.id,
                    event.agent_loop_id,
                    event.execution_id
                );
                let child_input = AgentLoopInput {
                    message: prompt.clone().unwrap_or_else(|| template.name.clone()),
                    context: HashMap::new(),
                    conversation: Vec::new(),
                };
                let executor = self.executor.clone();
                let shutdown = self.shutdown.clone();
                let storage = self.storage.clone();
                let ledger_template = template.clone();
                let ledger_event = event.clone();
                // The ledger entry travels with the real outcome (same
                // discipline as the cold-start path): recording happens when
                // the run settles, and a shutdown-abandoned run writes no
                // entry at all — never a false success at submission time.
                tokio::spawn(async move {
                    let run = executor(child_config, child_input);
                    let outcome: Result<(), String> = tokio::select! {
                        output = run => output.map(|_| ()).map_err(|e| {
                            warn!("Triggered agent execution failed: {}", e);
                            e.to_string()
                        }),
                        _ = shutdown.cancelled() => return,
                    };
                    record_trigger_execution(
                        &storage,
                        &ledger_template,
                        &ledger_event,
                        TriggerOutcome {
                            action_type,
                            success: outcome.is_ok(),
                            error: outcome.err(),
                            execution_time_ms: wf_common::now() - start,
                            child_execution_id: None,
                        },
                    )
                    .await;
                });
                return Ok(());
            }
        };

        // Ledger linkage: the durable record must point at the emitting
        // execution (via the `child_execution_id.or(event.execution_id)`
        // fallback in `record_trigger_execution`). The real child execution
        // id is generated inside `TriggeredAgentExecutionManager` and is not
        // returned here, so report `None` instead of a fabricated id that
        // would break the parent linkage.
        record_trigger_execution(
            &self.storage,
            template,
            event,
            TriggerOutcome {
                action_type,
                success,
                error: error.clone(),
                execution_time_ms: wf_common::now() - start,
                child_execution_id: None,
            },
        )
        .await;
        if success {
            Ok(())
        } else {
            Err(WorkflowError::TriggerError(error.unwrap_or_default()))
        }
    }
}

impl AgentTriggerRunner {
    /// Cold-start a fresh agent loop with no parent (`ExecuteAgent`).
    async fn run_cold(&self, template: &TriggerTemplate, event: &BaseEvent) -> WorkflowResult<()> {
        let Some(TriggerAction::ExecuteAgent {
            agent_id,
            prompt,
            model,
            input,
            timeout,
            checkpoint_message_interval,
        }) = &template.action
        else {
            return Ok(());
        };
        let timeout = *timeout;
        let start = wf_common::now();
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
            tool_call_protocol: None,
            token_limit: None,
            token_warning_threshold: None,
            enable_token_tracking: None,
            general_description: None,
            discoverable_metadata_block: None,
            history_normalization: false,
            checkpoint_message_interval: checkpoint_message_interval
                .and_then(|n| (n > 0).then_some(n)),
        };
        let child_input = AgentLoopInput {
            message: prompt.clone().unwrap_or_else(|| template.name.clone()),
            context: input.clone().unwrap_or_default(),
            conversation: Vec::new(),
        };
        let executor = self.executor.clone();
        let shutdown = self.shutdown.clone();
        let storage = self.storage.clone();
        let template = template.clone();
        let event = event.clone();
        let agent_id = agent_id.to_string();
        // The ledger entry travels with the real outcome: recording happens
        // when the run settles (failure and timeout included), not at
        // submission, so a cold-started child never leaves a false success.
        tokio::spawn(async move {
            let run = executor(child_config, child_input);
            let outcome: Result<(), String> = match timeout {
                Some(ms) => {
                    let elapsed = tokio::select! {
                        output = tokio::time::timeout(std::time::Duration::from_millis(ms), run) => output,
                        _ = shutdown.cancelled() => return,
                    };
                    match elapsed {
                        Ok(Ok(_)) => Ok(()),
                        Ok(Err(e)) => {
                            warn!("Cold-started agent '{}' failed: {}", agent_id, e);
                            Err(e.to_string())
                        }
                        Err(_) => {
                            warn!("Cold-started agent '{}' timed out after {}ms", agent_id, ms);
                            Err(format!("timed out after {}ms", ms))
                        }
                    }
                }
                None => {
                    tokio::select! {
                        output = run => match output {
                            Ok(_) => Ok(()),
                            Err(e) => {
                                warn!("Cold-started agent '{}' failed: {}", agent_id, e);
                                Err(e.to_string())
                            }
                        },
                        _ = shutdown.cancelled() => return,
                    }
                }
            };
            record_trigger_execution(
                &storage,
                &template,
                &event,
                TriggerOutcome {
                    action_type: "execute_agent",
                    success: outcome.is_ok(),
                    error: outcome.err(),
                    execution_time_ms: wf_common::now() - start,
                    child_execution_id: None,
                },
            )
            .await;
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Run the nested-agent trigger action with a recording executor and
    /// return the child `checkpoint_message_interval` the runner forwarded.
    async fn forwarded_interval(action: TriggerAction) -> Option<u32> {
        use wf_tools::callback::{AgentLoopOutput, LoopFinishReason};
        use wf_types::events::EventType;

        let seen = Arc::new(std::sync::Mutex::new(None::<Option<u32>>));
        let seen_clone = seen.clone();
        let executor: AgentExecutorCallback = Arc::new(move |config, _input| {
            let seen_clone = seen_clone.clone();
            Box::pin(async move {
                *wf_common::lock::lock_ok(seen_clone.lock()) =
                    Some(config.checkpoint_message_interval);
                Ok(AgentLoopOutput {
                    agent_loop_id: Id::from("child-1"),
                    result: serde_json::Value::Null,
                    iterations: 1,
                    finish_reason: LoopFinishReason::Completed,
                    conversation: Vec::new(),
                })
            })
        });
        let registry = Arc::new(AgentLoopRegistry::new());
        let parent = Arc::new(AgentLoopEntity::new(Id::from("parent-1")));
        registry.register(parent).expect("parent registers");
        let runner = AgentTriggerRunner::new(executor, registry, CancellationToken::new(), None);
        let template = TriggerTemplate {
            name: "t".to_string(),
            description: None,
            condition: None,
            action: Some(action),
            enabled: Some(true),
            max_triggers: None,
            priority: None,
            dispatch_mode: None,
            allow_multi_effect: None,
            effect_order: None,
            metadata: None,
            created_at: wf_common::now(),
            updated_at: wf_common::now(),
            create_checkpoint: None,
            checkpoint_description_template: None,
        };
        let event = BaseEvent {
            id: Id::from("evt-1"),
            r#type: EventType::NodeCompleted,
            timestamp: wf_common::now(),
            event_name: None,
            workflow_id: None,
            execution_id: None,
            agent_loop_id: Some(Id::from("parent-1")),
            metadata: None,
        };
        runner
            .run(&template, &event)
            .await
            .expect("trigger run succeeds");
        let recorded = *wf_common::lock::lock_ok(seen.lock());
        recorded.expect("executor ran")
    }

    #[tokio::test]
    async fn nested_agent_forwards_checkpoint_message_interval() {
        let interval = forwarded_interval(TriggerAction::ExecuteTriggeredAgentExecution {
            agent_id: "child".to_string(),
            prompt: None,
            model: None,
            result_variable: None,
            wait_for_completion: Some(true),
            timeout: None,
            input_mode: None,
            writeback: None,
            checkpoint_message_interval: Some(7),
        })
        .await;
        assert_eq!(interval, Some(7));

        let absent = forwarded_interval(TriggerAction::ExecuteTriggeredAgentExecution {
            agent_id: "child".to_string(),
            prompt: None,
            model: None,
            result_variable: None,
            wait_for_completion: Some(true),
            timeout: None,
            input_mode: None,
            writeback: None,
            checkpoint_message_interval: None,
        })
        .await;
        assert_eq!(absent, None);

        let zero = forwarded_interval(TriggerAction::ExecuteTriggeredAgentExecution {
            agent_id: "child".to_string(),
            prompt: None,
            model: None,
            result_variable: None,
            wait_for_completion: Some(true),
            timeout: None,
            input_mode: None,
            writeback: None,
            checkpoint_message_interval: Some(0),
        })
        .await;
        assert_eq!(zero, None, "zero disables instead of passing through");
    }

    #[tokio::test]
    async fn cold_start_agent_forwards_checkpoint_message_interval() {
        // Cold-start children run fire-and-forget on a spawned task; poll
        // briefly for the recording instead of asserting synchronously.
        use wf_tools::callback::{AgentLoopOutput, LoopFinishReason};
        use wf_types::events::EventType;

        let seen = Arc::new(std::sync::Mutex::new(None::<Option<u32>>));
        let seen_clone = seen.clone();
        let executor: AgentExecutorCallback = Arc::new(move |config, _input| {
            let seen_clone = seen_clone.clone();
            Box::pin(async move {
                *wf_common::lock::lock_ok(seen_clone.lock()) =
                    Some(config.checkpoint_message_interval);
                Ok(AgentLoopOutput {
                    agent_loop_id: Id::from("child-1"),
                    result: serde_json::Value::Null,
                    iterations: 1,
                    finish_reason: LoopFinishReason::Completed,
                    conversation: Vec::new(),
                })
            })
        });
        let runner = AgentTriggerRunner::new(
            executor,
            Arc::new(AgentLoopRegistry::new()),
            CancellationToken::new(),
            None,
        );
        let template = TriggerTemplate {
            name: "t".to_string(),
            description: None,
            condition: None,
            action: Some(TriggerAction::ExecuteAgent {
                agent_id: "child".to_string(),
                prompt: None,
                model: None,
                input: None,
                timeout: None,
                checkpoint_message_interval: Some(3),
            }),
            enabled: Some(true),
            max_triggers: None,
            priority: None,
            dispatch_mode: None,
            allow_multi_effect: None,
            effect_order: None,
            metadata: None,
            created_at: wf_common::now(),
            updated_at: wf_common::now(),
            create_checkpoint: None,
            checkpoint_description_template: None,
        };
        let event = BaseEvent {
            id: Id::from("evt-1"),
            r#type: EventType::NodeCompleted,
            timestamp: wf_common::now(),
            event_name: None,
            workflow_id: None,
            execution_id: None,
            agent_loop_id: None,
            metadata: None,
        };
        runner
            .run(&template, &event)
            .await
            .expect("cold start submits");
        let mut forwarded = None;
        for _ in 0..100 {
            if let Some(recorded) = *wf_common::lock::lock_ok(seen.lock()) {
                forwarded = recorded;
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        assert_eq!(forwarded, Some(3));
    }
}
