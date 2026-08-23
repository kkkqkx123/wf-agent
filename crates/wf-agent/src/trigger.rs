use std::collections::HashMap;
use std::sync::Arc;

use dashmap::DashMap;
use serde_json::Value;
use tracing::warn;

use wf_core::EventBus;
use wf_execution_shared::hooks::HookRegistry;
use wf_execution_shared::types::execution_entity::IExecutionEntity;
use wf_tools::callback::{AgentLoopConfig, AgentLoopInput, AgentLoopOutput};
use wf_types::hook::{SUBAGENT_START, SUBAGENT_STOP};
use wf_types::message::{Message, MessageContentValue, MessageRole};
use wf_types::trigger::{ConversationAnchor, TriggerAgentInputMode, TriggerAgentWriteback};
use wf_types::Id;

use crate::entity::AgentLoopEntity;
use crate::error::{AgentError, AgentResult};
use crate::hook::AgentHookHandler;

/// Callback that runs a child agent loop (usually backed by
/// AgentLoopExecutor).
pub type AgentExecutorCallback = Arc<
    dyn Fn(
            AgentLoopConfig,
            AgentLoopInput,
        ) -> futures::future::BoxFuture<'static, AgentResult<AgentLoopOutput>>
        + Send
        + Sync,
>;

/// Configuration for a triggered (nested) agent execution.
#[derive(Clone)]
pub struct TriggeredAgentExecutionConfig {
    /// Parent execution entity that triggered the child agent.
    pub parent: Arc<AgentLoopEntity>,
    /// Variable name on the parent into which the child result is written
    /// (always performed; the data-loss fall-back of the conversation
    /// write-back modes).
    pub result_variable: String,
    /// Whether to wait for completion (sync vs async fire-and-forget). In
    /// the agent scenario this only decides whether the runner blocks on
    /// the child submission/completion; the parent loop never synchronizes
    /// with the child (async injection: the write-back lands in the parent
    /// conversation and the next parent LLM request reads it).
    pub wait_for_completion: bool,
    /// Max child execution time in ms.
    pub timeout_ms: Option<u64>,
    /// Turn anchor captured at the trigger point: the parent conversation
    /// position/version the child ran against (write-back validation).
    pub anchor: Option<ConversationAnchor>,
    /// How the parent conversation snapshot feeds the child.
    pub input_mode: TriggerAgentInputMode,
    /// Where the child result is written back.
    pub writeback: TriggerAgentWriteback,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TriggeredTaskSubmission {
    pub task_id: String,
    pub status: String,
    pub submit_time: i64,
}

/// Manages triggered (nested) agent loop executions started from a trigger
/// event. Children are registered on the parent entity and their results are
/// written back into the parent's variable snapshots; a failing child never
/// fails the parent.
pub struct TriggeredAgentExecutionManager {
    executor: AgentExecutorCallback,
    /// In-flight background child executions keyed by task id. Populated when
    /// a fire-and-forget child is spawned and removed when it finishes (or is
    /// cancelled with its parent), so `running_count` reflects reality.
    running_tasks: Arc<DashMap<String, ()>>,
    /// Hook receiver registry: `SUBAGENT_START` / `SUBAGENT_STOP` are
    /// dispatched against the parent entity's hook configuration.
    hook_registry: Option<Arc<HookRegistry>>,
    /// Event bus for the `HOOK_TRIGGERED` audit copies (optional).
    event_bus: Option<Arc<EventBus>>,
}

impl TriggeredAgentExecutionManager {
    pub fn new(executor: AgentExecutorCallback) -> Self {
        Self {
            executor,
            running_tasks: Arc::new(DashMap::new()),
            hook_registry: None,
            event_bus: None,
        }
    }

    pub fn with_hook_registry(mut self, registry: Arc<HookRegistry>) -> Self {
        self.hook_registry = Some(registry);
        self
    }

    pub fn with_event_bus(mut self, bus: Arc<EventBus>) -> Self {
        self.event_bus = Some(bus);
        self
    }

