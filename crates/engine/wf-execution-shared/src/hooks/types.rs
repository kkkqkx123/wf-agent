use std::collections::HashMap;

use serde_json::Value;
use wf_types::Id;

pub use wf_types::hook::{
    hook_allows_trigger, hook_effect, hook_requires_handler, is_known_hook_point, AGENT_HOOK_TYPES,
    WORKFLOW_HOOK_TYPES,
};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct HookDefinition {
    pub id: Id,
    pub hook_type: String,
    pub weight: i32,
    pub condition: Option<String>,
    pub enabled: bool,
    /// Optional payload template, resolved against the hook context at
    /// emission time and surfaced on the `HOOK_TRIGGERED` audit event.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<Value>,
    /// Optional name of a dynamically registered [`HookHandler`]. When set,
    /// the handler is notified synchronously during fire; when absent
    /// the hook degrades to the audit-only behavior (event + log).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handler: Option<String>,
}

/// Outcome of one hook fire: the engine stops and waits for every
/// notified handler, aggregating their outcomes.
///
/// Handlers are observation-only by default: blocking tool calls, rewriting
/// inputs and permission decisions belong to the approval and workflow
/// mechanisms. A handler may additionally return [`HookOutcome::Veto`] to
/// deny the guarded step, but the veto only takes effect at gate points
/// that opt into it (`BEFORE_EXECUTE` on the workflow node path,
/// `BEFORE_TOOL_CALL` on the agent tool path); at every other point a veto
/// is recorded in the audit event and otherwise treated as `Continue`.
#[derive(Debug, Clone, PartialEq)]
pub enum HookOutcome {
    Continue,
    /// Deny the guarded step. `reason` is surfaced on the audit event and,
    /// at gate points, becomes the failure reason (node error / tool
    /// rejection). Timeouts and unresolvable handlers still resolve to
    /// `Continue`: gates fail open, so gate handlers must be fast and local.
    Veto {
        reason: String,
    },
}

impl HookOutcome {
    /// Whether this outcome denies the guarded step.
    pub fn is_veto(&self) -> bool {
        matches!(self, Self::Veto { .. })
    }

    /// Wire name used on the `HOOK_TRIGGERED` audit event.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Continue => "continue",
            Self::Veto { .. } => "vetoed",
        }
    }
}

/// The context a handler observes at a hook point: the execution id, the
/// named hook point and the parsed payload data.
#[derive(Debug, Clone)]
pub struct HookContext {
    pub execution_id: Id,
    pub hook_type: String,
    pub data: HashMap<String, Value>,
}

impl From<&wf_types::hook::HookPointConfig> for HookDefinition {
    /// Convert a serde-facing hook config into an executable hook definition.
    ///
    /// Thin adapter over the authoritative spec (`CanonicalHookSpec` holds
    /// the field semantics): defaults mirror the agent conversion (weight 0,
    /// enabled). `event_name` is deprecated and ignored: one fire aggregates
    /// many definitions into a single `HOOK_TRIGGERED` audit event, so
    /// per-definition names cannot be honored (a warn is emitted when one
    /// is set).
    fn from(config: &wf_types::hook::HookPointConfig) -> Self {
        if !config.event_name.is_empty() {
            tracing::warn!(
                hook_type = %config.hook_type,
                event_name = %config.event_name,
                "hook event_name is deprecated and ignored; subscribe via HOOK_TRIGGERED plus metadata.hook_type"
            );
        }
        Self::from(&wf_types::hook::CanonicalHookSpec::from_workflow(config))
    }
}

impl From<&wf_types::hook::CanonicalHookSpec> for HookDefinition {
    /// Build an executable definition from the authoritative spec.
    fn from(spec: &wf_types::hook::CanonicalHookSpec) -> Self {
        Self {
            id: Id::new(),
            hook_type: spec.hook_type.clone(),
            weight: spec.weight,
            condition: spec.condition.clone(),
            enabled: spec.enabled,
            payload: spec.payload.clone(),
            handler: spec.handler.clone(),
        }
    }
}

impl From<&wf_types::hook::HookPointStaticConfig> for HookDefinition {
    /// Static form: thin adapter via the authoritative spec.
    fn from(config: &wf_types::hook::HookPointStaticConfig) -> Self {
        if !config.event_name.is_empty() {
            tracing::warn!(
                hook_type = %config.hook_type,
                event_name = %config.event_name,
                "hook event_name is deprecated and ignored; subscribe via HOOK_TRIGGERED plus metadata.hook_type"
            );
        }
        Self::from(&wf_types::hook::CanonicalHookSpec::from_static(config))
    }
}

impl From<&wf_types::agent::AgentHookConfig> for HookDefinition {
    /// Agent form: thin adapter via the authoritative spec.
    fn from(config: &wf_types::agent::AgentHookConfig) -> Self {
        if !config.event_name.is_empty() {
            tracing::warn!(
                hook_type = %config.hook_type_name(),
                event_name = %config.event_name,
                "hook event_name is deprecated and ignored; subscribe via HOOK_TRIGGERED plus metadata.hook_type"
            );
        }
        Self::from(&wf_types::hook::CanonicalHookSpec::from_agent(config))
    }
}

impl From<&wf_tools::callback::HookConfig> for HookDefinition {
    /// Tool-callback form: thin adapter via the authoritative spec.
    fn from(config: &wf_tools::callback::HookConfig) -> Self {
        Self::from(&config.to_canonical())
    }
}
