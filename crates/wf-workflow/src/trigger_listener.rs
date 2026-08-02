//! Event-driven trigger listener.
//!
//! Subscribes to the runtime EventBus and reacts to events by matching them
//! against registered trigger templates (mirrors the TS `TriggerListener`).
//! A matching `ExecuteTriggeredSubworkflow` action runs the configured
//! sub-workflow; the context-compression chain uses this to summarize an
//! over-limit *named message array*:
//!
//! 1. a `CONTEXT_COMPRESSION_REQUESTED` event carries `target_context_id`
//!    (the name of the message array to compress) and its `messages` snapshot;
//! 2. the listener runs the summary sub-workflow over that snapshot;
//! 3. the compressed result is written back to the same named array through
//!    the [`ContextWriter`] trait (the workflow scenario registers its
//!    execution variable map; the agent scenario its conversation session);
//! 4. a `CONTEXT_COMPRESSION_COMPLETED` event carries the compressed array
//!    so external consumers can reproduce the write-back.
//!
//! wf-workflow stays decoupled from wf-resource: templates are provided
//! through the [`TriggerTemplateRegistry`] trait, sub-workflow execution
//! through the [`SubworkflowRunner`] trait and write-back through the
//! [`ContextWriter`] trait, all implemented by wf-runtime during assembly.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use dashmap::DashMap;
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};
use wf_core::error::EventError;
use wf_core::EventBus;
use wf_types::events::BaseEvent;
use wf_types::message::{Message, MessageContent, MessageContentValue};
use wf_types::trigger::{TriggerAction, TriggerCondition, TriggerTemplate};

use crate::error::{WorkflowError, WorkflowResult};

/// Default timeout applied to a triggered sub-workflow when the action does
/// not configure one.
const DEFAULT_TRIGGER_TIMEOUT_MS: u64 = 60000;

/// Lookup source for trigger templates. Implemented by wf-runtime over the
/// wf-resource registrar, where the predefined templates are registered.
pub trait TriggerTemplateRegistry: Send + Sync {
    /// All enabled trigger templates to match events against.
    fn templates(&self) -> Vec<TriggerTemplate>;
}

/// Executes a triggered sub-workflow and returns its final output.
/// Implemented by wf-runtime over the workflow coordinator.
#[async_trait]
pub trait SubworkflowRunner: Send + Sync {
    async fn run(
        &self,
        workflow_id: &str,
        input: serde_json::Value,
    ) -> WorkflowResult<serde_json::Value>;
}

/// Writes a compressed message array back into the named context of the
/// emitting execution. Implemented by wf-runtime: the workflow scenario
/// resolves the execution variable map, the agent scenario its conversation
/// session. Returns whether the write-back succeeded (false when the target
/// execution no longer exists).
pub trait ContextWriter: Send + Sync {
    fn write_context(&self, execution_id: &str, context_id: &str, messages: Vec<Message>) -> bool;
}

/// Listens for events and executes matching trigger templates.
///
/// Compression is best-effort: failures are logged, never propagated to the
/// emitting execution. Events are handled concurrently: each matching
/// (execution, template) pair runs in its own task, so the in-flight guard
/// and `max_triggers` budget are meaningful even for events that arrive
/// back-to-back.
#[derive(Clone)]
pub struct TriggerEventListener {
    bus: Arc<EventBus>,
    registry: Arc<dyn TriggerTemplateRegistry>,
    runner: Arc<dyn SubworkflowRunner>,
    writer: Arc<dyn ContextWriter>,
    /// `execution_id:trigger_name` pairs with a run in flight.
    in_flight: DashMap<String, ()>,
    /// Per-template fire counts (only consulted when `max_triggers > 0`).
    trigger_counts: Arc<std::sync::Mutex<HashMap<String, u32>>>,
    shutdown: CancellationToken,
}

