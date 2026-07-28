use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TriggerConditionSchema {
    pub condition_type: String,
    pub expression: Option<String>,
    pub threshold: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConversationHistoryOptionsSchema {
    pub max_messages: Option<u32>,
    pub include_tools: Option<bool>,
    pub include_system: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StopWorkflowExecutionActionParametersSchema {
    pub workflow_id: Option<String>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PauseWorkflowExecutionActionParametersSchema {
    pub workflow_id: Option<String>,
    pub duration_seconds: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResumeWorkflowExecutionActionParametersSchema {
    pub workflow_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SkipNodeActionParametersSchema {
    pub node_id: String,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SetVariableActionParametersSchema {
    pub variable_name: String,
    pub value: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SendNotificationActionParametersSchema {
    pub message: String,
    pub level: Option<String>,
    pub target: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ApplyMessageOperationActionParametersSchema {
    pub operation: String,
    pub target: Option<String>,
    pub parameters: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExecuteTriggeredSubworkflowActionConfigSchema {
    pub workflow_id: String,
    pub input: Option<serde_json::Value>,
    pub wait_for_completion: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExecuteScriptActionConfigSchema {
    pub script_id: String,
    pub arguments: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "action_type", rename_all = "snake_case")]
pub enum TriggerActionSchema {
    StopWorkflow(StopWorkflowExecutionActionParametersSchema),
    PauseWorkflow(PauseWorkflowExecutionActionParametersSchema),
    ResumeWorkflow(ResumeWorkflowExecutionActionParametersSchema),
    SkipNode(SkipNodeActionParametersSchema),
    SetVariable(SetVariableActionParametersSchema),
    SendNotification(SendNotificationActionParametersSchema),
    ApplyMessageOperation(ApplyMessageOperationActionParametersSchema),
    ExecuteTriggeredSubworkflow(ExecuteTriggeredSubworkflowActionConfigSchema),
    ExecuteScript(ExecuteScriptActionConfigSchema),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowTriggerSchema {
    pub name: String,
    pub description: Option<String>,
    pub enabled: bool,
    pub condition: TriggerConditionSchema,
    pub actions: Vec<TriggerActionSchema>,
    pub conversation_history_options: Option<ConversationHistoryOptionsSchema>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TriggerConfigOverrideSchema {
    pub enabled: Option<bool>,
    pub condition: Option<TriggerConditionSchema>,
    pub actions: Option<Vec<TriggerActionSchema>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TriggerReferenceSchema {
    pub trigger_id: String,
    pub overrides: Option<TriggerConfigOverrideSchema>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TriggerTemplateSchema {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub definition: WorkflowTriggerSchema,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VariableCallbackConfigSchema {
    pub variable_name: String,
    pub callback_url: Option<String>,
    pub callback_script: Option<String>,
}
