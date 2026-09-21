use serde::{Deserialize, Serialize};

/// Lightweight single-shot model call: system prompt (inline or template)
/// plus context messages go out as one request. This node never exposes
/// tools, never enriches skills and never assembles dynamic context; a
/// scenario needing any of those must use an `AGENT_LOOP` node.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LlmNodeConfig {
    pub profile_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_context: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generation: Option<super::super::super::llm::generation::LlmGenerationParams>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tool_calls_per_request: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_protocol: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LlmNodeOutput {
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<super::super::super::message::LlmToolCall>>,
}
