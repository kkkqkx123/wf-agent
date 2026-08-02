use serde::{Deserialize, Serialize};

/// Condition matching an event against a trigger template.
///
/// Matching semantics (backward compatible):
/// - `event_type` must equal the event's canonical name;
/// - `metadata` pairs are matched with AND semantics. Values are matched by
///   exact equality, except for the string conventions below (checked only
///   when the expected value is a JSON string):
///   - numeric comparison: `">=10000"`, `"<=5000"`, `">100"`, `"<50"` —
///     compares the event value numerically;
///   - prefix: `"^agent-"` — matches when the event string value starts
///     with the suffix after `^`;
/// - `metadata_exists` lists keys that must be present regardless of value;
/// - `execution_prefix` matches when either the event `execution_id` or
///   `agent_loop_id` starts with the prefix (routing by execution /
///   agent-loop family).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TriggerCondition {
    pub event_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<crate::Metadata>,
    /// Keys that must exist in the event metadata (value ignored).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata_exists: Option<Vec<String>>,
    /// Prefix match on the event `execution_id` / `agent_loop_id`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution_prefix: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "action_type", rename_all = "snake_case")]
pub enum TriggerAction {
    StopWorkflowExecution {},
    PauseWorkflowExecution {},
    ResumeWorkflowExecution {},
    SkipNode {
        #[serde(skip_serializing_if = "Option::is_none")]
        node_id: Option<String>,
    },
    SetVariable {
        variable_name: String,
        value: serde_json::Value,
    },
    SendNotification {
        message: String,
    },
    ExecuteTriggeredSubworkflow {
        triggered_workflow_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        wait_for_completion: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        timeout: Option<u64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        input_mapping: Option<crate::Metadata>,
        #[serde(skip_serializing_if = "Option::is_none")]
        output_mapping: Option<crate::Metadata>,
    },
    ExecuteScript {
        script_name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        parameters: Option<serde_json::Value>,
        #[serde(skip_serializing_if = "Option::is_none")]
        timeout: Option<u64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        ignore_error: Option<bool>,
    },
}
