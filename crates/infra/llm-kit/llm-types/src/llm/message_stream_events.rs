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
    ToolCallDelta,
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
    ToolCallDelta(MessageStreamToolCallDelta),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MessageStreamConnect {
    pub session_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MessageStreamChunk {
    pub content: String,
}

/// Text increment with the accumulated snapshot of the whole streamed text.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MessageStreamText {
    pub text: String,
    #[serde(default)]
    pub snapshot: String,
}

/// Incremental tool_use JSON input. `index` refers to the tool call block the
/// fragment belongs to (Anthropic-style); `parsed_snapshot` is the best-effort
/// parse of the accumulated JSON buffer, refreshed by the message accumulator.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MessageStreamInputJson {
    pub partial_json: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub index: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parsed_snapshot: Option<serde_json::Value>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream_stats: Option<super::StreamStats>,
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

/// Reasoning increment with the accumulated snapshot.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MessageStreamReasoning {
    pub reasoning: String,
    #[serde(default)]
    pub snapshot: String,
}

/// Token usage reported mid-stream (e.g. OpenAI's `include_usage` chunk or
/// Anthropic's `message_delta` usage).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MessageStreamUsage {
    pub usage: super::TokenUsageStats,
}

/// Incremental streaming tool call data.
///
/// Providers deliver tool calls across multiple chunks. `index` groups the
/// fragments of one logical call; `id`/`name` are typically present only on
/// the first fragment (OpenAI) or on `content_block_start` (Anthropic).
/// `arguments` is an increment to be accumulated by the consumer. When
/// `is_snapshot` is true the fragment carries a complete tool call (e.g.
/// Gemini `functionCall` parts), and `arguments` is the fully serialized
/// JSON arguments.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MessageStreamToolCallDelta {
    pub index: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arguments: Option<String>,
    #[serde(default)]
    pub is_snapshot: bool,
}
