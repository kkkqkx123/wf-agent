use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TokenLimitExceededEvent {
    pub base: super::BaseEvent,
    pub tokens_used: u32,
    pub token_limit: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TokenUsageWarningEvent {
    pub base: super::BaseEvent,
    pub tokens_used: u32,
    pub token_limit: u32,
    pub usage_percentage: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ErrorEvent {
    pub base: super::BaseEvent,
    pub node_id: Option<super::super::Id>,
    pub error: serde_json::Value,
    pub stack_trace: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VariableChangedEvent {
    pub base: super::BaseEvent,
    pub variable_name: String,
    pub variable_value: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LlmStreamAbortedEvent {
    pub base: super::BaseEvent,
    pub node_id: Option<super::super::Id>,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LlmStreamErrorEvent {
    pub base: super::BaseEvent,
    pub node_id: Option<super::super::Id>,
    pub error: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContextCompressionRequestedEvent {
    pub base: super::BaseEvent,
    pub tokens_used: u32,
    pub token_limit: u32,
    pub stats: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContextCompressionCompletedEvent {
    pub base: super::BaseEvent,
    pub summary: Option<String>,
    pub tokens_after: Option<u32>,
}
