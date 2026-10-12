use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use futures::Stream;
use tokio::sync::mpsc::{self, error::TrySendError};

use wf_core::EventBus;
use wf_types::events::{BaseEvent, EventType};

use crate::infra::error::ApiResult;

/// Filter for a live event subscription (SSE/WS friendly).
#[derive(Debug, Clone, Default)]
pub struct EventSubscriptionOptions {
    /// Only events belonging to this workflow execution.
    pub execution_id: Option<String>,
    /// Only events belonging to this agent loop.
    pub agent_loop_id: Option<String>,
    /// Only events referencing this workflow.
    pub workflow_id: Option<String>,
    /// Only events of these types; `None` delivers every type.
    pub event_types: Option<Vec<EventType>>,
    /// Only checkpoint-domain events (`Checkpoint*`). When true, execution
    /// and agent events are excluded even without an explicit type list.
    /// Lets the checkpoint page subscribe without mixing execution noise.
    pub checkpoint_only: bool,
    /// Only checkpoint events for this file path (matches `file` metadata
    /// or the comma-separated `conflict_files` list).
    pub file_path: Option<String>,
    /// Only checkpoint events for this snapshot or commit id (matches
    /// `snapshot_id` / `checkpoint_id` metadata).
    pub snapshot_id: Option<String>,
}

impl EventSubscriptionOptions {
    /// Subscribe to a single execution's lifecycle until a terminal event.
    pub fn for_execution(execution_id: impl Into<String>) -> Self {
        Self {
            execution_id: Some(execution_id.into()),
            ..Default::default()
        }
    }

    /// Subscribe to checkpoint-domain changes only (file changes, merges,
    /// GC). Optionally narrowed to one file path.
    pub fn for_checkpoint(file_path: Option<String>) -> Self {
        Self {
            checkpoint_only: true,
            file_path,
            ..Default::default()
        }
    }

    /// Whether an event type belongs to the checkpoint domain.
    pub fn is_checkpoint_event(event_type: &EventType) -> bool {
        matches!(
            event_type,
            EventType::CheckpointCreated
                | EventType::CheckpointRestored
                | EventType::CheckpointDeleted
                | EventType::CheckpointFailed
                | EventType::CheckpointFileChanged
                | EventType::CheckpointMergeConflicted
                | EventType::CheckpointGcCompleted
        )
    }

    pub(crate) fn matches(&self, event: &BaseEvent) -> bool {
        if self.checkpoint_only && !Self::is_checkpoint_event(&event.r#type) {
            return false;
        }
        if let Some(execution_id) = &self.execution_id {
            if event.execution_id.as_deref() != Some(execution_id.as_str()) {
                return false;
            }
        }
        if let Some(agent_loop_id) = &self.agent_loop_id {
            if event.agent_loop_id.as_deref() != Some(agent_loop_id.as_str()) {
                return false;
            }
        }
        if let Some(workflow_id) = &self.workflow_id {
            if event.workflow_id.as_deref() != Some(workflow_id.as_str()) {
                return false;
            }
        }
        if let Some(types) = &self.event_types {
            if !types.contains(&event.r#type) {
                return false;
            }
        }
        if let Some(path) = &self.file_path {
            let meta = event.metadata.as_ref();
            let file_hit = meta
                .and_then(|m| m.get("file"))
                .and_then(|v| v.as_str())
                .is_some_and(|f| f == path.as_str());
            let conflict_hit =
                meta.and_then(|m| m.get("conflict_files"))
                    .is_some_and(|v| match v {
                        serde_json::Value::String(s) => {
                            s.split(',').any(|p| p.trim() == path.as_str())
                        }
                        serde_json::Value::Array(items) => items
                            .iter()
                            .filter_map(|i| i.as_str())
                            .any(|p| p == path.as_str()),
                        _ => false,
                    });
            if !(file_hit || conflict_hit) {
                return false;
            }
        }
        if let Some(snapshot) = &self.snapshot_id {
            let meta = event.metadata.as_ref();
            let hit = meta
                .and_then(|m| m.get("snapshot_id").or_else(|| m.get("checkpoint_id")))
                .and_then(|v| v.as_str())
                .is_some_and(|s| s == snapshot.as_str());
            if !hit {
                return false;
            }
        }
        true
    }

