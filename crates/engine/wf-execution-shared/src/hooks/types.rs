use std::collections::HashMap;

use serde_json::Value;
use wf_types::Id;

pub use wf_types::hook::{is_known_hook_type, AGENT_HOOK_TYPES, WORKFLOW_HOOK_TYPES};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BaseHookDefinition {
    pub id: Id,
    pub hook_type: String,
    pub weight: i32,
    pub condition: Option<String>,
    pub enabled: bool,
    /// Optional payload template, resolved against the hook context at
    /// emission time and surfaced on the `HOOK_TRIGGERED` audit event.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<Value>,
    /// Optional name of a dynamically registered [`HookReceiver`]. When set,
    /// the receiver is notified synchronously during dispatch; when absent
    /// the hook degrades to the audit-only behavior (event + log).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub receiver: Option<String>,
}

#[derive(Debug, Clone)]
pub struct BaseHookContext {
    pub execution_id: Id,
    pub data: HashMap<String, Value>,
}

/// Outcome of one hook dispatch: the engine stops and waits for every
/// notified receiver, aggregating their outcomes. `Intercept` is a mechanism
/// reserve (blocking tool calls, rewriting inputs, permission decisions are
/// handled by approval/workflow mechanisms); no engine consumer wires it yet.
#[derive(Debug, Clone, PartialEq)]
pub enum HookOutcome {
    Continue,
    Intercept { reason: String },
}

/// The context a receiver observes at a hook point: the execution id, the
/// named hook point and the parsed payload data.
#[derive(Debug, Clone)]
pub struct HookContext {
    pub execution_id: Id,
    pub hook_type: String,
    pub data: HashMap<String, Value>,
}

impl From<&BaseHookContext> for HookContext {
    fn from(ctx: &BaseHookContext) -> Self {
        Self {
            execution_id: ctx.execution_id.clone(),
            hook_type: String::new(),
            data: ctx.data.clone(),
        }
    }
}

impl From<&wf_types::hook::BaseHookConfig> for BaseHookDefinition {
    /// Convert a serde-facing hook config into an executable hook definition.
    ///
    /// `condition` moves from `Option<Value>` to `Option<String>` (the
    /// condition expression is a string when set); defaults mirror the agent
    /// conversion (weight 0, enabled).
    fn from(config: &wf_types::hook::BaseHookConfig) -> Self {
        Self {
            id: Id::new(),
            hook_type: config.hook_type.clone(),
            weight: config.weight.unwrap_or(0),
            condition: config
                .condition
                .as_ref()
                .and_then(|v| v.as_str())
                .map(ToString::to_string),
            enabled: config.enabled.unwrap_or(true),
            payload: config.event_payload.clone(),
            receiver: config.receiver.clone(),
        }
    }
}
