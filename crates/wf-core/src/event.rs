use tokio::sync::broadcast;

use wf_types::events::BaseEvent;

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

    pub fn subscribe(&self) -> Subscription {
        Subscription {
            receiver: self.sender.subscribe(),
        }
    }

    pub fn publish(&self, event: BaseEvent) -> Result<usize, EventError> {
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
}
