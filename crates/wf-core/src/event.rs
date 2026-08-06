use std::collections::VecDeque;
use std::sync::Mutex;

use tokio::sync::broadcast;

use wf_types::events::BaseEvent;

use crate::error::EventError;

const DEFAULT_CAPACITY: usize = 1024;

/// Number of most recent events kept for search/observability. Bounded so
/// unbounded publish rates never grow memory.
const DEFAULT_RECENT_LIMIT: usize = 512;

pub struct EventBus {
    sender: broadcast::Sender<BaseEvent>,
    /// Ring buffer of the most recent published events, serving the unified
    /// search's `event` type and lightweight observability queries.
    recent: Mutex<VecDeque<BaseEvent>>,
    recent_limit: usize,
}

pub struct EventBusBuilder {
    capacity: usize,
    recent_limit: usize,
}

pub struct Subscription {
    receiver: broadcast::Receiver<BaseEvent>,
}

#[derive(Debug, Clone)]
pub struct StateTransition {
    pub from: String,
    pub to: String,
    pub timestamp: i64,
}

impl EventBus {
    pub fn new(capacity: usize) -> Self {
        Self {
            sender: broadcast::channel(capacity).0,
            recent: Mutex::new(VecDeque::with_capacity(DEFAULT_RECENT_LIMIT)),
            recent_limit: DEFAULT_RECENT_LIMIT,
        }
    }

    /// Build a bus with an explicit recent-event history size.
    pub fn with_recent_limit(capacity: usize, recent_limit: usize) -> Self {
        Self {
            sender: broadcast::channel(capacity).0,
            recent: Mutex::new(VecDeque::with_capacity(recent_limit)),
            recent_limit,
        }
    }

    pub fn builder() -> EventBusBuilder {
        EventBusBuilder {
            capacity: DEFAULT_CAPACITY,
            recent_limit: DEFAULT_RECENT_LIMIT,
        }
    }

    pub fn subscribe(&self) -> Subscription {
        Subscription {
            receiver: self.sender.subscribe(),
        }
    }

    pub fn publish(&self, event: BaseEvent) -> Result<usize, EventError> {
        self.record_recent(event.clone());
        Ok(self.sender.send(event)?)
    }

    pub fn receiver_count(&self) -> usize {
        self.sender.receiver_count()
    }

    /// Number of events sent but not yet received by any subscriber
    /// (backlog depth of the broadcast channel).
    pub fn queue_len(&self) -> usize {
        self.sender.len()
    }

    /// Maximum number of events retained in the recent-event history.
    pub fn recent_limit(&self) -> usize {
        self.recent_limit
    }

    /// Most recent published events, newest first, up to `recent_limit`.
    pub fn recent_events(&self) -> Vec<BaseEvent> {
        self.recent
            .lock()
            .expect("event bus recent history lock poisoned")
            .iter()
            .rev()
            .cloned()
            .collect()
    }

    fn record_recent(&self, event: BaseEvent) {
        let mut recent = self
            .recent
            .lock()
            .expect("event bus recent history lock poisoned");
        if recent.len() == self.recent_limit {
            recent.pop_front();
        }
        recent.push_back(event);
    }
}

impl EventBusBuilder {
    pub fn capacity(mut self, cap: usize) -> Self {
        self.capacity = cap;
        self
    }

    pub fn recent_limit(mut self, limit: usize) -> Self {
        self.recent_limit = limit;
        self
    }

    pub fn build(self) -> EventBus {
        EventBus::with_recent_limit(self.capacity, self.recent_limit)
    }
}

impl Subscription {
    pub async fn recv(&mut self) -> Result<BaseEvent, EventError> {
        Ok(self.receiver.recv().await?)
    }

    pub fn try_recv(&mut self) -> Result<BaseEvent, EventError> {
        Ok(self.receiver.try_recv()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_types::events::{BaseEvent, EventType};

    fn make_event(execution_id: Option<&str>, event_type: EventType) -> BaseEvent {
        BaseEvent {
            id: "test-id".to_string(),
            r#type: event_type,
            timestamp: 0,
            workflow_id: None,
            execution_id: execution_id.map(|s| s.to_string()),
            agent_loop_id: None,
            metadata: None,
        }
    }

    #[tokio::test]
    async fn test_publish_and_receive() {
        let bus = EventBus::new(16);
        let mut sub = bus.subscribe();

        let event = make_event(None, EventType::Heartbeat);
        bus.publish(event.clone()).unwrap();

        let received = sub.recv().await.unwrap();
        assert_eq!(received.r#type, EventType::Heartbeat);
    }

    #[tokio::test]
    async fn test_subscribe_global_receives_all() {
        let bus = EventBus::new(16);
        let mut sub = bus.subscribe();

        bus.publish(make_event(None, EventType::Heartbeat)).unwrap();
        bus.publish(make_event(None, EventType::NodeStarted))
            .unwrap();

        let r1 = sub.recv().await.unwrap();
        let r2 = sub.recv().await.unwrap();
        assert_eq!(r1.r#type, EventType::Heartbeat);
        assert_eq!(r2.r#type, EventType::NodeStarted);
    }

    #[tokio::test]
    async fn test_builder() {
        let bus = EventBus::builder().capacity(64).build();
        assert_eq!(bus.receiver_count(), 0);
    }

    #[tokio::test]
    async fn test_try_recv_lagged() {
        let bus = EventBus::new(1);
        let mut sub = bus.subscribe();

        bus.publish(make_event(None, EventType::Heartbeat)).unwrap();
        bus.publish(make_event(None, EventType::NodeStarted))
            .unwrap();

        let result = sub.try_recv();
        assert!(matches!(result, Err(EventError::Lagged(1))));
    }

    #[tokio::test]
    async fn recent_events_are_retained_newest_first() {
        let bus = EventBus::new(16);
        let _sub = bus.subscribe();
        bus.publish(make_event(Some("exec-1"), EventType::Heartbeat))
            .unwrap();
        bus.publish(make_event(Some("exec-2"), EventType::NodeStarted))
            .unwrap();
        bus.publish(make_event(Some("exec-3"), EventType::NodeCompleted))
            .unwrap();

        let recent = bus.recent_events();
        assert_eq!(recent.len(), 3);
        assert_eq!(recent[0].execution_id.as_deref(), Some("exec-3"));
        assert_eq!(recent[2].execution_id.as_deref(), Some("exec-1"));
    }

    #[tokio::test]
    async fn recent_events_are_bounded() {
        let bus = EventBus::with_recent_limit(16, 3);
        let _sub = bus.subscribe();
        for i in 0..10 {
            bus.publish(make_event(Some(&format!("exec-{i}")), EventType::Heartbeat))
                .unwrap();
        }
        let recent = bus.recent_events();
        assert_eq!(recent.len(), 3);
        assert_eq!(recent[0].execution_id.as_deref(), Some("exec-9"));
        assert_eq!(recent[2].execution_id.as_deref(), Some("exec-7"));
    }
}