    pub(crate) fn is_terminal(event: &BaseEvent) -> bool {
        matches!(
            event.r#type,
            EventType::WorkflowExecutionCompleted
                | EventType::WorkflowExecutionFailed
                | EventType::WorkflowExecutionCancelled
                | EventType::AgentCompleted
                | EventType::AgentFailed
                | EventType::AgentCancelled
        )
    }
}

/// Async subscription over the shared `EventBus`, delivered through an mpsc
/// channel so multiple consumers each receive every matching event (broadcast
/// semantics are handled by `wf_core::EventBus`).
///
/// The subscription starts forwarding on construction; dropping it aborts the
/// background forwarder immediately, releasing its registered broadcast
/// receiver and mpsc sender. Consumable as a `Stream` or via
/// [`EventSubscription::next`].
pub struct EventSubscription {
    rx: mpsc::Receiver<BaseEvent>,
    task: tokio::task::JoinHandle<()>,
}

impl EventSubscription {
    pub fn new(rx: mpsc::Receiver<BaseEvent>, task: tokio::task::JoinHandle<()>) -> Self {
        Self { rx, task }
    }

    /// Await the next matching event; `None` when the subscription was closed.
    pub async fn next(&mut self) -> Option<BaseEvent> {
        self.rx.recv().await
    }

    /// Poll for a buffered event without awaiting.
    pub fn try_next(&mut self) -> Option<BaseEvent> {
        self.rx.try_recv().ok()
    }
}

impl Drop for EventSubscription {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl Stream for EventSubscription {
    type Item = BaseEvent;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.rx).poll_recv(cx)
    }
}

/// Spawn a background forwarder delivering matching events from `bus` into a
/// fresh subscription channel.
pub fn spawn_event_subscription(
    bus: Arc<EventBus>,
    options: &EventSubscriptionOptions,
) -> EventSubscription {
    let (tx, rx) = mpsc::channel(256);
    let filter = options.clone();
    let mut sub = bus.subscribe();
    let task = tokio::spawn(async move {
        while let Ok(event) = sub.recv().await {
            if !filter.matches(&event) {
                continue;
            }
            let terminal = EventSubscriptionOptions::is_terminal(&event);
            if terminal {
                // Terminal events must always reach a slow consumer, otherwise
                // the subscription never ends.
                if tx.send(event).await.is_err() {
                    break;
                }
                break;
            }
            // Never stall the broadcast receiver on a slow consumer: blocking
            // here would lag the bus and silently drop events for this
            // subscriber. Drop overflow instead (broadcast-lag semantics).
            match tx.try_send(event) {
                Ok(()) => {}
                Err(TrySendError::Closed(_)) => break,
                Err(TrySendError::Full(_)) => {}
            }
        }
    });
    EventSubscription::new(rx, task)
}