impl TriggerEventListener {
    pub fn new(
        bus: Arc<EventBus>,
        registry: Arc<dyn TriggerTemplateRegistry>,
        runner: Arc<dyn SubworkflowRunner>,
        writer: Arc<dyn ContextWriter>,
        shutdown: CancellationToken,
    ) -> Self {
        Self {
            bus,
            registry,
            runner,
            writer,
            in_flight: DashMap::new(),
            trigger_counts: Arc::new(std::sync::Mutex::new(HashMap::new())),
            shutdown,
        }
    }

    /// Run the listener loop until shutdown is requested.
    pub async fn run(&self) {
        let mut subscription = self.bus.subscribe();
        loop {
            tokio::select! {
                _ = self.shutdown.cancelled() => {
                    debug!("TriggerEventListener shutdown requested");
                    break;
                }
                event = subscription.recv() => match event {
                    Ok(event) => self.dispatch(event),
                    Err(EventError::Lagged(_)) => {
                        warn!("TriggerEventListener lagged behind the event bus");
                        continue;
                    }
                    Err(_) => break,
                },
            }
        }
    }

    /// Dispatch an event: match it against all templates and launch one task
    /// per matching (execution, template) pair. This runs synchronously in
    /// the listener loop, so the in-flight claim and `max_triggers` budget
    /// are race-free even for events that arrive back-to-back; only the
    /// sub-workflow execution happens in the spawned tasks.
    fn dispatch(&self, event: BaseEvent) {
        let Some(execution_id) = event.execution_id.clone() else {
            return;
        };

        for template in self.registry.templates() {
            if !template.enabled.unwrap_or(true) {
                continue;
            }
            let Some(condition) = &template.condition else {
                continue;
            };
            let Some(action) = &template.action else {
                continue;
            };
            if !self.matches(&event, condition) {
                continue;
            }

            let key = format!("{}:{}", execution_id, template.name);
            // Atomic re-entrancy guard: a present entry means a run for this
            // (execution, trigger) pair is already in flight.
            if self.in_flight.insert(key.clone(), ()).is_some() {
                debug!(
                    "Trigger '{}' already running for execution {}, skipping",
                    template.name, execution_id
                );
                continue;
            }
            if let Some(max) = template.max_triggers {
                if max > 0 {
                    let mut counts = self.trigger_counts.lock().unwrap();
                    let count = counts.entry(template.name.clone()).or_insert(0);
                    if *count >= max {
                        debug!(
                            "Trigger '{}' reached max_triggers ({}), skipping",
                            template.name, max
                        );
                        self.in_flight.remove(&key);
                        continue;
                    }
                    *count += 1;
                }
            }

            let listener = self.clone();
            let action = action.clone();
            let name = template.name.clone();
            let event = event.clone();
            tokio::spawn(async move {
                let outcome = listener.execute_trigger(&name, &action, &event).await;
                listener.in_flight.remove(&key);
                if let Err(e) = outcome {
                    warn!("Trigger '{}' failed: {}", name, e);
                }
            });
        }
    }

    /// Match an event against a trigger condition: the event type (canonical
    /// SCREAMING_SNAKE_CASE name, see [`wf_types::events::EventType::as_str`])
    /// must equal `condition.event_type`, and every configured metadata pair
    /// must be present in the event metadata.
    fn matches(&self, event: &BaseEvent, condition: &TriggerCondition) -> bool {
        if event.r#type.as_str() != condition.event_type {
            return false;
        }

