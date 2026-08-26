use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AgentHookType {
    BeforeIteration,
    AfterIteration,
    BeforeToolCall,
    AfterToolCall,
    BeforeLlmCall,
    AfterLlmCall,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentHookConfig {
    pub hook_type: AgentHookType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,
    pub event_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_payload: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weight: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub create_checkpoint: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checkpoint_description: Option<String>,
    /// Optional name of a runtime-registered hook receiver.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub receiver: Option<String>,
}
