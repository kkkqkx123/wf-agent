//! Execution event types published on the `ExecutionEventBus`.

use serde::{Deserialize, Serialize};

/// The six execution event kinds published by the execution layers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionEventType {
    StateChanged,
    ErrorOccurred,
    InterruptionOccurred,
    ToolExecuted,
    IterationStarted,
    IterationCompleted,
}

/// Execution state changed event (previous/new status + change summary).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionStateChangedEvent {
    pub execution_id: String,
    pub timestamp: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_status: Option<String>,
    pub new_status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub changes: Option<serde_json::Map<String, serde_json::Value>>,
}

/// Error occurred event.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ErrorOccurredEvent {
    pub execution_id: String,
    pub timestamp: i64,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iteration: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_id: Option<String>,
}

/// Interruption occurred event.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct InterruptionOccurredEvent {
    pub execution_id: String,
    pub timestamp: i64,
    pub interruption_type: String,
    pub reason: String,
}

/// Tool executed event.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ToolExecutedEvent {
    pub execution_id: String,
    pub timestamp: i64,
    pub tool_name: String,
    pub status: String,
    pub duration_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Map<String, serde_json::Value>>,
}

/// Agent iteration started event.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct IterationStartedEvent {
    pub execution_id: String,
    pub timestamp: i64,
    pub iteration: u32,
}

/// Agent iteration completed event.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct IterationCompletedEvent {
    pub execution_id: String,
    pub timestamp: i64,
    pub iteration: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Map<String, serde_json::Value>>,
}

/// Union of all execution events.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ExecutionEvent {
    StateChanged(ExecutionStateChangedEvent),
    ErrorOccurred(ErrorOccurredEvent),
    InterruptionOccurred(InterruptionOccurredEvent),
    ToolExecuted(ToolExecutedEvent),
    IterationStarted(IterationStartedEvent),
    IterationCompleted(IterationCompletedEvent),
}

impl ExecutionEvent {
    pub fn event_type(&self) -> ExecutionEventType {
        match self {
            Self::StateChanged(_) => ExecutionEventType::StateChanged,
            Self::ErrorOccurred(_) => ExecutionEventType::ErrorOccurred,
            Self::InterruptionOccurred(_) => ExecutionEventType::InterruptionOccurred,
            Self::ToolExecuted(_) => ExecutionEventType::ToolExecuted,
            Self::IterationStarted(_) => ExecutionEventType::IterationStarted,
            Self::IterationCompleted(_) => ExecutionEventType::IterationCompleted,
        }
    }

    pub fn execution_id(&self) -> &str {
        match self {
            Self::StateChanged(e) => &e.execution_id,
            Self::ErrorOccurred(e) => &e.execution_id,
            Self::InterruptionOccurred(e) => &e.execution_id,
            Self::ToolExecuted(e) => &e.execution_id,
            Self::IterationStarted(e) => &e.execution_id,
            Self::IterationCompleted(e) => &e.execution_id,
        }
    }
}