    /// Submit a triggered child agent execution.
    ///
    /// - `wait_for_completion == true`: returns the child output.
    /// - otherwise: spawns the child in the background and returns a task
    ///   submission descriptor.
    pub async fn submit_triggered_execution(
        &self,
        config: TriggeredAgentExecutionConfig,
        child_config: AgentLoopConfig,
        child_input: AgentLoopInput,
    ) -> AgentResult<TriggeredTaskSubmission> {
        let task_id = wf_common::generate_id();
        let parent = config.parent.clone();

        // Register the child on the parent entity. The child entity id is a
        // newly generated execution id (not the agent definition id): reusing
        // the definition id would make every trigger of the same agent share
        // one execution identity, causing partition mixing in file
        // checkpointing (each execution must be its own actor).
        let child_execution_id = Id::from(wf_common::generate_id());
        // Seed the child's ancestor chain from the live parent so nested
        // triggered runs keep full ancestry (parent's chain + parent id).
        let mut child_ancestors = parent.get_ancestors();
        if child_ancestors.last() != Some(parent.id()) {
            child_ancestors.push(parent.id().clone());
        }
        let child_entity = AgentLoopEntity::new(child_execution_id)
            .with_parent_execution_id(parent.id().clone())
            .with_ancestors(child_ancestors);
        parent.register_child(child_entity.id().clone()).await;

        // SUBAGENT_START: child registered and about to run; mounted on the
        // parent entity's hook configuration.
        let child_execution_id = child_entity.id().clone();
        let child_agent_id = child_config.agent_id.clone();
        let mut start_data = HashMap::new();
        start_data.insert(
            "child_execution_id".to_string(),
            Value::String(child_execution_id.clone()),
        );
        start_data.insert(
            "agent_id".to_string(),
            Value::String(child_agent_id.clone()),
        );
        start_data.insert(
            "prompt".to_string(),
            Value::String(child_input.message.clone()),
        );
        start_data.insert(
            "wait_for_completion".to_string(),
            Value::Bool(config.wait_for_completion),
        );
        AgentHookHandler::emit_agent_hooks(
            &parent,
            SUBAGENT_START,
            start_data,
            self.hook_registry.as_deref(),
            self.event_bus.as_deref(),
        )
        .await;

        let submission = TriggeredTaskSubmission {
            task_id: task_id.clone(),
            status: "QUEUED".to_string(),
            submit_time: wf_common::now(),
        };

        if config.wait_for_completion {
            let parent_for_run = parent.clone();
            let result = self
                .execute_child(
                    parent_for_run,
                    child_entity,
                    child_config,
                    child_input,
                    config.result_variable,
                    config.timeout_ms,
                    config.writeback,
                    config.anchor,
                )
                .await;
            // SUBAGENT_STOP: child finished (success or failure); mounted on
            // the parent entity's hook configuration.
            let mut stop_data = HashMap::new();
            stop_data.insert(
                "child_execution_id".to_string(),
                Value::String(child_execution_id),
            );
            stop_data.insert("agent_id".to_string(), Value::String(child_agent_id));
            match &result {
                Ok(value) => {
                    stop_data.insert("success".to_string(), Value::Bool(true));
                    stop_data.insert("result".to_string(), value.clone());
                }
                Err(e) => {
                    stop_data.insert("success".to_string(), Value::Bool(false));
                    stop_data.insert("error".to_string(), Value::String(e.to_string()));
                }
            }
            AgentHookHandler::emit_agent_hooks(
                &parent,
                SUBAGENT_STOP,
                stop_data,
                self.hook_registry.as_deref(),
                self.event_bus.as_deref(),
            )
            .await;
            match result {
                Ok(_) => Ok(submission),
                Err(e) => Err(e),
            }
        } else {
            let parent_clone = parent.clone();
            let executor = self.executor.clone();
            let parent_token = parent.get_abort_signal();
            let running_tasks = self.running_tasks.clone();
            let hook_registry = self.hook_registry.clone();
            let event_bus = self.event_bus.clone();
            let result_variable = config.result_variable.clone();
            let writeback = config.writeback;
            let anchor = config.anchor;
            running_tasks.insert(task_id.clone(), ());
            tokio::spawn(async move {
                let child_run = executor(child_config, child_input);
                let outcome = tokio::select! {
                    run = child_run => match run {
                        Ok(output) => {
                            write_back_result(
                                &parent_clone,
                                &result_variable,
                                writeback,
                                anchor,
                                &output,
                                event_bus.as_deref(),
                            )
                            .await;
                            (true, None, Some(output.result))
                        }
                        Err(e) => (false, Some(e.to_string()), None),
                    },
                    _ = parent_token.cancelled() => {
                        (false, Some("parent aborted".to_string()), None)
                    }
                };
                parent_clone.unregister_child(child_entity.id()).await;
                running_tasks.remove(&task_id);
                // SUBAGENT_STOP: background child finished (success,
                // failure, or parent abort).
                let mut stop_data = HashMap::new();
                stop_data.insert(
                    "child_execution_id".to_string(),
                    Value::String(child_execution_id),
                );
                stop_data.insert("agent_id".to_string(), Value::String(child_agent_id));
                stop_data.insert("success".to_string(), Value::Bool(outcome.0));
                if let Some(error) = outcome.1 {
                    stop_data.insert("error".to_string(), Value::String(error));
                }
                if let Some(result) = outcome.2 {
                    stop_data.insert("result".to_string(), result);
                }
                AgentHookHandler::emit_agent_hooks(
                    &parent_clone,
                    SUBAGENT_STOP,
                    stop_data,
                    hook_registry.as_deref(),
                    event_bus.as_deref(),
                )
                .await;
            });

            Ok(submission)
        }
    }

