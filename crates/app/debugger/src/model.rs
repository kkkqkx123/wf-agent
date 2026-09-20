use std::collections::HashMap;

use serde::{Deserialize, Serialize};

pub const TRACE_SCHEMA_V1: &str = "wf-debug-trace/v1";
pub const MAX_PAYLOAD_CHARS: usize = 4000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TraceKind {
    Workflow,
    Agent,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Trace {
    #[serde(default = "default_schema")]
    pub schema: String,
    pub kind: TraceKind,
    #[serde(default)]
    pub graph_ref: String,
    #[serde(default)]
    pub initial_variables: HashMap<String, serde_json::Value>,
    #[serde(default)]
    pub steps: Vec<StepRecord>,
    #[serde(default)]
    pub assertions: Vec<crate::assert::Assertion>,
    #[serde(default)]
    pub trigger_templates: Vec<TriggerTemplateView>,
}

fn default_schema() -> String {
    TRACE_SCHEMA_V1.to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StepRecord {
    pub index: usize,
    pub node_id: String,
    #[serde(default)]
    pub node_name: String,
    #[serde(default)]
    pub node_type: String,
    #[serde(default)]
    pub input: serde_json::Value,
    #[serde(default)]
    pub result: serde_json::Value,
    #[serde(default)]
    pub success: bool,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub error_kind: Option<String>,
    #[serde(default)]
    pub retryable: Option<bool>,
    #[serde(default)]
    pub recovery_hint: Option<String>,
    #[serde(default)]
    pub branch_id: Option<String>,
    #[serde(default)]
    pub route_target: Option<String>,
    #[serde(default)]
    pub start_time: Option<i64>,
    #[serde(default)]
    pub end_time: Option<i64>,
    #[serde(default)]
    pub variable_before: HashMap<String, serde_json::Value>,
    #[serde(default)]
    pub variable_after: HashMap<String, serde_json::Value>,
    #[serde(default)]
    pub messages_before: HashMap<String, Vec<MessageView>>,
    #[serde(default)]
    pub messages_after: HashMap<String, Vec<MessageView>>,
    #[serde(default)]
    pub tool_calls: Vec<ToolCallView>,
    #[serde(default)]
    pub llm_calls: Vec<LlmCallView>,
    #[serde(default)]
    pub approval: Option<ApprovalView>,
    #[serde(default)]
    pub visibility: Option<VisibilityView>,
    #[serde(default)]
    pub loop_round: Option<LoopRoundView>,
    #[serde(default)]
    pub merge: Option<MergeView>,
    #[serde(default)]
    pub interruption: Option<InterruptionView>,
    #[serde(default)]
    pub checkpoint: Option<CheckpointMark>,
    #[serde(default)]
    pub interaction: Option<InteractionView>,
    #[serde(default)]
    pub hooks_fired: Vec<HookFireView>,
    #[serde(default)]
    pub triggers_seen: Vec<TriggerEventView>,
    #[serde(default)]
    pub children: Vec<StepRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MessageView {
    pub id: String,
    #[serde(default)]
    pub role: String,
    #[serde(default)]
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolCallView {
    pub name: String,
    #[serde(default)]
    pub call_id: String,
    #[serde(default)]
    pub arguments: serde_json::Value,
    #[serde(default)]
    pub result: Option<serde_json::Value>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub duration_ms: Option<i64>,
    #[serde(default)]
    pub success: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LlmCallView {
    #[serde(default)]
    pub profile_id: String,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub prompt_tokens: u32,
    #[serde(default)]
    pub completion_tokens: u32,
    #[serde(default)]
    pub content_preview: Option<String>,
    #[serde(default)]
    pub tool_call_count: u32,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ApprovalView {
    pub tool_name: String,
    pub decision: String,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct VisibilityView {
    #[serde(default)]
    pub visible: Vec<String>,
    #[serde(default)]
    pub gated: Vec<String>,
    #[serde(default)]
    pub hidden: Vec<String>,
    #[serde(default)]
    pub discoverable: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LoopRoundView {
    pub loop_id: String,
    pub round: u32,
    #[serde(default)]
    pub item: Option<serde_json::Value>,
    #[serde(default)]
    pub failed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MergeView {
    pub join_node_id: String,
    #[serde(default)]
    pub branch_count: usize,
    #[serde(default)]
    pub failures_absorbed: usize,
    #[serde(default)]
    pub summary: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InterruptionView {
    #[serde(rename = "type")]
    pub interruption_type: String,
    #[serde(default)]
    pub recovered: bool,
    #[serde(default)]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CheckpointMark {
    pub timing: String,
    #[serde(default)]
    pub checkpoint_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InteractionView {
    pub interaction_id: String,
    pub prompt: String,
    #[serde(default)]
    pub pending: bool,
    #[serde(default)]
    pub response: Option<serde_json::Value>,
    #[serde(default)]
    pub timed_out: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HookFireView {
    pub hook_type: String,
    #[serde(default)]
    pub hook_id: String,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub condition: Option<String>,
    #[serde(default)]
    pub condition_matched: Option<bool>,
    #[serde(default)]
    pub payload: serde_json::Value,
    #[serde(default)]
    pub handler: Option<String>,
    #[serde(default)]
    pub outcome: String,
    #[serde(default)]
    pub veto_reason: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub duration_ms: Option<i64>,
    #[serde(default)]
    pub gate: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TriggerEventView {
    pub template_name: String,
    pub event_type: String,
    #[serde(default)]
    pub event_name: Option<String>,
    #[serde(default)]
    pub matched: bool,
    #[serde(default)]
    pub drop_reason: Option<String>,
    #[serde(default)]
    pub permit: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TriggerTemplateView {
    pub name: String,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub event_type: String,
    #[serde(default)]
    pub event_name: Option<String>,
    #[serde(default)]
    pub expression: Option<String>,
    #[serde(default)]
    pub priority: Option<i32>,
    #[serde(default)]
    pub scope: Option<String>,
    #[serde(default)]
    pub max_triggers: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RouteDecisionPoint {
    pub node_id: String,
    #[serde(default)]
    pub branches: Vec<RouteBranch>,
    #[serde(default)]
    pub default_target: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RouteBranch {
    pub target_node_id: String,
    #[serde(default)]
    pub expression: Option<String>,
}

pub fn cap_payload_text(text: &str) -> String {
    if text.len() <= MAX_PAYLOAD_CHARS {
        return text.to_string();
    }
    let mut end = MAX_PAYLOAD_CHARS;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…[truncated {} chars]", &text[..end], text.len() - end)
}

pub fn cap_json_value(value: &serde_json::Value) -> serde_json::Value {
    let text = serde_json::to_string(value).unwrap_or_default();
    if text.len() <= MAX_PAYLOAD_CHARS {
        return value.clone();
    }
    serde_json::Value::String(cap_payload_text(&text))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_defaults_to_v1() {
        let trace: Trace = serde_json::from_str(r#"{"kind":"workflow","steps":[]}"#)
            .expect("minimal trace parses");
        assert_eq!(trace.schema, TRACE_SCHEMA_V1);
        assert!(trace.steps.is_empty());
    }

    #[test]
    fn payload_cap_truncates_long_text() {
        let long = "x".repeat(MAX_PAYLOAD_CHARS + 10);
        let capped = cap_payload_text(&long);
        assert!(capped.contains("truncated"));
    }
}
