use std::collections::HashSet;

use tokio::sync::broadcast;

use wf_types::events::{BaseEvent, EventType};

use crate::error::EventError;

const DEFAULT_CAPACITY: usize = 1024;

pub struct EventBus {
    sender: broadcast::Sender<BaseEvent>,
}

pub struct EventBusBuilder {
    capacity: usize,
}

pub struct Subscription {
    receiver: broadcast::Receiver<BaseEvent>,
    filter: EventFilter,
}

#[derive(Debug, Clone, Default)]
pub struct EventFilter {
    pub execution_id: Option<String>,
    pub event_types: Option<HashSet<EventType>>,
    pub workflow_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct StateTransition {
    pub from: String,
    pub to: String,
    pub timestamp: i64,
}

impl EventBus {
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self { sender }
    }

    pub fn builder() -> EventBusBuilder {
        EventBusBuilder {
            capacity: DEFAULT_CAPACITY,
        }
    }

    pub fn subscribe(&self, filter: EventFilter) -> Subscription {
        Subscription {
            receiver: self.sender.subscribe(),
            filter,
        }
    }

    pub fn subscribe_global(&self) -> Subscription {
        self.subscribe(EventFilter::default())
    }

    pub fn publish(&self, event: BaseEvent) -> Result<usize, EventError> {
        Ok(self.sender.send(event)?)
    }

    pub fn receiver_count(&self) -> usize {
        self.sender.receiver_count()
    }
}

impl EventBusBuilder {
    pub fn capacity(mut self, cap: usize) -> Self {
        self.capacity = cap;
        self
    }

    pub fn build(self) -> EventBus {
        EventBus::new(self.capacity)
    }
}

impl Subscription {
    pub async fn recv(&mut self) -> Result<BaseEvent, EventError> {
        loop {
            let event = self.receiver.recv().await?;
            if self.filter.matches(&event) {
                return Ok(event);
            }
        }
    }

    pub fn try_recv(&mut self) -> Result<BaseEvent, EventError> {
        loop {
            let event = self.receiver.try_recv()?;
            if self.filter.matches(&event) {
                return Ok(event);
            }
        }
    }

    pub fn filter(&self) -> &EventFilter {
        &self.filter
    }
}

impl EventFilter {
    pub fn matches(&self, event: &BaseEvent) -> bool {
        if let Some(ref eid) = self.execution_id {
            if event.execution_id.as_ref() != Some(eid) {
                return false;
            }
        }
        if let Some(ref wid) = self.workflow_id {
            if event.workflow_id.as_ref() != Some(wid) {
                return false;
            }
        }
        if let Some(ref types) = self.event_types {
            if !types.contains(&event.r#type) {
                return false;
            }
        }
        true
    }

    pub fn with_execution_id(mut self, execution_id: String) -> Self {
        self.execution_id = Some(execution_id);
        self
    }

    pub fn with_workflow_id(mut self, workflow_id: String) -> Self {
        self.workflow_id = Some(workflow_id);
        self
    }

    pub fn with_event_types(mut self, types: HashSet<EventType>) -> Self {
        self.event_types = Some(types);
        self
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
        let mut sub = bus.subscribe_global();

        let event = make_event(None, EventType::Heartbeat);
        bus.publish(event.clone()).unwrap();

        let received = sub.recv().await.unwrap();
        assert_eq!(received.r#type, EventType::Heartbeat);
    }

    #[tokio::test]
    async fn test_filter_by_execution_id() {
        let bus = EventBus::new(16);
        let filter = EventFilter::default().with_execution_id("exec-1".to_string());
        let mut sub = bus.subscribe(filter);

        bus.publish(make_event(Some("exec-2"), EventType::Heartbeat))
            .unwrap();
        bus.publish(make_event(Some("exec-1"), EventType::NodeStarted))
            .unwrap();

        let received = sub.recv().await.unwrap();
        assert_eq!(received.execution_id, Some("exec-1".to_string()));
        assert_eq!(received.r#type, EventType::NodeStarted);
    }

    #[tokio::test]
    async fn test_filter_by_event_type() {
        let bus = EventBus::new(16);
        let mut types = HashSet::new();
        types.insert(EventType::NodeStarted);
        types.insert(EventType::NodeCompleted);
        let filter = EventFilter::default().with_event_types(types);
        let mut sub = bus.subscribe(filter);

        bus.publish(make_event(None, EventType::Heartbeat)).unwrap();
        bus.publish(make_event(None, EventType::NodeStarted))
            .unwrap();

        let received = sub.recv().await.unwrap();
        assert_eq!(received.r#type, EventType::NodeStarted);
    }

    #[tokio::test]
    async fn test_subscribe_global_receives_all() {
        let bus = EventBus::new(16);
        let mut sub = bus.subscribe_global();

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
        let mut sub = bus.subscribe_global();

        bus.publish(make_event(None, EventType::Heartbeat)).unwrap();
        bus.publish(make_event(None, EventType::NodeStarted))
            .unwrap();

        let result = sub.try_recv();
        assert!(matches!(result, Err(EventError::Lagged(1))));
    }
}
