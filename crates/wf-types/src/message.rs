use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum MessageRole {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LlmToolCall {
    pub id: String,
    pub r#type: String,
    pub function: LlmFunctionCall,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LlmFunctionCall {
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolUseContent {
    pub id: String,
    pub name: String,
    pub input: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolResultContent {
    pub tool_use_id: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ImageUrlContent {
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MessageContent {
    Text {
        text: String,
    },
    ImageUrl {
        image_url: ImageUrlContent,
    },
    ToolUse {
        tool_use: ToolUseContent,
    },
    ToolResult {
        tool_result: ToolResultContent,
    },
    Thinking {
        thinking: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        signature: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Message {
    pub id: crate::Id,
    pub role: MessageRole,
    pub content: MessageContentValue,
    pub timestamp: crate::Timestamp,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<LlmToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<crate::Metadata>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(untagged)]
pub enum MessageContentValue {
    Text(String),
    Rich(Vec<MessageContent>),
}

// `#[serde(untagged)]` cannot deserialize a variant that is itself an
// internally tagged enum (`#[serde(tag = "type")]`): the buffered replay
// machinery rejects it with "data did not match any variant". A custom
// Deserialize over `serde_json::Value` keeps the round trip working and is
// lenient towards the TS reference formats: a bare string (plain text), an
// array of typed content parts, or a single content part object.
impl<'de> Deserialize<'de> for MessageContentValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::Error as _;

        let value = serde_json::Value::deserialize(deserializer)?;
        match value {
            serde_json::Value::String(text) => Ok(MessageContentValue::Text(text)),
            serde_json::Value::Array(items) => {
                serde_json::from_value(serde_json::Value::Array(items))
                    .map(MessageContentValue::Rich)
                    .map_err(D::Error::custom)
            }
            serde_json::Value::Object(_) => serde_json::from_value(value)
                .map(|content: MessageContent| MessageContentValue::Rich(vec![content]))
                .map_err(D::Error::custom),
            other => Err(D::Error::custom(format!(
                "invalid message content value: {other}"
            ))),
        }
    }
}

pub mod batch_management_operation;
pub mod batch_snapshot;
pub mod message_array;
pub mod message_context;
pub mod message_mark_map;
pub mod message_operations;
pub mod named_message_context;

pub use batch_management_operation::*;
pub use batch_snapshot::*;
pub use message_array::*;
pub use message_context::*;
pub use message_mark_map::*;
pub use message_operations::*;
pub use named_message_context::*;