    /// Run a child agent to completion, write its result back into the
    /// parent and unregister it. A child failure is reported back but does
    /// not touch the parent's execution state.
    #[allow(clippy::too_many_arguments)]
    async fn execute_child(
        &self,
        parent: Arc<AgentLoopEntity>,
        child_entity: AgentLoopEntity,
        child_config: AgentLoopConfig,
        child_input: AgentLoopInput,
        result_variable: String,
        timeout_ms: Option<u64>,
        writeback: TriggerAgentWriteback,
        anchor: Option<ConversationAnchor>,
    ) -> AgentResult<Value> {
        let mut future = Box::pin((self.executor)(child_config, child_input));
        let output = match timeout_ms {
            Some(ms) if ms > 0 => {
                match tokio::time::timeout(std::time::Duration::from_millis(ms), &mut future).await
                {
                    Ok(result) => result,
                    Err(_) => {
                        parent.unregister_child(child_entity.id()).await;
                        return Err(AgentError::ExecutionError(format!(
                            "Triggered agent execution '{}' timed out after {}ms",
                            child_entity.id(),
                            ms
                        )));
                    }
                }
            }
            _ => future.await,
        };

        let output = match output {
            Ok(out) => out,
            Err(e) => {
                parent.unregister_child(child_entity.id()).await;
                return Err(e);
            }
        };

        write_back_result(
            &parent,
            &result_variable,
            writeback,
            anchor,
            &output,
            self.event_bus.as_deref(),
        )
        .await;
        parent.unregister_child(child_entity.id()).await;
        Ok(output.result)
    }

