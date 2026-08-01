use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum MessageStreamEventType {
    Connect,
    Stream,
    Text,
    InputJson,
    Message,
    FinalMessage,
    Error,
    Abort,
    End,
    ReasoningText,
    Usage,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "event_type", rename_all = "snake_case")]
pub enum MessageStreamEvent {
    Connect(MessageStreamConnect),
    Stream(MessageStreamChunk),
    Text(MessageStreamText),
    InputJson(MessageStreamInputJson),
    Message(MessageStreamMsg),
    FinalMessage(MessageStreamFinal),
    Error(MessageStreamError),
    Abort(MessageStreamAbort),
    End(MessageStreamEnd),
    ReasoningText(MessageStreamReasoning),
    Usage(MessageStreamUsage),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MessageStreamConnect {
    pub session_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MessageStreamChunk {
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MessageStreamText {
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MessageStreamInputJson {
    pub partial_json: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MessageStreamMsg {
    pub message: super::super::message::Message,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MessageStreamFinal {
    pub message: super::super::message::Message,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<super::TokenUsageStats>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MessageStreamError {
    pub error: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MessageStreamAbort {
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MessageStreamEnd {}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MessageStreamReasoning {
    pub reasoning: String,
}

/// Token usage reported mid-stream (e.g. OpenAI's `include_usage` chunk or
/// Anthropic's `message_delta` usage).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MessageStreamUsage {
    pub usage: super::TokenUsageStats,
}
