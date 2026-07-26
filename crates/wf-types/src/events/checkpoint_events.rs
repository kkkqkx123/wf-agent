use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CheckpointCreatedEvent {
    pub base: super::BaseEvent,
    pub checkpoint_id: super::super::Id,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CheckpointRestoredEvent {
    pub base: super::BaseEvent,
    pub checkpoint_id: super::super::Id,
    pub execution_id: super::super::Id,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CheckpointDeletedEvent {
    pub base: super::BaseEvent,
    pub checkpoint_id: super::super::Id,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CheckpointFailedEvent {
    pub base: super::BaseEvent,
    pub checkpoint_id: Option<super::super::Id>,
    pub operation: String,
    pub error: String,
}