        let Some(condition_metadata) = &condition.metadata else {
            return true;
        };
        let Some(event_metadata) = &event.metadata else {
            return false;
        };
        condition_metadata.iter().all(|(key, expected)| {
            event_metadata
                .get(key)
                .is_some_and(|value| value == expected)
        })
    }

    async fn execute_trigger(
        &self,
        name: &str,
        action: &TriggerAction,
        event: &BaseEvent,
    ) -> WorkflowResult<()> {
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
        let meta = match wf_llm::ContextCompressionRequestedMeta::try_from(event) {
            Ok(meta) => meta,
            Err(e) => {
                debug!(
                    "Trigger '{}' matched but the event is not a valid compression request: {}",
                    name, e
                );
                return Ok(());
            }
        };
        let target_context_id = meta.target_context_id;
        if meta.messages.is_empty() {
            debug!(
                "Trigger '{}' matched but the event carries no named message array, skipping",
                name
            );
            return Ok(());
        }
        let messages = meta.messages;

        let input = serde_json::json!({ "conversationHistory": messages });
        let wait = wait_for_completion.unwrap_or(true);
        let timeout_ms = timeout.unwrap_or(DEFAULT_TRIGGER_TIMEOUT_MS);

        if wait {
            let outcome = tokio::time::timeout(
                Duration::from_millis(timeout_ms),
                self.run_subworkflow(triggered_workflow_id, input, event, &target_context_id),
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
            // Fire-and-forget: the emitting execution must not wait.
            let runner = self.runner.clone();
            let writer = self.writer.clone();
            let bus = self.bus.clone();
            let workflow_id = triggered_workflow_id.clone();
            let event = event.clone();
            let target_context_id = target_context_id.clone();
            tokio::spawn(async move {
                match runner.run(&workflow_id, input).await {
                    Ok(output) => {
                        if let Err(e) = handle_subworkflow_output(
                            &writer,
                            &bus,
                            &event,
                            &target_context_id,
                            &output,
                        ) {
                            warn!(
                                "Triggered subworkflow '{}' completed but write-back failed: {}",
                                workflow_id, e
                            );
                        }
                    }
                    Err(e) => warn!("Triggered subworkflow '{}' failed: {}", workflow_id, e),
                }
            });
            Ok(())
        }
    }

    /// Run the sub-workflow, write the compressed array back to the named
    /// context of the emitting execution and emit the completed event.
    async fn run_subworkflow(
        &self,
        workflow_id: &str,
        input: serde_json::Value,
        event: &BaseEvent,
        target_context_id: &str,
    ) -> WorkflowResult<()> {
        let output = self.runner.run(workflow_id, input).await?;
        handle_subworkflow_output(&self.writer, &self.bus, event, target_context_id, &output)
    }
}

/// Write the compressed output back to the emitting execution and publish
/// the CONTEXT_COMPRESSION_COMPLETED event.
fn handle_subworkflow_output(
    writer: &Arc<dyn ContextWriter>,
    bus: &Arc<EventBus>,
    event: &BaseEvent,
    target_context_id: &str,
    output: &serde_json::Value,
) -> WorkflowResult<()> {
    let messages: Vec<Message> = serde_json::from_value(output.clone()).unwrap_or_default();
    if messages.is_empty() {
        return Err(WorkflowError::TriggerError(
            "Compression sub-workflow returned no messages".to_string(),
        ));
    }
    if let Some(execution_id) = &event.execution_id {
        if !writer.write_context(execution_id, target_context_id, messages.clone()) {
            warn!(
                "Context write-back failed for execution {} context {}",
                execution_id, target_context_id
            );
        }
    }
    let completed = build_compression_completed_event(event, target_context_id, &messages);
    let _ = bus.publish(completed);
    Ok(())
}

