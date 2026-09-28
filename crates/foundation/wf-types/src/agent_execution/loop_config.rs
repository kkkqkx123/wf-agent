use serde::{Deserialize, Serialize};

use crate::Id;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HookConfig {
    pub hook_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,
    pub enabled: bool,
    #[serde(default)]
    pub priority: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handler: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub create_checkpoint: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checkpoint_description: Option<String>,
}

impl HookConfig {
    pub fn to_canonical(&self) -> crate::hook::CanonicalHookSpec {
        let mut spec = crate::hook::CanonicalHookSpec::from_parts(
            self.hook_type.clone(),
            self.condition.clone(),
            self.enabled,
            self.priority,
            self.payload.clone(),
            self.handler.clone(),
        );
        spec.create_checkpoint = self.create_checkpoint;
        spec.checkpoint_description = self.checkpoint_description.clone();
        spec
    }

    pub fn from_canonical(spec: &crate::hook::CanonicalHookSpec) -> Self {
        Self {
            hook_type: spec.hook_type.clone(),
            condition: spec.condition.clone(),
            enabled: spec.enabled,
            priority: spec.priority,
            payload: spec.payload.clone(),
            handler: spec.handler.clone(),
            create_checkpoint: spec.create_checkpoint,
            checkpoint_description: spec.checkpoint_description.clone(),
        }
    }

    pub fn from_agent_hook(hook: &crate::agent::AgentHookConfig) -> Self {
        Self::from_canonical(&crate::hook::CanonicalHookSpec::from_agent(hook))
    }
}

impl From<&crate::agent::AgentHookConfig> for HookConfig {
    fn from(hook: &crate::agent::AgentHookConfig) -> Self {
        Self::from_agent_hook(hook)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentLoopConfig {
    pub agent_id: Id,
    pub model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_iterations: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_execution_time: Option<u64>,
    #[serde(default)]
    pub hooks: Vec<HookConfig>,
    #[serde(default)]
    pub available_tool_names: Vec<String>,
    #[serde(default)]
    pub initial_tool_names: Vec<String>,
    #[serde(default)]
    pub discoverable_tool_names: Vec<String>,
    #[serde(default)]
    pub activated_tool_names: Vec<String>,
    #[serde(default)]
    pub hidden_tool_names: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enable_general_tool: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_protocol: Option<crate::llm::tool_call_protocol::ToolCallProtocolConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_limit: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_warning_threshold: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enable_token_tracking: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checkpoint_message_interval: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub general_description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discoverable_metadata_block: Option<String>,
    #[serde(default)]
    pub history_normalization: bool,
}
