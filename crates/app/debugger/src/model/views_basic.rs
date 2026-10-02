use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MessageView {
    pub id: String,
    #[serde(default)]
    pub role: String,
    #[serde(default)]
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolCallView {
    pub name: String,
    #[serde(default)]
    pub call_id: String,
    #[serde(default)]
    pub arguments: serde_json::Value,
    #[serde(default)]
    pub result: Option<serde_json::Value>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub duration_ms: Option<i64>,
    #[serde(default)]
    pub success: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LlmCallView {
    #[serde(default)]
    pub profile_id: String,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub prompt_tokens: u32,
    #[serde(default)]
    pub completion_tokens: u32,
    /// Reported total; zero means unreported and the analyzer falls back to
    /// prompt plus completion.
    #[serde(default)]
    pub total_tokens: u32,
    #[serde(default)]
    pub reasoning_tokens: Option<u32>,
    #[serde(default)]
    pub cache_read_tokens: Option<u32>,
    #[serde(default)]
    pub cache_write_tokens: Option<u32>,
    #[serde(default)]
    pub total_cost: Option<f64>,
    /// True when the usage is a local estimate rather than provider-reported.
    /// Estimates feed the cost track only and never drive budget decisions.
    #[serde(default)]
    pub estimated: bool,
    #[serde(default)]
    pub content_preview: Option<String>,
    #[serde(default)]
    pub tool_call_count: u32,
    #[serde(default)]
    pub error: Option<String>,
}

impl LlmCallView {
    pub fn effective_total(&self) -> u64 {
        if self.total_tokens > 0 {
            u64::from(self.total_tokens)
        } else {
            u64::from(self.prompt_tokens) + u64::from(self.completion_tokens)
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ApprovalView {
    pub tool_name: String,
    pub decision: String,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct VisibilityView {
    #[serde(default)]
    pub visible: Vec<String>,
    #[serde(default)]
    pub gated: Vec<String>,
    #[serde(default)]
    pub hidden: Vec<String>,
    #[serde(default)]
    pub discoverable: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn llm_total_falls_back_to_prompt_plus_completion() {
        let call = LlmCallView {
            profile_id: "p".to_string(),
            model: None,
            prompt_tokens: 10,
            completion_tokens: 5,
            total_tokens: 0,
            reasoning_tokens: None,
            cache_read_tokens: None,
            cache_write_tokens: None,
            total_cost: None,
            estimated: false,
            content_preview: None,
            tool_call_count: 0,
            error: None,
        };
        assert_eq!(call.effective_total(), 15);
    }
}