/// Build the CONTEXT_COMPRESSION_COMPLETED event from the compressed message
/// array (messageOutputs of the summary workflow): the array itself, the
/// summary text and the estimated token count.
fn build_compression_completed_event(
    event: &BaseEvent,
    target_context_id: &str,
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
        event.execution_id.as_deref().unwrap_or_default(),
        event.agent_loop_id.as_deref(),
        target_context_id,
        summary.as_deref(),
        tokens_after,
        Some(messages),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    use wf_types::events::EventType;

    fn text_message(role: wf_types::message::MessageRole, text: &str) -> Message {
        Message {
            id: wf_types::Id::new(),
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

    fn compression_template(name: &str, max_triggers: u32) -> TriggerTemplate {
        TriggerTemplate {
            name: name.to_string(),
            description: None,
            condition: Some(TriggerCondition {
                event_type: "CONTEXT_COMPRESSION_REQUESTED".to_string(),
                event_name: None,
                condition: None,
                metadata: None,
            }),
            action: Some(TriggerAction::ExecuteTriggeredSubworkflow {
                triggered_workflow_id: "summary_flow".to_string(),
                wait_for_completion: Some(true),
                timeout: Some(1000),
                input_mapping: None,
                output_mapping: None,
            }),
            enabled: Some(true),
            max_triggers: Some(max_triggers),
            metadata: None,
            created_at: 0,
            updated_at: 0,
            create_checkpoint: None,
            checkpoint_description_template: None,
        }
    }

    struct StaticRegistry(Vec<TriggerTemplate>);

    impl TriggerTemplateRegistry for StaticRegistry {
        fn templates(&self) -> Vec<TriggerTemplate> {
            self.0.clone()
        }
    }

    struct RecordingRunner {
        calls: Arc<AtomicU32>,
        latest_input: Arc<std::sync::Mutex<Option<serde_json::Value>>>,
        delay_ms: u64,
    }

    #[async_trait]
    impl SubworkflowRunner for RecordingRunner {
        async fn run(
            &self,
            workflow_id: &str,
            input: serde_json::Value,
        ) -> WorkflowResult<serde_json::Value> {
            assert_eq!(workflow_id, "summary_flow");
            if self.delay_ms > 0 {
                tokio::time::sleep(Duration::from_millis(self.delay_ms)).await;
            }
            self.calls.fetch_add(1, Ordering::SeqCst);
            *self.latest_input.lock().unwrap() = Some(input);
            Ok(serde_json::json!([{
                "id": "m1",
                "role": "assistant",
                "content": {"type": "text", "text": "compressed summary"},
                "timestamp": 0
            }]))
        }
    }

    #[derive(Default)]
    #[allow(clippy::type_complexity)]
    struct RecordingWriter {
        writes: Arc<std::sync::Mutex<Vec<(String, String, Vec<Message>)>>>,
    }

    impl ContextWriter for RecordingWriter {
        fn write_context(
            &self,
            execution_id: &str,
            context_id: &str,
            messages: Vec<Message>,
        ) -> bool {
            self.writes.lock().unwrap().push((
                execution_id.to_string(),
                context_id.to_string(),
                messages,
            ));
            true
        }
    }

    fn requested_event(execution_id: &str, target: Option<&str>) -> BaseEvent {
        wf_llm::build_context_compression_requested_event(
            execution_id,
            Some("loop-1"),
            target.unwrap_or("chat"),
            1200,
            1000,
            1,
            Some(&[text_message(
                wf_types::message::MessageRole::User,
                "long conversation",
            )]),
        )
    }

    /// The listener's subscription is created when its spawned task first
    /// polls; publishing before that loses the event. Wait until the bus
    /// sees the expected number of receivers before publishing.
    async fn wait_for_listener(bus: &EventBus, expected_receivers: usize) {
        while bus.receiver_count() < expected_receivers {
            tokio::task::yield_now().await;
        }
    }

    /// Poll a condition until it holds (2s budget) so async assertions do
    /// not depend on fixed sleeps.
    async fn wait_until(cond: impl Fn() -> bool) {
        for _ in 0..200 {
            if cond() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    #[tokio::test]
    async fn matching_trigger_runs_subworkflow_writes_back_and_emits_completed() {
        let bus = Arc::new(EventBus::new(64));
        let mut sub = bus.subscribe();
        let registry: Arc<dyn TriggerTemplateRegistry> =
            Arc::new(StaticRegistry(vec![compression_template("ctx", 0)]));
        let calls = Arc::new(AtomicU32::new(0));
        let latest_input = Arc::new(std::sync::Mutex::new(None));
        let runner: Arc<dyn SubworkflowRunner> = Arc::new(RecordingRunner {
            calls: calls.clone(),
            latest_input: latest_input.clone(),
            delay_ms: 0,
        });
        let writer = RecordingWriter::default();
        let writes = writer.writes.clone();
        let writer: Arc<dyn ContextWriter> = Arc::new(writer);
        let listener = Arc::new(TriggerEventListener::new(
            bus.clone(),
            registry,
            runner,
            writer,
            CancellationToken::new(),
        ));
        let handle = tokio::spawn({
            let listener = listener.clone();
            async move { listener.run().await }
        });

        // The test's own subscription plus the listener's.
        wait_for_listener(&bus, 2).await;
        bus.publish(requested_event("exec-1", Some("chat")))
            .unwrap();

        // Wait for the completion event (skipping the requested event itself).
        let completed = loop {
            match sub.recv().await {
                Ok(event) if event.r#type == EventType::ContextCompressionCompleted => break event,
                Ok(_) => continue,
                Err(_) => panic!("event bus closed"),
            }
        };
        assert_eq!(completed.execution_id.as_deref(), Some("exec-1"));
        assert_eq!(completed.agent_loop_id.as_deref(), Some("loop-1"));
        let completed_meta =
            wf_llm::ContextCompressionCompletedMeta::try_from(&completed).unwrap();
        assert_eq!(completed_meta.target_context_id, "chat");
        assert_eq!(completed_meta.summary.as_deref(), Some("compressed summary"));
        assert!(
            completed_meta.tokens_after > 0,
            "compressed messages must be tokenizable"
        );
        assert!(
            completed_meta.tokens_after < 100,
            "compressed summary must be small"
        );
        assert_eq!(
            completed_meta.messages.len(),
            1,
            "completed event carries the compressed array"
        );

        // The sub-workflow ran with the conversation payload as input.
        let input = latest_input
            .lock()
            .unwrap()
            .clone()
            .expect("runner must have recorded the input");
        assert_eq!(
            input["conversationHistory"][0]["content"],
            serde_json::json!("long conversation")
        );
        assert_eq!(calls.load(Ordering::SeqCst), 1);

        // The compressed array was written back to the named context.
        {
            let writes = writes.lock().unwrap();
            assert_eq!(writes.len(), 1);
            assert_eq!(writes[0].0, "exec-1");
            assert_eq!(writes[0].1, "chat");
            assert_eq!(writes[0].2.len(), 1);
        }

        listener.shutdown.cancel();
        let _ = tokio::time::timeout(Duration::from_secs(2), handle).await;
    }

    #[tokio::test]
    async fn event_without_named_array_skips_execution() {
        let bus = Arc::new(EventBus::new(64));
        let registry: Arc<dyn TriggerTemplateRegistry> =
            Arc::new(StaticRegistry(vec![compression_template("ctx", 0)]));
        let calls = Arc::new(AtomicU32::new(0));
        let runner: Arc<dyn SubworkflowRunner> = Arc::new(RecordingRunner {
            calls: calls.clone(),
            latest_input: Arc::new(std::sync::Mutex::new(None)),
            delay_ms: 0,
        });
        let writer: Arc<dyn ContextWriter> = Arc::new(RecordingWriter::default());
        let listener = Arc::new(TriggerEventListener::new(
            bus.clone(),
            registry,
            runner,
            writer,
            CancellationToken::new(),
        ));
        let handle = tokio::spawn({
            let listener = listener.clone();
            async move { listener.run().await }
        });

        // Missing target_context_id (no named array): must be skipped.
        wait_for_listener(&bus, 1).await;
        let mut bare = requested_event("exec-2", None);
        bare.metadata = None;
        bus.publish(bare).unwrap();

        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(calls.load(Ordering::SeqCst), 0, "no named array -> no run");

        listener.shutdown.cancel();
        let _ = tokio::time::timeout(Duration::from_secs(2), handle).await;
    }

    #[tokio::test]
    async fn duplicate_event_is_skipped_while_in_flight() {
        let bus = Arc::new(EventBus::new(64));
        let registry: Arc<dyn TriggerTemplateRegistry> =
            Arc::new(StaticRegistry(vec![compression_template("ctx", 0)]));
        let calls = Arc::new(AtomicU32::new(0));
        let runner: Arc<dyn SubworkflowRunner> = Arc::new(RecordingRunner {
            calls: calls.clone(),
            latest_input: Arc::new(std::sync::Mutex::new(None)),
            delay_ms: 200,
        });
        let writer: Arc<dyn ContextWriter> = Arc::new(RecordingWriter::default());
        let listener = Arc::new(TriggerEventListener::new(
            bus.clone(),
            registry,
            runner,
            writer,
            CancellationToken::new(),
        ));
        let handle = tokio::spawn({
            let listener = listener.clone();
            async move { listener.run().await }
        });

        // Two identical compression requests for the same execution: the
        // second must be skipped while the first is in flight.
        wait_for_listener(&bus, 1).await;
        bus.publish(requested_event("exec-3", Some("chat")))
            .unwrap();
        bus.publish(requested_event("exec-3", Some("chat")))
            .unwrap();

        // Wait for the first run to finish, then give a would-be second run
        // (which must not happen) room to surface.
        wait_until(|| calls.load(Ordering::SeqCst) == 1).await;
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert_eq!(
            calls.load(Ordering::SeqCst),
            1,
            "re-entrancy must be prevented"
        );

        listener.shutdown.cancel();
        let _ = tokio::time::timeout(Duration::from_secs(2), handle).await;
    }

    #[tokio::test]
    async fn max_triggers_limits_per_template() {
        let bus = Arc::new(EventBus::new(64));
        let registry: Arc<dyn TriggerTemplateRegistry> =
            Arc::new(StaticRegistry(vec![compression_template("ctx", 1)]));
        let calls = Arc::new(AtomicU32::new(0));
        let runner: Arc<dyn SubworkflowRunner> = Arc::new(RecordingRunner {
            calls: calls.clone(),
            latest_input: Arc::new(std::sync::Mutex::new(None)),
            delay_ms: 0,
        });
        let writer: Arc<dyn ContextWriter> = Arc::new(RecordingWriter::default());
        let listener = Arc::new(TriggerEventListener::new(
            bus.clone(),
            registry,
            runner,
            writer,
            CancellationToken::new(),
        ));
        let handle = tokio::spawn({
            let listener = listener.clone();
            async move { listener.run().await }
        });

        // Sequential (non-concurrent) events: the second execution must be
        // dropped by the max_triggers=1 budget.
        wait_for_listener(&bus, 1).await;
        bus.publish(requested_event("exec-4", Some("chat")))
            .unwrap();
        wait_until(|| calls.load(Ordering::SeqCst) == 1).await;
        bus.publish(requested_event("exec-4", Some("chat")))
            .unwrap();
        tokio::time::sleep(Duration::from_millis(200)).await;

        assert_eq!(
            calls.load(Ordering::SeqCst),
            1,
            "max_triggers must cap runs"
        );

        listener.shutdown.cancel();
        let _ = tokio::time::timeout(Duration::from_secs(2), handle).await;
    }

    #[test]
    fn event_type_serialization_matches_template_condition() {
        let event = BaseEvent {
            id: wf_types::Id::new(),
            r#type: EventType::ContextCompressionRequested,
            timestamp: 0,
            workflow_id: None,
            execution_id: Some("x".to_string()),
            agent_loop_id: None,
            metadata: None,
        };
        let condition = TriggerCondition {
            event_type: "CONTEXT_COMPRESSION_REQUESTED".to_string(),
            event_name: None,
            condition: None,
            metadata: None,
        };
        let listener = TriggerEventListener::new(
            Arc::new(EventBus::new(4)),
            Arc::new(StaticRegistry(Vec::new())),
            Arc::new(RecordingRunner {
                calls: Arc::new(AtomicU32::new(0)),
                latest_input: Arc::new(std::sync::Mutex::new(None)),
                delay_ms: 0,
            }),
            Arc::new(RecordingWriter::default()),
            CancellationToken::new(),
        );
        assert!(listener.matches(&event, &condition));
        assert!(!listener.matches(
            &BaseEvent {
                r#type: EventType::TokenLimitExceeded,
                ..event
            },
            &condition
        ));
    }
}
