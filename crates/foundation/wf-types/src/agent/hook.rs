use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AgentHookType {
    BeforeIteration,
    AfterIteration,
    BeforeToolCall,
    AfterToolCall,
    BeforeLlmCall,
    AfterLlmCall,
    BeforeAgent,
    AfterAgent,
    SubagentStart,
    SubagentStop,
    BeforeUserPrompt,
}

impl AgentHookType {
    /// Canonical wire name shared with the hook registry.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::BeforeIteration => "BEFORE_ITERATION",
            Self::AfterIteration => "AFTER_ITERATION",
            Self::BeforeToolCall => "BEFORE_TOOL_CALL",
            Self::AfterToolCall => "AFTER_TOOL_CALL",
            Self::BeforeLlmCall => "BEFORE_LLM_CALL",
            Self::AfterLlmCall => "AFTER_LLM_CALL",
            Self::BeforeAgent => "BEFORE_AGENT",
            Self::AfterAgent => "AFTER_AGENT",
            Self::SubagentStart => "SUBAGENT_START",
            Self::SubagentStop => "SUBAGENT_STOP",
            Self::BeforeUserPrompt => "BEFORE_USER_PROMPT",
        }
    }

    /// All known agent hook types in registration order.
    pub fn all() -> &'static [Self] {
        &[
            Self::BeforeIteration,
            Self::AfterIteration,
            Self::BeforeToolCall,
            Self::AfterToolCall,
            Self::BeforeLlmCall,
            Self::AfterLlmCall,
            Self::BeforeAgent,
            Self::AfterAgent,
            Self::SubagentStart,
            Self::SubagentStop,
            Self::BeforeUserPrompt,
        ]
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentHookConfig {
    pub hook_type: AgentHookType,
    // Single source of truth for the wire name lives on
    // AgentHookType::as_str; validation and runtime conversion must use
    // hook_type_name() instead of re-serializing the enum.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,
    /// Deprecated: retained for config compatibility only; ignored at
    /// runtime (see `crate::hook::HookPointConfig.event_name`).
    #[serde(default)]
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
    /// Optional name of a runtime-registered hook handler, notified
    /// synchronously at this hook point before the `HOOK_TRIGGERED` audit
    /// event is published. A trigger template matching that event always
    /// starts after the handler while its completion is not awaited, so
    /// prefer one path unless both effects commute. `BEFORE_*` points are
    /// handler-only (trigger-closed): the handler observes and, at the gate
    /// points, may return `Veto` to deny the guarded step; without a handler
    /// the definition only writes a write-only audit event.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub handler: Option<String>,
}

impl AgentHookConfig {
    /// Wire name of the hook type, shared with the hook registry.
    pub fn hook_type_name(&self) -> &'static str {
        self.hook_type.as_str()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_hook_types_match_registry() {
        for hook in AgentHookType::all() {
            assert!(
                crate::hook::is_known_hook_point(hook.as_str()),
                "{} must be registered",
                hook.as_str()
            );
        }
        assert_eq!(
            AgentHookType::all().len(),
            crate::hook::AGENT_HOOK_TYPES.len()
        );
    }

    #[test]
    fn wire_names_roundtrip() {
        let value = serde_json::to_value(AgentHookType::SubagentStart).unwrap();
        assert_eq!(value, serde_json::json!("SUBAGENT_START"));
        let back: AgentHookType = serde_json::from_value(value).unwrap();
        assert_eq!(back.as_str(), "SUBAGENT_START");
    }
}
