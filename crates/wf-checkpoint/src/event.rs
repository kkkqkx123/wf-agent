use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::broadcast;
use wf_types::events::{BaseEvent, EventType};

const DEFAULT_CHANNEL_CAPACITY: usize = 256;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CheckpointData {
    pub checkpoint_id: Option<String>,
    pub execution_id: Option<String>,
    pub operation: Option<String>,
    pub error: Option<String>,
    pub description: Option<String>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum CheckpointEvent {
    Created {
        base: BaseEvent,
        data: CheckpointData,
    },
    Restored {
        base: BaseEvent,
        data: CheckpointData,
    },
    Deleted {
        base: BaseEvent,
        data: CheckpointData,
    },
    Failed {
        base: BaseEvent,
        data: CheckpointData,
    },
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

    fn make_base(event_type: EventType) -> BaseEvent {
        BaseEvent {
            id: wf_common::generate_id(),
            r#type: event_type,
            timestamp: chrono::Utc::now().timestamp_millis(),
            workflow_id: None,
            execution_id: None,
            agent_loop_id: None,
            metadata: None,
        }
    }

    pub fn created(checkpoint_id: impl Into<String>) -> CheckpointEvent {
        Self::created_with(checkpoint_id, None, None)
    }

    /// Create a `Created` event with execution id and description filled in.
    pub fn created_with(
        checkpoint_id: impl Into<String>,
        execution_id: Option<String>,
        description: Option<String>,
    ) -> CheckpointEvent {
        CheckpointEvent::Created {
            base: Self::make_base(EventType::CheckpointCreated),
            data: CheckpointData {
                checkpoint_id: Some(checkpoint_id.into()),
                execution_id,
                operation: None,
                error: None,
                description,
                reason: None,
            },
        }
    }

    pub fn restored(
        checkpoint_id: impl Into<String>,
        execution_id: impl Into<String>,
    ) -> CheckpointEvent {
        let mut base = Self::make_base(EventType::CheckpointRestored);
        base.execution_id = Some(execution_id.into());
        CheckpointEvent::Restored {
            base,
            data: CheckpointData {
                checkpoint_id: Some(checkpoint_id.into()),
                execution_id: None,
                operation: None,
                error: None,
                description: None,
                reason: None,
            },
        }
    }

    pub fn deleted(checkpoint_id: impl Into<String>) -> CheckpointEvent {
        Self::deleted_with(checkpoint_id, None)
    }

    /// Create a `Deleted` event carrying the deletion reason
    /// (`"manual" | "cleanup" | "policy"`, aligned with TS).
    pub fn deleted_with(
        checkpoint_id: impl Into<String>,
        reason: Option<String>,
    ) -> CheckpointEvent {
        CheckpointEvent::Deleted {
            base: Self::make_base(EventType::CheckpointDeleted),
            data: CheckpointData {
                checkpoint_id: Some(checkpoint_id.into()),
                execution_id: None,
                operation: None,
                error: None,
                description: None,
                reason,
            },
        }
    }

    pub fn failed(operation: impl Into<String>, error: impl Into<String>) -> CheckpointEvent {
        Self::failed_with(None, operation, error, None)
    }

    /// Create a `Failed` event with the checkpoint id and execution id filled
    /// in, so consumers can correlate the failure with the checkpoint.
    pub fn failed_with(
        checkpoint_id: Option<String>,
        operation: impl Into<String>,
        error: impl Into<String>,
        execution_id: Option<String>,
    ) -> CheckpointEvent {
        CheckpointEvent::Failed {
            base: Self::make_base(EventType::CheckpointFailed),
            data: CheckpointData {
                checkpoint_id,
                execution_id,
                operation: Some(operation.into()),
                error: Some(error.into()),
                description: None,
                reason: None,
            },
        }
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
        match &event {
            CheckpointEvent::Created { base, data } => {
                assert_eq!(data.checkpoint_id, Some("cp-1".to_string()));
                assert_eq!(base.r#type, EventType::CheckpointCreated);
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
            CheckpointEvent::Created { data, .. } => {
                assert_eq!(data.checkpoint_id, Some("cp-1".to_string()))
            }
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
