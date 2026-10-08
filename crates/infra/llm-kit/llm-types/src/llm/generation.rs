use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThinkingLevel {
    None,
    Minimal,
    Low,
    Medium,
    High,
    Xhigh,
    Max,
}

impl ThinkingLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            ThinkingLevel::None => "none",
            ThinkingLevel::Minimal => "minimal",
            ThinkingLevel::Low => "low",
            ThinkingLevel::Medium => "medium",
            ThinkingLevel::High => "high",
            ThinkingLevel::Xhigh => "xhigh",
            ThinkingLevel::Max => "max",
        }
    }
}

impl std::fmt::Display for ThinkingLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Verbosity {
    Low,
    Medium,
    High,
}

impl Verbosity {
    pub fn as_str(&self) -> &'static str {
        match self {
            Verbosity::Low => "low",
            Verbosity::Medium => "medium",
            Verbosity::High => "high",
        }
    }
}

impl std::fmt::Display for Verbosity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReasoningSummary {
    Auto,
    Concise,
    Detailed,
}

impl ReasoningSummary {
    pub fn as_str(&self) -> &'static str {
        match self {
            ReasoningSummary::Auto => "auto",
            ReasoningSummary::Concise => "concise",
            ReasoningSummary::Detailed => "detailed",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThinkingDisplay {
    Omitted,
    Summarized,
}

impl ThinkingDisplay {
    pub fn as_str(&self) -> &'static str {
        match self {
            ThinkingDisplay::Omitted => "omitted",
            ThinkingDisplay::Summarized => "summarized",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct LlmThinkingConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub level: Option<ThinkingLevel>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        alias = "budgetTokens",
        alias = "budget_tokens"
    )]
    pub budget_tokens: Option<u32>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        alias = "includeThoughts",
        alias = "include_thoughts"
    )]
    pub include_thoughts: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adaptive: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display: Option<ThinkingDisplay>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<ReasoningSummary>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolChoiceMode {
    Auto,
    Any,
    None,
    Required,
}

impl ToolChoiceMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            ToolChoiceMode::Auto => "auto",
            ToolChoiceMode::Any => "any",
            ToolChoiceMode::None => "none",
            ToolChoiceMode::Required => "required",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct LlmToolChoice {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<ToolChoiceMode>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        alias = "toolName",
        alias = "tool_name"
    )]
    pub tool_name: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResponseFormatKind {
    Text,
    JsonObject,
    JsonSchema,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct LlmResponseFormat {
    #[serde(default, alias = "type")]
    pub kind: Option<ResponseFormatKind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LlmServiceTier {
    Auto,
    Default,
    Flex,
    Scale,
    Priority,
    Fast,
    #[serde(alias = "standardOnly")]
    StandardOnly,
}

impl LlmServiceTier {
    pub fn as_str(&self) -> &'static str {
        match self {
            LlmServiceTier::Auto => "auto",
            LlmServiceTier::Default => "default",
            LlmServiceTier::Flex => "flex",
            LlmServiceTier::Scale => "scale",
            LlmServiceTier::Priority => "priority",
            LlmServiceTier::Fast => "fast",
            LlmServiceTier::StandardOnly => "standard_only",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct LlmGenerationParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        alias = "topP",
        alias = "top_p"
    )]
    pub top_p: Option<f64>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        alias = "topK",
        alias = "top_k"
    )]
    pub top_k: Option<u32>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        alias = "maxTokens",
        alias = "max_tokens",
        alias = "maxCompletionTokens",
        alias = "max_completion_tokens",
        alias = "maxOutputTokens",
        alias = "max_output_tokens"
    )]
    pub max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<LlmThinkingConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verbosity: Option<Verbosity>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        alias = "toolChoice",
        alias = "tool_choice"
    )]
    pub tool_choice: Option<LlmToolChoice>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        alias = "parallelToolCalls",
        alias = "parallel_tool_calls"
    )]
    pub parallel_tool_calls: Option<bool>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        alias = "responseFormat",
        alias = "response_format"
    )]
    pub response_format: Option<LlmResponseFormat>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        alias = "serviceTier",
        alias = "service_tier"
    )]
    pub service_tier: Option<LlmServiceTier>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub store: Option<bool>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        alias = "userId",
        alias = "user_id"
    )]
    pub user_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<u64>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        alias = "frequencyPenalty",
        alias = "frequency_penalty"
    )]
    pub frequency_penalty: Option<f64>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        alias = "presencePenalty",
        alias = "presence_penalty"
    )]
    pub presence_penalty: Option<f64>,
}

