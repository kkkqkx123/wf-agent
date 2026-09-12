use serde::{Deserialize, Serialize};

/// Agent loop hook types (fired by the agent engine at iteration / LLM /
/// tool-call / loop boundaries).
///
/// Single source of truth for known hook types; the runtime pipeline in
/// `wf-execution-shared` re-exports these and config validation
/// (`wf-agent`, `wf-config`) references [`is_known_hook_point`].
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
/// services (e.g. context compression) registered as handlers are notified
/// synchronously; the audit event is a persistence and observability copy
/// only and must not be used as a functional trigger source.
pub const INTERNAL_SIGNAL_TYPES: &[&str] = &["CONTEXT_COMPRESSION_REQUESTED"];

/// Hook type of the engine's internal context-compression signal: the engine
/// dispatches it synchronously when a named message array exceeds its token
/// limit (or a forced safety-net request fires) and the builtin compression
/// service takes over immediately. The `CONTEXT_COMPRESSION_REQUESTED`
/// event is the audit and persistence copy; user trigger templates should
/// not subscribe to it for functional work.
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
pub fn is_known_hook_point(hook_type: &str) -> bool {
    AGENT_HOOK_TYPES.contains(&hook_type)
        || WORKFLOW_HOOK_TYPES.contains(&hook_type)
        || INTERNAL_SIGNAL_TYPES.contains(&hook_type)
}

/// Effect category of a hook point, mirroring `events::EventCategory`.
///
/// Observability points are loss-tolerant and skip completeness checks;
/// request and mutated points require a registered handler or trigger rule.
/// Unknown hook types default to `Observable` for forward compatibility.
pub fn hook_effect(hook_type: &str) -> crate::events::EventCategory {
    use crate::events::EventCategory;
    match hook_type {
        s if INTERNAL_SIGNAL_TYPES.contains(&s) => EventCategory::Request,
        "ON_ERROR" | "WORKFLOW_BEFORE" | "WORKFLOW_AFTER" => EventCategory::Mutated,
        _ => EventCategory::Observable,
    }
}

/// Whether a hook point requires a registered handler or trigger rule to be
/// considered completely wired. Only request and mutated points do;
/// observability points are usable on demand with zero subscribers.
pub fn hook_requires_handler(hook_type: &str) -> bool {
    !matches!(
        hook_effect(hook_type),
        crate::events::EventCategory::Observable
    )
}

/// Delivery model: a hook has two independent paths with no ordering
/// guarantee. The synchronous `handler` path is for fast local observation
/// (millisecond budget, engine-awaited). The asynchronous path publishes an
/// audit event that trigger templates may match later (delay-tolerant side
/// effects, retries, cross-execution access). Prefer one path per hook
/// unless both effects are known to commute.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HookPointConfig {
    pub hook_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub condition: Option<serde_json::Value>,
    /// Deprecated: retained for config compatibility only. One fire
    /// aggregates many definitions into a single `HOOK_TRIGGERED` audit
    /// event, so a per-definition event name cannot be honored; the runtime
    /// ignores it (warns once) and trigger templates must match
    /// `HOOK_TRIGGERED` plus `metadata.hook_type` instead.
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
    /// Optional name of a runtime-registered hook handler; when set the
    /// engine notifies it synchronously at this hook point. Independent from
    /// the asynchronous trigger path; the two have no ordering guarantee.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub handler: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HookPointStaticConfig {
    pub hook_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,
    /// Deprecated: retained for config compatibility only; ignored at
    /// runtime (see `HookPointConfig.event_name`).
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
    /// Optional name of a runtime-registered hook handler. Independent from
    /// the asynchronous trigger path; the two have no ordering guarantee.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub handler: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn observability_hooks_skip_handler_check() {
        for hook in [
            "BEFORE_ITERATION",
            "AFTER_TOOL_CALL",
            "BEFORE_EXECUTE",
            "SUBAGENT_START",
        ] {
            assert!(
                !hook_requires_handler(hook),
                "{hook} must be usable with zero subscribers"
            );
        }
    }

    #[test]
    fn request_and_mutated_hooks_require_handler() {
        assert!(hook_requires_handler("CONTEXT_COMPRESSION_REQUESTED"));
        assert!(hook_requires_handler("ON_ERROR"));
        assert!(hook_requires_handler("WORKFLOW_BEFORE"));
    }

    #[test]
    fn unknown_hooks_default_to_observable() {
        assert!(!hook_requires_handler("SOME_FUTURE_HOOK"));
    }
}
