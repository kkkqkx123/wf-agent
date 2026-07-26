use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::broadcast;
use wf_types::events::{
    BaseEvent, CheckpointCreatedEvent, CheckpointDeletedEvent, CheckpointFailedEvent,
    CheckpointRestoredEvent, EventType,
};

const DEFAULT_CHANNEL_CAPACITY: usize = 256;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum CheckpointEvent {
    Created(CheckpointCreatedEvent),
    Restored(CheckpointRestoredEvent),
    Deleted(CheckpointDeletedEvent),
    Failed(CheckpointFailedEvent),
}

pub type EventSender = broadcast::Sender<CheckpointEvent>;
pub type EventReceiver = broadcast::Receiver<CheckpointEvent>;

#[derive(Clone)]
pub struct CheckpointEventBus {
    sender: Arc<EventSender>,
}

impl CheckpointEventBus {
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(DEFAULT_CHANNEL_CAPACITY);
        Self {
            sender: Arc::new(sender),
        }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self {
            sender: Arc::new(sender),
        }
    }

    pub fn subscribe(&self) -> EventReceiver {
        self.sender.subscribe()
    }

    pub fn publish(&self, event: CheckpointEvent) -> usize {
        self.sender.send(event).unwrap_or(0)
    }

    pub fn receiver_count(&self) -> usize {
        self.sender.receiver_count()
    }

    pub fn created(checkpoint_id: impl Into<String>) -> CheckpointEvent {
        CheckpointEvent::Created(CheckpointCreatedEvent {
            base: BaseEvent {
                id: uuid::Uuid::new_v4().to_string(),
                r#type: EventType::CheckpointCreated,
                timestamp: chrono::Utc::now().timestamp_millis(),
                workflow_id: None,
                execution_id: None,
                agent_loop_id: None,
                metadata: None,
            },
            checkpoint_id: checkpoint_id.into(),
            description: None,
        })
    }

    pub fn restored(checkpoint_id: impl Into<String>, execution_id: impl Into<String>) -> CheckpointEvent {
        let exec_id: String = execution_id.into();
        CheckpointEvent::Restored(CheckpointRestoredEvent {
            base: BaseEvent {
                id: uuid::Uuid::new_v4().to_string(),
                r#type: EventType::CheckpointRestored,
                timestamp: chrono::Utc::now().timestamp_millis(),
                workflow_id: None,
                execution_id: Some(exec_id.clone()),
                agent_loop_id: None,
                metadata: None,
            },
            checkpoint_id: checkpoint_id.into(),
            execution_id: exec_id,
            description: None,
        })
    }

    pub fn deleted(checkpoint_id: impl Into<String>) -> CheckpointEvent {
        CheckpointEvent::Deleted(CheckpointDeletedEvent {
            base: BaseEvent {
                id: uuid::Uuid::new_v4().to_string(),
                r#type: EventType::CheckpointDeleted,
                timestamp: chrono::Utc::now().timestamp_millis(),
                workflow_id: None,
                execution_id: None,
                agent_loop_id: None,
                metadata: None,
            },
            checkpoint_id: checkpoint_id.into(),
            reason: None,
        })
    }

    pub fn failed(operation: impl Into<String>, error: impl Into<String>) -> CheckpointEvent {
        CheckpointEvent::Failed(CheckpointFailedEvent {
            base: BaseEvent {
                id: uuid::Uuid::new_v4().to_string(),
                r#type: EventType::CheckpointFailed,
                timestamp: chrono::Utc::now().timestamp_millis(),
                workflow_id: None,
                execution_id: None,
                agent_loop_id: None,
                metadata: None,
            },
            checkpoint_id: None,
            operation: operation.into(),
            error: error.into(),
        })
    }
}

impl Default for CheckpointEventBus {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn created_event_has_correct_type() {
        let event = CheckpointEventBus::created("cp-1");
        match event {
            CheckpointEvent::Created(ref e) => {
                assert_eq!(e.checkpoint_id, "cp-1");
                assert_eq!(e.base.r#type, EventType::CheckpointCreated);
            }
            _ => panic!("expected Created event"),
        }
    }

    #[test]
    fn publish_and_receive() {
        let bus = CheckpointEventBus::new();
        let mut rx = bus.subscribe();

        bus.publish(CheckpointEventBus::created("cp-1"));
        let event = rx.try_recv().unwrap();

        match event {
            CheckpointEvent::Created(ref e) => assert_eq!(e.checkpoint_id, "cp-1"),
            _ => panic!("wrong event type"),
        }
    }

    #[test]
    fn multiple_subscribers() {
        let bus = CheckpointEventBus::new();
        let mut rx1 = bus.subscribe();
        let mut rx2 = bus.subscribe();

        bus.publish(CheckpointEventBus::deleted("cp-1"));

        assert!(rx1.try_recv().is_ok());
        assert!(rx2.try_recv().is_ok());
    }
}
