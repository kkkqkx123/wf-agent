use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use futures::Stream;
use tokio::sync::mpsc;

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
}

impl EventSubscriptionOptions {
    /// Subscribe to a single execution's lifecycle until a terminal event.
    pub fn for_execution(execution_id: impl Into<String>) -> Self {
        Self {
            execution_id: Some(execution_id.into()),
            ..Default::default()
        }
    }

    pub(crate) fn matches(&self, event: &BaseEvent) -> bool {
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
/// The subscription starts forwarding on construction; dropping it stops the
/// background forwarder on the next event. Consumable as a `Stream` or via
/// [`EventSubscription::next`].
pub struct EventSubscription {
    rx: mpsc::Receiver<BaseEvent>,
}

impl EventSubscription {
    pub fn new(rx: mpsc::Receiver<BaseEvent>) -> Self {
        Self { rx }
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
    tokio::spawn(async move {
        while let Ok(event) = sub.recv().await {
            if !filter.matches(&event) {
                continue;
            }
            let terminal = EventSubscriptionOptions::is_terminal(&event);
            if tx.send(event).await.is_err() {
                break;
            }
            if terminal {
                break;
            }
        }
    });
    EventSubscription::new(rx)
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
}
