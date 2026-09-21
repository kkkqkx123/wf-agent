use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::views::{
    ApprovalView, CheckpointMark, HookFireView, InteractionView, InterruptionView, LlmCallView,
    LoopRoundView, MergeView, MessageView, ToolCallView, TriggerEventView, VisibilityView,
};

/// Core identity, inputs, outputs and nested structure of one recorded step.
/// Dimension-specific detail lives in the view structs; this record only
/// references them so new dimensions do not reshape the core.
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
    // Sub-execution hierarchy. `depth` mirrors the engine entity depth so the
    // analyzer can validate it against the actual nesting position.
    #[serde(default)]
    pub exec_id: Option<String>,
    #[serde(default)]
    pub parent_exec_id: Option<String>,
    #[serde(default)]
    pub root_exec_id: Option<String>,
    #[serde(default)]
    pub depth: Option<u32>,
    #[serde(default)]
    pub result_var: Option<String>,
    #[serde(default)]
    pub wait_for_child: Option<bool>,
    #[serde(default)]
    pub child_timeout_ms: Option<i64>,
    #[serde(default)]
    pub dialog_anchor: Option<String>,
    #[serde(default)]
    pub writeback: Option<String>,
    #[serde(default)]
    pub children: Vec<StepRecord>,
}
