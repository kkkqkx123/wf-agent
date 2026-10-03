use std::collections::HashMap;

use serde_json::Value;
use wf_types::Id;

pub use wf_types::hook::{
    hook_allows_trigger, hook_effect, hook_requires_handler, is_known_hook_point, AGENT_HOOK_TYPES,
    WORKFLOW_HOOK_TYPES,
};

/// Canonical hook context data keys. Emitters build their payloads through
/// [`HookContext`] constructors so condition expressions and payload
/// templates observe one stable vocabulary instead of hand-written strings.
pub const KEY_EXECUTION_ID: &str = "execution_id";
pub const KEY_WORKFLOW_ID: &str = "workflow_id";
pub const KEY_HOOK_TYPE: &str = "hook_type";
pub const KEY_STATUS: &str = "status";
pub const KEY_CURRENT_ITERATION: &str = "current_iteration";
pub const KEY_NODE_ID: &str = "node_id";
pub const KEY_NODE_NAME: &str = "node_name";
pub const KEY_NODE_TYPE: &str = "node_type";
pub const KEY_DURATION_MS: &str = "duration_ms";
pub const KEY_ERROR: &str = "error";
pub const KEY_REJECTION_SOURCE: &str = "rejection_source";

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct HookDefinition {
    pub id: Id,
    pub hook_type: String,
    pub priority: i32,
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
    /// Opt-in checkpoint mark carried from the config spec. When
    /// `Some(true)` the engine creates a strategy-gated checkpoint after
    /// the hook fires; `None`/false means no checkpoint request.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub create_checkpoint: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checkpoint_description: Option<String>,
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
    /// rejection). Unresolvable handlers still resolve to `Continue`:
    /// gates fail open on configuration gaps; a handler that wants to deny
    /// on its own slow path returns a `Veto` itself.
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
/// named hook point, the parsed payload data, and the owning execution's
/// cancellation signal.
#[derive(Debug, Clone)]
pub struct HookContext {
    pub execution_id: Id,
    pub hook_type: String,
    pub data: HashMap<String, Value>,
    /// Abort signal of the owning execution. The pipeline enforces the
    /// handler-declared timeout and races every notification against this
    /// token so a misbehaving handler cannot outlive the execution, but a
    /// cooperating handler still races its own waits against it and applies
    /// its own deadline policy; unbounded human interaction does not belong
    /// in a hook handler at all, it belongs to the approval and suspend
    /// mechanisms.
    pub cancellation: tokio_util::sync::CancellationToken,
}

impl HookContext {
    /// Base constructor. The payload map always carries the execution id
    /// and hook type so condition expressions observe one stable vocabulary
    /// even when callers build their data maps by hand.
    pub fn new(
        execution_id: Id,
        hook_type: String,
        mut data: HashMap<String, Value>,
        cancellation: tokio_util::sync::CancellationToken,
    ) -> Self {
        data.entry(KEY_EXECUTION_ID.to_string())
            .or_insert_with(|| Value::String(execution_id.to_string()));
        data.entry(KEY_HOOK_TYPE.to_string())
            .or_insert_with(|| Value::String(hook_type.clone()));
        Self {
            execution_id,
            hook_type,
            data,
            cancellation,
        }
    }

    /// Agent loop base payload: execution id, status text, iteration count,
    /// plus caller extra data. Extra data never removes the base keys.
    #[allow(clippy::too_many_arguments)]
    pub fn agent_base(
        execution_id: Id,
        hook_type: String,
        status: String,
        current_iteration: u32,
        mut extra_data: HashMap<String, Value>,
        cancellation: tokio_util::sync::CancellationToken,
    ) -> Self {
        let mut data = HashMap::new();
        data.insert(
            KEY_EXECUTION_ID.to_string(),
            Value::String(execution_id.to_string()),
        );
        data.insert(KEY_STATUS.to_string(), Value::String(status));
        data.insert(
            KEY_CURRENT_ITERATION.to_string(),
            Value::Number(serde_json::Number::from(current_iteration)),
        );
        for (key, value) in extra_data.drain() {
            data.entry(key).or_insert(value);
        }
        Self::new(execution_id, hook_type, data, cancellation)
    }

    /// Workflow execution base payload: execution id, workflow id, status
    /// text, plus caller extra data.
    pub fn workflow_base(
        execution_id: Id,
        workflow_id: Id,
        hook_type: String,
        status: String,
        mut extra_data: HashMap<String, Value>,
        cancellation: tokio_util::sync::CancellationToken,
    ) -> Self {
        let mut data = HashMap::new();
        data.insert(
            KEY_EXECUTION_ID.to_string(),
            Value::String(execution_id.to_string()),
        );
        data.insert(
            KEY_WORKFLOW_ID.to_string(),
            Value::String(workflow_id.to_string()),
        );
        data.insert(KEY_STATUS.to_string(), Value::String(status));
        for (key, value) in extra_data.drain() {
            data.entry(key).or_insert(value);
        }
        Self::new(execution_id, hook_type, data, cancellation)
    }

    /// Workflow node payload: workflow base plus node identity and optional
    /// duration, error and rejection source.
    #[allow(clippy::too_many_arguments)]
    pub fn workflow_node(
        execution_id: Id,
        workflow_id: Id,
        hook_type: String,
        status: String,
        node_id: &str,
        node_name: &str,
        node_type: &str,
        duration_ms: Option<i64>,
        error: Option<&str>,
        rejection_source: Option<&str>,
        mut extra_data: HashMap<String, Value>,
        cancellation: tokio_util::sync::CancellationToken,
    ) -> Self {
        let mut data = HashMap::new();
        data.insert(
            KEY_EXECUTION_ID.to_string(),
            Value::String(execution_id.to_string()),
        );
        data.insert(
            KEY_WORKFLOW_ID.to_string(),
            Value::String(workflow_id.to_string()),
        );
        data.insert(KEY_STATUS.to_string(), Value::String(status));
        data.insert(KEY_NODE_ID.to_string(), Value::String(node_id.to_string()));
        data.insert(
            KEY_NODE_NAME.to_string(),
            Value::String(node_name.to_string()),
        );
        data.insert(
            KEY_NODE_TYPE.to_string(),
            Value::String(node_type.to_string()),
        );
        if let Some(duration) = duration_ms {
            data.insert(KEY_DURATION_MS.to_string(), Value::Number(duration.into()));
        }
        if let Some(err) = error {
            data.insert(KEY_ERROR.to_string(), Value::String(err.to_string()));
        }
        if let Some(source) = rejection_source {
            data.insert(
                KEY_REJECTION_SOURCE.to_string(),
                Value::String(source.to_string()),
            );
        }
        for (key, value) in extra_data.drain() {
            data.entry(key).or_insert(value);
        }
        Self::new(execution_id, hook_type, data, cancellation)
    }
}

impl From<&wf_types::hook::HookPointConfig> for HookDefinition {
    /// Convert a serde-facing hook config into an executable hook definition.
    ///
    /// Thin adapter over the authoritative spec (`CanonicalHookSpec` holds
    /// the field semantics): defaults mirror the agent conversion (priority 0,
    /// enabled).
    fn from(config: &wf_types::hook::HookPointConfig) -> Self {
        Self::from(&wf_types::hook::CanonicalHookSpec::from_workflow(config))
    }
}

impl From<&wf_types::hook::CanonicalHookSpec> for HookDefinition {
    /// Build an executable definition from the authoritative spec.
    fn from(spec: &wf_types::hook::CanonicalHookSpec) -> Self {
        Self {
            id: Id::new(),
            hook_type: spec.hook_type.clone(),
            priority: spec.priority,
            condition: spec.condition.clone(),
            enabled: spec.enabled,
            payload: spec.payload.clone(),
            handler: spec.handler.clone(),
            create_checkpoint: spec.create_checkpoint,
            checkpoint_description: spec.checkpoint_description.clone(),
        }
    }
}

impl From<&wf_types::hook::HookPointStaticConfig> for HookDefinition {
    /// Static form: thin adapter via the authoritative spec.
    fn from(config: &wf_types::hook::HookPointStaticConfig) -> Self {
        Self::from(&wf_types::hook::CanonicalHookSpec::from_static(config))
    }
}

impl From<&wf_types::agent::AgentHookConfig> for HookDefinition {
    /// Agent form: thin adapter via the authoritative spec.
    fn from(config: &wf_types::agent::AgentHookConfig) -> Self {
        Self::from(&wf_types::hook::CanonicalHookSpec::from_agent(config))
    }
}

impl From<&wf_tools::callback::HookConfig> for HookDefinition {
    /// Tool-callback form: thin adapter via the authoritative spec. The input
    /// source is model output, so the authoritative load-time rules are
    /// re-applied here as structured warnings (priority floor, unknown-type
    /// audit vacuum, empty handler name): invalid values are clamped to the
    /// validated shape instead of entering the fire pipeline unchecked.
    fn from(config: &wf_tools::callback::HookConfig) -> Self {
        let spec = config.to_canonical();
        if !is_known_hook_point(&spec.hook_type) {
            tracing::warn!(
                hook_type = %spec.hook_type,
                source = "tool-callback",
                "tool-callback hook references unknown hook type; allowing registration but it will never fire"
            );
        }
        if wf_types::hook::validate_hook_priority(spec.priority).is_err() {
            tracing::warn!(
                hook_type = %spec.hook_type,
                priority = spec.priority,
                source = "tool-callback",
                "tool-callback hook priority below 0 clamped to 0"
            );
        }
        if wf_types::hook::validate_hook_handler_name(spec.handler.as_deref()).is_err() {
            tracing::warn!(
                hook_type = %spec.hook_type,
                source = "tool-callback",
                "tool-callback hook handler is empty and will be ignored"
            );
        }
        let mut def = HookDefinition::from(&spec);
        if def.priority < 0 {
            def.priority = 0;
        }
        if def.handler.as_deref().is_some_and(|h| h.trim().is_empty()) {
            def.handler = None;
        }
        def
    }
}