    /// Number of background child executions currently in flight.
    pub fn running_count(&self) -> usize {
        self.running_tasks.len()
    }
}

/// Write the child result back per the configured mode:
///
/// - the parent variable snapshot always receives `output.result`
///   (data-loss fall-back: a discarded or skipped conversation write-back
///   never loses the result);
/// - `ConversationReplace` / `ConversationAppend` additionally publish a
///   `CONVERSATION_WRITEBACK_COMPLETED` event carrying the anchor version;
///   the parent session consumer applies it only while the conversation is
///   still at that version (stale results are discarded, mirroring the
///   compression path).
///
/// The child final reply becomes a single `assistant` message by default;
/// full child-conversation injection is a future extension.
async fn write_back_result(
    parent: &Arc<AgentLoopEntity>,
    result_variable: &str,
    writeback: TriggerAgentWriteback,
    anchor: Option<ConversationAnchor>,
    output: &AgentLoopOutput,
    event_bus: Option<&wf_core::EventBus>,
) {
    parent
        .state
        .write()
        .await
        .set_variable_snapshot(result_variable.to_string(), output.result.clone());
    if writeback == TriggerAgentWriteback::Variable {
        return;
    }
    let Some(bus) = event_bus else {
        warn!(
            "Conversation write-back skipped for parent {}: no event bus (variable fall-back kept)",
            parent.id()
        );
        return;
    };
    let Some(anchor) = anchor else {
        warn!(
            "Conversation write-back skipped for parent {}: trigger event carries no anchor (variable fall-back kept)",
            parent.id()
        );
        return;
    };
    let operation = match writeback {
        TriggerAgentWriteback::ConversationReplace => wf_llm::WRITEBACK_OPERATION_REPLACE,
        TriggerAgentWriteback::ConversationAppend => wf_llm::WRITEBACK_OPERATION_APPEND,
        TriggerAgentWriteback::Variable => return,
    };
    let message = assistant_message_from_result(&output.result);
    let event = wf_llm::build_conversation_writeback_completed_event(
        parent.id(),
        Some(parent.id()),
        wf_llm::CONVERSATION_CONTEXT_ID,
        anchor.array_version,
        operation,
        std::slice::from_ref(&message),
    );
    let _ = bus.publish(event);
}

/// Convert the child final reply into one `assistant` message for
/// conversation write-back.
fn assistant_message_from_result(result: &Value) -> Message {
    let content = match result {
        Value::String(text) => text.clone(),
        other => serde_json::to_string(other).unwrap_or_else(|_| "null".to_string()),
    };
    Message {
        id: wf_common::generate_id(),
        role: MessageRole::Assistant,
        content: MessageContentValue::Text(content),
        timestamp: wf_common::now(),
        tool_call_id: None,
        tool_name: None,
        tool_calls: None,
        thinking: None,
        metadata: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use wf_types::Id;

    fn make_parent() -> Arc<AgentLoopEntity> {
        Arc::new(AgentLoopEntity::new(Id::from("parent-1".to_string())))
    }

    fn child_config(id: &str) -> AgentLoopConfig {
        AgentLoopConfig {
            agent_id: Id::from(id.to_string()),
            model: "mock".to_string(),
            max_iterations: Some(5),
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
        }
    }

    fn success_executor(result: Value) -> AgentExecutorCallback {
        Arc::new(move |_config, _input| {
            let result = result.clone();
            Box::pin(async move {
                Ok(AgentLoopOutput {
                    agent_loop_id: Id::from("child".to_string()),
                    result,
                    iterations: 1,
                    conversation: Vec::new(),
                })
            })
        })
    }

    fn failing_executor() -> AgentExecutorCallback {
        Arc::new(|_config, _input| {
            Box::pin(async move { Err(AgentError::ExecutionError("child boom".to_string())) })
        })
    }

    #[tokio::test]
    async fn test_sync_triggered_execution_writes_result() {
        let parent = make_parent();
        let manager =
            TriggeredAgentExecutionManager::new(success_executor(Value::from("child ok")));

        let submission = manager
            .submit_triggered_execution(
                TriggeredAgentExecutionConfig {
                    parent: parent.clone(),
                    result_variable: "trigger_result".to_string(),
                    wait_for_completion: true,
                    timeout_ms: Some(5000),
                    anchor: None,
                    input_mode: Default::default(),
                    writeback: Default::default(),
                },
                child_config("child-1"),
                AgentLoopInput {
                    message: "run".to_string(),
                    context: std::collections::HashMap::new(),
                    conversation: Vec::new(),
                },
            )
            .await
            .expect("sync child must succeed");

        assert_eq!(submission.status, "QUEUED");

        let state = parent.state.read().await;
        let snapshots = state.variable_snapshots();
        assert_eq!(
            snapshots.get("trigger_result"),
            Some(&Value::from("child ok"))
        );
        drop(state);
        assert_eq!(parent.child_execution_ids().read().await.len(), 0);
    }

    #[tokio::test]
    async fn test_child_failure_does_not_fail_parent() {
        let parent = make_parent();
        let manager = TriggeredAgentExecutionManager::new(failing_executor());

        let result = manager
            .submit_triggered_execution(
                TriggeredAgentExecutionConfig {
                    parent: parent.clone(),
                    result_variable: "trigger_result".to_string(),
                    wait_for_completion: true,
                    timeout_ms: Some(5000),
                    anchor: None,
                    input_mode: Default::default(),
                    writeback: Default::default(),
                },
                child_config("child-2"),
                AgentLoopInput {
                    message: "run".to_string(),
                    context: std::collections::HashMap::new(),
                    conversation: Vec::new(),
                },
            )
            .await;

        assert!(result.is_err());
        // Parent state untouched, child unregistered.
        assert!(!parent.state.read().await.is_failed());
        assert_eq!(parent.child_execution_ids().read().await.len(), 0);
    }

    #[tokio::test]
    async fn test_async_triggered_execution_submits_immediately() {
        let parent = make_parent();
        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();
        let executor: AgentExecutorCallback = Arc::new(move |_config, _input| {
            let counter = counter_clone.clone();
            Box::pin(async move {
                counter.fetch_add(1, Ordering::SeqCst);
                Ok(AgentLoopOutput {
                    agent_loop_id: Id::from("child".to_string()),
                    result: Value::Null,
                    iterations: 1,
                    conversation: Vec::new(),
                })
            })
        });
        let manager = TriggeredAgentExecutionManager::new(executor);

        let submission = manager
            .submit_triggered_execution(
                TriggeredAgentExecutionConfig {
                    parent: parent.clone(),
                    result_variable: "trigger_result".to_string(),
                    wait_for_completion: false,
                    timeout_ms: None,
                    anchor: None,
                    input_mode: Default::default(),
                    writeback: Default::default(),
                },
                child_config("child-3"),
                AgentLoopInput {
                    message: "run".to_string(),
                    context: std::collections::HashMap::new(),
                    conversation: Vec::new(),
                },
            )
            .await
            .expect("async submission must succeed");

        assert_eq!(submission.status, "QUEUED");
        assert!(!submission.task_id.is_empty());

        // Wait for the background task to finish.
        for _ in 0..50 {
            if counter.load(Ordering::SeqCst) > 0
                && parent.child_execution_ids().read().await.is_empty()
            {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        assert_eq!(counter.load(Ordering::SeqCst), 1);
        assert_eq!(parent.child_execution_ids().read().await.len(), 0);
    }

    #[tokio::test]
    async fn test_child_timeout() {
        let parent = make_parent();
        let executor: AgentExecutorCallback = Arc::new(|_config, _input| {
            Box::pin(async move {
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                Ok(AgentLoopOutput {
                    agent_loop_id: Id::from("child".to_string()),
                    result: Value::Null,
                    iterations: 1,
                    conversation: Vec::new(),
                })
            })
        });
        let manager = TriggeredAgentExecutionManager::new(executor);

        let result = manager
            .submit_triggered_execution(
                TriggeredAgentExecutionConfig {
                    parent: parent.clone(),
                    result_variable: "trigger_result".to_string(),
                    wait_for_completion: true,
                    timeout_ms: Some(30),
                    anchor: None,
                    input_mode: Default::default(),
                    writeback: Default::default(),
                },
                child_config("child-4"),
                AgentLoopInput {
                    message: "run".to_string(),
                    context: std::collections::HashMap::new(),
                    conversation: Vec::new(),
                },
            )
            .await;

        assert!(result.is_err());
        assert_eq!(parent.child_execution_ids().read().await.len(), 0);
    }

    /// Records hook dispatches into a shared log (hook types for ordering,
    /// contexts for data assertions).
    #[allow(clippy::type_complexity)]
    struct HookRecorder {
        name: &'static str,
        log: Arc<std::sync::Mutex<Vec<String>>>,
        contexts: Arc<std::sync::Mutex<Vec<(String, HashMap<String, Value>)>>>,
    }

    #[async_trait::async_trait]
    impl wf_execution_shared::hooks::HookReceiver for HookRecorder {
        fn name(&self) -> &str {
            self.name
        }

        async fn on_hook(
            &self,
            ctx: &wf_execution_shared::hooks::HookContext,
        ) -> wf_execution_shared::hooks::HookOutcome {
            self.log.lock().unwrap().push(ctx.hook_type.clone());
            self.contexts
                .lock()
                .unwrap()
                .push((ctx.hook_type.clone(), ctx.data.clone()));
            wf_execution_shared::hooks::HookOutcome::Continue
        }
    }

    #[allow(clippy::type_complexity)]
    fn recorder_pair(
        log: &Arc<std::sync::Mutex<Vec<String>>>,
        contexts: &Arc<std::sync::Mutex<Vec<(String, HashMap<String, Value>)>>>,
    ) -> (
        Arc<wf_execution_shared::hooks::HookRegistry>,
        Vec<Arc<HookRecorder>>,
    ) {
        let registry = Arc::new(wf_execution_shared::hooks::HookRegistry::new());
        let start = Arc::new(HookRecorder {
            name: "rec_start",
            log: log.clone(),
            contexts: contexts.clone(),
        });
        let stop = Arc::new(HookRecorder {
            name: "rec_stop",
            log: log.clone(),
            contexts: contexts.clone(),
        });
        registry.register(wf_types::hook::SUBAGENT_START, start.clone(), 0);
        registry.register(wf_types::hook::SUBAGENT_STOP, stop.clone(), 0);
        (registry, vec![start, stop])
    }

    #[tokio::test]
    async fn test_subagent_start_stop_sync_lifecycle_order_and_data() {
        let bus = Arc::new(wf_core::EventBus::new(32));
        let log = Arc::new(std::sync::Mutex::new(Vec::new()));
        let contexts = Arc::new(std::sync::Mutex::new(Vec::new()));
        let (registry, _recorders) = recorder_pair(&log, &contexts);

        // The executor callback records when the child actually runs.
        let log_for_child = log.clone();
        let executor: AgentExecutorCallback = Arc::new(move |_config, _input| {
            let log = log_for_child.clone();
            Box::pin(async move {
                log.lock().unwrap().push("child-ran".to_string());
                Ok(AgentLoopOutput {
                    agent_loop_id: Id::from("child".to_string()),
                    result: Value::from("child ok"),
                    iterations: 1,
                    conversation: Vec::new(),
                })
            })
        });
        let manager = TriggeredAgentExecutionManager::new(executor)
            .with_hook_registry(registry)
            .with_event_bus(bus);
        let parent = make_parent();

        let submission = manager
            .submit_triggered_execution(
                TriggeredAgentExecutionConfig {
                    parent: parent.clone(),
                    result_variable: "trigger_result".to_string(),
                    wait_for_completion: true,
                    timeout_ms: Some(5000),
                    anchor: None,
                    input_mode: Default::default(),
                    writeback: Default::default(),
                },
                child_config("child-5"),
                AgentLoopInput {
                    message: "summarize the hook".to_string(),
                    context: std::collections::HashMap::new(),
                    conversation: Vec::new(),
                },
            )
            .await
            .expect("sync child must succeed");

        assert_eq!(submission.status, "QUEUED");

        // Order: START (child registered, before it runs) -> child ran ->
        // STOP.
        let events = log.lock().unwrap();
        assert_eq!(
            *events,
            vec!["SUBAGENT_START", "child-ran", "SUBAGENT_STOP"],
            "START must fire before the child runs, STOP after"
        );
        drop(events);

        let recorded = contexts.lock().unwrap();
        let start = recorded
            .iter()
            .find(|(t, _)| t == wf_types::hook::SUBAGENT_START)
            .expect("SUBAGENT_START recorded");
        let start_child_id = start
            .1
            .get("child_execution_id")
            .and_then(|v| v.as_str())
            .expect("child_execution_id present");
        assert!(
            !start_child_id.is_empty(),
            "child execution id must be a generated id"
        );
        assert_ne!(
            start_child_id, "child-5",
            "child id must not reuse the agent definition id"
        );
        assert_eq!(
            start.1.get("agent_id").and_then(|v| v.as_str()),
            Some("child-5"),
            "agent_id still reports the agent definition id"
        );
        assert_eq!(
            start.1.get("prompt").and_then(|v| v.as_str()),
            Some("summarize the hook")
        );
        assert_eq!(start.1.get("wait_for_completion"), Some(&Value::Bool(true)));

        let stop = recorded
            .iter()
            .find(|(t, _)| t == wf_types::hook::SUBAGENT_STOP)
            .expect("SUBAGENT_STOP recorded");
        assert_eq!(stop.1.get("success"), Some(&Value::Bool(true)));
        assert_eq!(stop.1.get("result"), Some(&Value::from("child ok")));
        assert_eq!(stop.1.get("error"), None);
    }

    #[tokio::test]
    async fn test_subagent_stop_reports_failure() {
        let bus = Arc::new(wf_core::EventBus::new(32));
        let log = Arc::new(std::sync::Mutex::new(Vec::new()));
        let contexts = Arc::new(std::sync::Mutex::new(Vec::new()));
        let (registry, _recorders) = recorder_pair(&log, &contexts);
        let manager = TriggeredAgentExecutionManager::new(failing_executor())
            .with_hook_registry(registry)
            .with_event_bus(bus);
        let parent = make_parent();

        let result = manager
            .submit_triggered_execution(
                TriggeredAgentExecutionConfig {
                    parent: parent.clone(),
                    result_variable: "trigger_result".to_string(),
                    wait_for_completion: true,
                    timeout_ms: Some(5000),
                    anchor: None,
                    input_mode: Default::default(),
                    writeback: Default::default(),
                },
                child_config("child-6"),
                AgentLoopInput {
                    message: "run".to_string(),
                    context: std::collections::HashMap::new(),
                    conversation: Vec::new(),
                },
            )
            .await;
        assert!(result.is_err());

        let recorded = contexts.lock().unwrap();
        let stop = recorded
            .iter()
            .find(|(t, _)| t == wf_types::hook::SUBAGENT_STOP)
            .expect("SUBAGENT_STOP recorded on failure");
        assert_eq!(stop.1.get("success"), Some(&Value::Bool(false)));
        assert!(
            stop.1
                .get("error")
                .and_then(|v| v.as_str())
                .is_some_and(|e| e.contains("child boom")),
            "failure error must be surfaced: {:?}",
            stop.1.get("error")
        );
    }

    #[tokio::test]
    async fn test_subagent_stop_fires_for_background_child() {
        let bus = Arc::new(wf_core::EventBus::new(32));
        let log = Arc::new(std::sync::Mutex::new(Vec::new()));
        let contexts = Arc::new(std::sync::Mutex::new(Vec::new()));
        let (registry, _recorders) = recorder_pair(&log, &contexts);
        let manager = TriggeredAgentExecutionManager::new(success_executor(Value::from("ok")))
            .with_hook_registry(registry)
            .with_event_bus(bus);
        let parent = make_parent();

        manager
            .submit_triggered_execution(
                TriggeredAgentExecutionConfig {
                    parent: parent.clone(),
                    result_variable: "trigger_result".to_string(),
                    wait_for_completion: false,
                    timeout_ms: None,
                    anchor: None,
                    input_mode: Default::default(),
                    writeback: Default::default(),
                },
                child_config("child-7"),
                AgentLoopInput {
                    message: "run".to_string(),
                    context: std::collections::HashMap::new(),
                    conversation: Vec::new(),
                },
            )
            .await
            .expect("async submission must succeed");

        // The background child settles and emits SUBAGENT_STOP.
        for _ in 0..50 {
            let has_stop = contexts
                .lock()
                .unwrap()
                .iter()
                .any(|(t, _)| t == wf_types::hook::SUBAGENT_STOP);
            if has_stop {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        let recorded = contexts.lock().unwrap();
        let stop = recorded
            .iter()
            .find(|(t, _)| t == wf_types::hook::SUBAGENT_STOP)
            .expect("SUBAGENT_STOP must fire for the background child");
        assert_eq!(stop.1.get("success"), Some(&Value::Bool(true)));
        // The child execution id must be a newly generated id, never the
        // agent definition id: reusing it would make repeated triggers of
        // the same agent share one file-checkpoint actor partition.
        let child_id = stop
            .1
            .get("child_execution_id")
            .and_then(|v| v.as_str())
            .expect("child_execution_id present");
        assert!(!child_id.is_empty());
        assert_ne!(child_id, "child-7");
    }

    #[tokio::test]
    async fn test_conversation_append_writeback_publishes_anchored_event() {
        use wf_types::events::EventType;
        use wf_types::trigger::ConversationAnchor;

        let bus = Arc::new(wf_core::EventBus::new(32));
        let mut sub = bus.subscribe();
        let parent = make_parent();
        // Seed the parent conversation; the anchor points at its current
        // position.
        parent
            .conversation()
            .write()
            .await
            .add_message(wf_types::message::Message {
                id: wf_common::generate_id(),
                role: MessageRole::User,
                content: MessageContentValue::Text("parent context".to_string()),
                timestamp: wf_common::now(),
                tool_call_id: None,
                tool_name: None,
                tool_calls: None,
                thinking: None,
                metadata: None,
            });
        let (message_count, array_version) = {
            let conv = parent.conversation().read().await;
            (conv.messages().len(), conv.conversation_version())
        };
        let manager =
            TriggeredAgentExecutionManager::new(success_executor(Value::from("child ok")))
                .with_event_bus(bus);

        manager
            .submit_triggered_execution(
                TriggeredAgentExecutionConfig {
                    parent: parent.clone(),
                    result_variable: "trigger_result".to_string(),
                    wait_for_completion: true,
                    timeout_ms: Some(5000),
                    anchor: Some(ConversationAnchor {
                        message_count,
                        array_version,
                    }),
                    input_mode: TriggerAgentInputMode::PrefixToAnchor,
                    writeback: TriggerAgentWriteback::ConversationAppend,
                },
                child_config("child-wb"),
                AgentLoopInput {
                    message: "run".to_string(),
                    context: std::collections::HashMap::new(),
                    conversation: Vec::new(),
                },
            )
            .await
            .expect("sync child must succeed");

        // The write-back event carries the anchor version and the child
        // result as one assistant message.
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
        assert_eq!(meta.target_context_id, "conversation");
        assert_eq!(
            writeback_event.execution_id.as_deref(),
            Some(parent.id().as_str())
        );
        assert_eq!(meta.messages.len(), 1);
        assert_eq!(meta.messages[0].role, MessageRole::Assistant);
        assert_eq!(
            meta.messages[0].content,
            MessageContentValue::Text("child ok".to_string())
        );

        // The variable fall-back write-back still happened.
        let state = parent.state.read().await;
        assert_eq!(
            state.variable_snapshots().get("trigger_result"),
            Some(&Value::from("child ok"))
        );
    }

    #[tokio::test]
    async fn test_conversation_writeback_skipped_without_anchor() {
        use wf_types::events::EventType;

        let bus = Arc::new(wf_core::EventBus::new(32));
        let mut sub = bus.subscribe();
        let parent = make_parent();
        let manager =
            TriggeredAgentExecutionManager::new(success_executor(Value::from("child ok")))
                .with_event_bus(bus);

        manager
            .submit_triggered_execution(
                TriggeredAgentExecutionConfig {
                    parent: parent.clone(),
                    result_variable: "trigger_result".to_string(),
                    wait_for_completion: true,
                    timeout_ms: Some(5000),
                    anchor: None,
                    input_mode: TriggerAgentInputMode::FullSnapshot,
                    writeback: TriggerAgentWriteback::ConversationAppend,
                },
                child_config("child-wb2"),
                AgentLoopInput {
                    message: "run".to_string(),
                    context: std::collections::HashMap::new(),
                    conversation: Vec::new(),
                },
            )
            .await
            .expect("sync child must succeed");

        // No conversation write-back event without an anchor.
        let observed = tokio::time::timeout(std::time::Duration::from_millis(200), async {
            loop {
                match sub.recv().await {
                    Ok(event) if event.r#type == EventType::ConversationWritebackCompleted => {
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
            !observed,
            "write-back must be skipped when the anchor is missing"
        );

        // The variable fall-back keeps the result observable.
        let state = parent.state.read().await;
        assert_eq!(
            state.variable_snapshots().get("trigger_result"),
            Some(&Value::from("child ok"))
        );
    }

    #[tokio::test]
    async fn test_async_child_also_writes_back_result() {
        use std::sync::atomic::{AtomicU32, Ordering};

        let parent = make_parent();
        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();
        let executor: AgentExecutorCallback = Arc::new(move |_config, _input| {
            let counter = counter_clone.clone();
            Box::pin(async move {
                counter.fetch_add(1, Ordering::SeqCst);
                Ok(AgentLoopOutput {
                    agent_loop_id: Id::from("child".to_string()),
                    result: Value::from("async child ok"),
                    iterations: 1,
                    conversation: Vec::new(),
                })
            })
        });
        let manager = TriggeredAgentExecutionManager::new(executor);

        manager
            .submit_triggered_execution(
                TriggeredAgentExecutionConfig {
                    parent: parent.clone(),
                    result_variable: "trigger_result".to_string(),
                    wait_for_completion: false,
                    timeout_ms: None,
                    anchor: None,
                    input_mode: TriggerAgentInputMode::PrefixToAnchor,
                    writeback: TriggerAgentWriteback::Variable,
                },
                child_config("child-async-wb"),
                AgentLoopInput {
                    message: "run".to_string(),
                    context: std::collections::HashMap::new(),
                    conversation: Vec::new(),
                },
            )
            .await
            .expect("async submission must succeed");

        // The fire-and-forget child settles and writes back the variable.
        for _ in 0..50 {
            let written = parent
                .state
                .read()
                .await
                .variable_snapshots()
                .get("trigger_result")
                .cloned();
            if written.is_some() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        let state = parent.state.read().await;
        assert_eq!(
            state.variable_snapshots().get("trigger_result"),
            Some(&Value::from("async child ok")),
            "fire-and-forget children must write back the result too"
        );
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }
}
