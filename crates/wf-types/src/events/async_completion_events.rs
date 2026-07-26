use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AsyncCompletionRegisteredEvent {
    pub base: super::BaseEvent,
    pub execution_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AsyncCompletionTriggeredEvent {
    pub base: super::BaseEvent,
    pub execution_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AsyncCompletionErrorTriggeredEvent {
    pub base: super::BaseEvent,
    pub execution_id: String,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AsyncCompletionFailedEvent {
    pub base: super::BaseEvent,
    pub execution_id: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AsyncCompletionCleanedUpEvent {
    pub base: super::BaseEvent,
    pub execution_id: String,
    pub reason: Option<String>,
}