/// Await the first event matching `options`, bounded by `timeout`. Returns
/// `None` when the window elapses without a match.
pub async fn wait_for_event(
    bus: Arc<EventBus>,
    options: &EventSubscriptionOptions,
    timeout: Duration,
) -> ApiResult<Option<BaseEvent>> {
    let mut sub = spawn_event_subscription(bus, options);
    match tokio::time::timeout(timeout, sub.next()).await {
        Ok(Some(event)) => Ok(Some(event)),
        Ok(None) => Ok(None),
        Err(_) => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_common::generate_id;

    fn make_event(
        execution_id: Option<&str>,
        agent_loop_id: Option<&str>,
        event_type: EventType,
        ts: i64,
    ) -> BaseEvent {
        BaseEvent {
            id: generate_id(),
            r#type: event_type,
            timestamp: ts,
            workflow_id: None,
            execution_id: execution_id.map(ToOwned::to_owned),
            agent_loop_id: agent_loop_id.map(ToOwned::to_owned),
            event_name: None,
            metadata: None,
        }
    }

    #[tokio::test]
    async fn subscription_delivers_matching_events_and_stops_on_terminal() {
        let bus = Arc::new(EventBus::new(64));
        let mut sub = spawn_event_subscription(
            bus.clone(),
            &EventSubscriptionOptions::for_execution("exec-1"),
        );

        bus.publish(make_event(
            Some("exec-1"),
            None,
            EventType::NodeStarted,
            100,
        ))
        .unwrap();
        bus.publish(make_event(
            Some("exec-2"),
            None,
            EventType::NodeStarted,
            200,
        ))
        .unwrap();
        bus.publish(make_event(
            Some("exec-1"),
            None,
            EventType::WorkflowExecutionCompleted,
            300,
        ))
        .unwrap();

        let first = sub.next().await.unwrap();
        assert_eq!(first.r#type, EventType::NodeStarted);
        let second = sub.next().await.unwrap();
        assert_eq!(second.r#type, EventType::WorkflowExecutionCompleted);
    }

    #[tokio::test]
    async fn wait_for_event_returns_on_match() {
        let bus = Arc::new(EventBus::new(64));
        let mut options = EventSubscriptionOptions::for_execution("exec-t");
        options.event_types = Some(vec![EventType::AgentCompleted]);

        let handle = tokio::spawn({
            let bus = bus.clone();
            async move {
                wait_for_event(bus, &options, Duration::from_secs(5))
                    .await
                    .unwrap()
            }
        });
        tokio::time::sleep(Duration::from_millis(20)).await;
        bus.publish(make_event(
            Some("exec-t"),
            Some("agent-t"),
            EventType::AgentCompleted,
            1,
        ))
        .unwrap();

        let event = handle.await.unwrap().expect("event within window");
        assert_eq!(event.r#type, EventType::AgentCompleted);
    }

    #[tokio::test]
    async fn wait_for_event_times_out_without_match() {
        let bus = Arc::new(EventBus::new(64));
        let result = wait_for_event(
            bus,
            &EventSubscriptionOptions::for_execution("never"),
            Duration::from_millis(50),
        )
        .await
        .unwrap();
        assert!(result.is_none());
    }

    fn checkpoint_event(
        event_type: EventType,
        file: Option<&str>,
        snapshot: Option<&str>,
    ) -> BaseEvent {
        let mut metadata = std::collections::HashMap::new();
        if let Some(file) = file {
            metadata.insert("file".to_string(), serde_json::json!(file));
        }
        if let Some(snapshot) = snapshot {
            metadata.insert("snapshot_id".to_string(), serde_json::json!(snapshot));
        }
        BaseEvent {
            id: generate_id(),
            r#type: event_type,
            timestamp: 1,
            workflow_id: None,
            execution_id: None,
            agent_loop_id: None,
            event_name: None,
            metadata: Some(metadata),
        }
    }

    #[test]
    fn checkpoint_only_filter_excludes_execution_noise() {
        let options = EventSubscriptionOptions::for_checkpoint(None);
        assert!(options.matches(&checkpoint_event(
            EventType::CheckpointFileChanged,
            Some("a.txt"),
            None
        )));
        assert!(options.matches(&checkpoint_event(
            EventType::CheckpointGcCompleted,
            None,
            None
        )));
        assert!(!options.matches(&make_event(
            Some("exec-1"),
            None,
            EventType::NodeStarted,
            100
        )));
    }

    #[test]
    fn checkpoint_file_and_snapshot_narrowing() {
        let by_file = EventSubscriptionOptions::for_checkpoint(Some("a.txt".to_string()));
        assert!(by_file.matches(&checkpoint_event(
            EventType::CheckpointFileChanged,
            Some("a.txt"),
            None
        )));
        assert!(!by_file.matches(&checkpoint_event(
            EventType::CheckpointFileChanged,
            Some("b.txt"),
            None
        )));

        let mut by_snapshot = EventSubscriptionOptions::for_checkpoint(None);
        by_snapshot.snapshot_id = Some("snap-1".to_string());
        assert!(by_snapshot.matches(&checkpoint_event(
            EventType::CheckpointFileChanged,
            Some("a.txt"),
            Some("snap-1")
        )));
        assert!(!by_snapshot.matches(&checkpoint_event(
            EventType::CheckpointFileChanged,
            Some("a.txt"),
            Some("snap-2")
        )));
    }
}