impl LlmGenerationParams {
    pub fn is_empty(&self) -> bool {
        self.temperature.is_none()
            && self.top_p.is_none()
            && self.top_k.is_none()
            && self.max_tokens.is_none()
            && self.stop.is_none()
            && self.thinking.is_none()
            && self.verbosity.is_none()
            && self.tool_choice.is_none()
            && self.parallel_tool_calls.is_none()
            && self.response_format.is_none()
            && self.service_tier.is_none()
            && self.store.is_none()
            && self.user_id.is_none()
            && self.seed.is_none()
            && self.frequency_penalty.is_none()
            && self.presence_penalty.is_none()
    }

    pub fn merge_over(&mut self, over: &LlmGenerationParams) {
        if over.temperature.is_some() {
            self.temperature = over.temperature;
        }
        if over.top_p.is_some() {
            self.top_p = over.top_p;
        }
        if over.top_k.is_some() {
            self.top_k = over.top_k;
        }
        if over.max_tokens.is_some() {
            self.max_tokens = over.max_tokens;
        }
        if over.stop.is_some() {
            self.stop = over.stop.clone();
        }
        if over.thinking.is_some() {
            self.thinking = over.thinking.clone();
        }
        if over.verbosity.is_some() {
            self.verbosity = over.verbosity;
        }
        if over.tool_choice.is_some() {
            self.tool_choice = over.tool_choice.clone();
        }
        if over.parallel_tool_calls.is_some() {
            self.parallel_tool_calls = over.parallel_tool_calls;
        }
        if over.response_format.is_some() {
            self.response_format = over.response_format.clone();
        }
        if over.service_tier.is_some() {
            self.service_tier = over.service_tier;
        }
        if over.store.is_some() {
            self.store = over.store;
        }
        if over.user_id.is_some() {
            self.user_id = over.user_id.clone();
        }
        if over.seed.is_some() {
            self.seed = over.seed;
        }
        if over.frequency_penalty.is_some() {
            self.frequency_penalty = over.frequency_penalty;
        }
        if over.presence_penalty.is_some() {
            self.presence_penalty = over.presence_penalty;
        }
    }

    pub fn thinking_level(&self) -> Option<ThinkingLevel> {
        self.thinking.as_ref().and_then(|t| t.level)
    }

    pub fn has_reasoning(&self) -> bool {
        match self.thinking.as_ref() {
            None => false,
            Some(t) => {
                t.level.is_some_and(|l| l != ThinkingLevel::None)
                    || t.budget_tokens.is_some()
                    || t.adaptive.unwrap_or(false)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thinking_level_roundtrip() {
        for level in [
            ThinkingLevel::None,
            ThinkingLevel::Minimal,
            ThinkingLevel::Low,
            ThinkingLevel::Medium,
            ThinkingLevel::High,
            ThinkingLevel::Xhigh,
            ThinkingLevel::Max,
        ] {
            let json = serde_json::to_string(&level).unwrap();
            let back: ThinkingLevel = serde_json::from_str(&json).unwrap();
            assert_eq!(back, level);
        }
        assert_eq!(
            serde_json::from_str::<ThinkingLevel>("\"xhigh\"").unwrap(),
            ThinkingLevel::Xhigh
        );
    }

    #[test]
    fn generation_camel_case_aliases() {
        let params: LlmGenerationParams = serde_json::from_value(serde_json::json!({
            "topP": 0.9,
            "topK": 40,
            "maxTokens": 1024,
            "toolChoice": {"mode": "auto"},
            "parallelToolCalls": false,
            "serviceTier": "auto",
            "frequencyPenalty": 0.5,
        }))
        .unwrap();
        assert_eq!(params.top_p, Some(0.9));
        assert_eq!(params.top_k, Some(40));
        assert_eq!(params.max_tokens, Some(1024));
        assert_eq!(params.parallel_tool_calls, Some(false));
        assert_eq!(params.service_tier, Some(LlmServiceTier::Auto));
        assert_eq!(params.frequency_penalty, Some(0.5));
    }

    #[test]
    fn merge_prefers_overlay() {
        let mut base = LlmGenerationParams {
            temperature: Some(0.7),
            max_tokens: Some(100),
            ..Default::default()
        };
        let over = LlmGenerationParams {
            temperature: Some(0.2),
            verbosity: Some(Verbosity::Low),
            ..Default::default()
        };
        base.merge_over(&over);
        assert_eq!(base.temperature, Some(0.2));
        assert_eq!(base.max_tokens, Some(100));
        assert_eq!(base.verbosity, Some(Verbosity::Low));
    }
}
