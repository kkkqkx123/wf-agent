use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DecisionQuestionKind {
    Predicate,
    Choice,
    Score,
    Box,
}

impl DecisionQuestionKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Predicate => "predicate",
            Self::Choice => "choice",
            Self::Score => "score",
            Self::Box => "box",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.to_lowercase().as_str() {
            "predicate" => Some(Self::Predicate),
            "choice" => Some(Self::Choice),
            "score" => Some(Self::Score),
            "box" => Some(Self::Box),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DecisionChoiceOption {
    pub value: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DecisionScoreLevel {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DecisionNodeQuestion {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub question_type: DecisionQuestionKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub true_description: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub false_description: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub choices: Option<Vec<DecisionChoiceOption>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub levels: Option<Vec<DecisionScoreLevel>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DecisionImageAsset {
    pub alias: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base64: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variable: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DecisionNodeConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decision_profile_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub protocol: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state_variable: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state_expression: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state_json: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state_messages: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state_parts: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub image_assets: Vec<DecisionImageAsset>,
    #[serde(default)]
    pub questions: Vec<DecisionNodeQuestion>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_variable: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub safety_identifier: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_secs: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DecisionNodeOutput {
    pub model: String,
    pub answers: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<serde_json::Value>,
}
