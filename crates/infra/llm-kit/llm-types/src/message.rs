//! Message and tool-call data models shared across llm-kit crates.

use serde::{Deserialize, Serialize};

/// Role of a conversation participant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    /// Instructions from the application.
    System,
    /// End-user input.
    User,
    /// Model output.
    Assistant,
    /// Tool execution result returned to the model.
    Tool,
}

/// Content of a conversation message.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MessageContent {
    /// Plain text content.
    Text(String),
    /// Structured parts (text chunks, images, etc.).
    Parts(Vec<ContentPart>),
}

/// A single structured content part.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ContentPart {
    /// A text chunk.
    Text { text: String },
}

/// A conversation message.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Message {
    /// Role of the message author.
    pub role: MessageRole,
    /// Message content.
    pub content: MessageContent,
    /// Tool calls requested by the assistant (if any).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCall>,
}

impl Message {
    /// Creates a text message with the given role.
    pub fn text(role: MessageRole, text: impl Into<String>) -> Self {
        Self {
            role,
            content: MessageContent::Text(text.into()),
            tool_calls: Vec::new(),
        }
    }
}

/// A completed tool call emitted by the model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolCall {
    /// Provider-assigned identifier for this call.
    pub id: String,
    /// Name of the tool being invoked.
    pub name: String,
    /// Raw JSON arguments payload.
    pub arguments: String,
}

/// An incremental tool-call fragment produced while streaming.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ToolCallDelta {
    /// Identifier, present once per call in the stream.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Tool name fragment.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Arguments JSON fragment.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arguments: Option<String>,
}
