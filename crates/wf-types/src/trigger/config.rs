use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TriggerCondition {
    pub event_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<crate::Metadata>,
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
