use serde::{Deserialize, Serialize};

/// Agent loop hook types (fired by the agent engine at iteration / LLM /
/// tool-call / loop boundaries).
///
/// Single source of truth for known hook types; the runtime pipeline in
/// `wf-execution-shared` re-exports these and config validation
/// (`wf-agent`, `wf-config`) references [`is_known_hook_type`].
pub const AGENT_HOOK_TYPES: &[&str] = &[
    "BEFORE_ITERATION",
    "AFTER_ITERATION",
    "BEFORE_LLM_CALL",
    "AFTER_LLM_CALL",
    "BEFORE_TOOL_CALL",
    "AFTER_TOOL_CALL",
    "BEFORE_AGENT",
    "AFTER_AGENT",
    // Sub-agent lifecycle observation points: fired by the triggered-agent
    // manager on the parent entity's hook configuration (START when the child
    // is created, STOP when its execution settles).
    "SUBAGENT_START",
    "SUBAGENT_STOP",
    // User input boundary: fired when a user prompt enters the loop, before
    // the start event.
    "BEFORE_USER_PROMPT",
];

/// Internal engine signal points: named hook types that are not part of the
/// user-facing hook config vocabulary. The engine dispatches them so builtin
/// services (e.g. context compression) registered as receivers are notified
/// synchronously; the audit event still leaves an event-bus copy for
/// persistence and user trigger rules.
pub const INTERNAL_SIGNAL_TYPES: &[&str] = &["CONTEXT_COMPRESSION_REQUESTED"];

/// Hook type of the engine's internal context-compression signal: the engine
/// dispatches it synchronously when a named message array exceeds its token
/// limit (or a forced safety-net request fires); the
/// `CONTEXT_COMPRESSION_REQUESTED` event remains the audit/persistence
/// channel.
pub const CONTEXT_COMPRESSION_SIGNAL: &str = "CONTEXT_COMPRESSION_REQUESTED";

/// Sub-agent lifecycle start: fired by the triggered-agent manager once the
/// child entity is created and registered on the parent.
pub const SUBAGENT_START: &str = "SUBAGENT_START";

/// Sub-agent lifecycle stop: fired once a child execution settles (success,
/// failure, timeout or parent abort).
pub const SUBAGENT_STOP: &str = "SUBAGENT_STOP";

/// User input boundary: fired when a user prompt enters the loop, before the
/// start event.
pub const BEFORE_USER_PROMPT: &str = "BEFORE_USER_PROMPT";

/// Workflow hook types. `BEFORE_EXECUTE` / `AFTER_EXECUTE` fire per node;
/// `ON_ERROR` fires when a node fails; `WORKFLOW_BEFORE` / `WORKFLOW_AFTER`
/// fire once around the whole execution.
pub const WORKFLOW_HOOK_TYPES: &[&str] = &[
    "BEFORE_EXECUTE",
    "AFTER_EXECUTE",
    "ON_ERROR",
    "WORKFLOW_BEFORE",
    "WORKFLOW_AFTER",
];

/// Whether the hook type is a known agent or workflow hook type. Config
/// validation uses this as the single source of truth; unknown types may
/// still be handled by externally registered handlers.
pub fn is_known_hook_type(hook_type: &str) -> bool {
    AGENT_HOOK_TYPES.contains(&hook_type)
        || WORKFLOW_HOOK_TYPES.contains(&hook_type)
        || INTERNAL_SIGNAL_TYPES.contains(&hook_type)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BaseHookConfig {
    pub hook_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub condition: Option<serde_json::Value>,
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
    /// Optional name of a runtime-registered hook receiver; when set the
    /// engine notifies it synchronously at this hook point.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub receiver: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BaseHookStaticConfig {
    pub hook_type: String,
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
