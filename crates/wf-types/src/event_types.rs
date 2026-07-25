#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "event", content = "data", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WorkflowEvent {
    WorkflowStarted(WorkflowStartedData),
    WorkflowCompleted(WorkflowCompletedData),
    WorkflowFailed(WorkflowFailedData),
    WorkflowPaused(WorkflowPausedData),
    NodeStarted(NodeStartedData),
    NodeCompleted(NodeCompletedData),
    NodeFailed(NodeFailedData),
    ToolCalled(ToolCalledData),
    ToolCompleted(ToolCompletedData),
    ToolFailed(ToolFailedData),
    LlmRequested(LlmRequestedData),
    LlmResponseReceived(LlmResponseReceivedData),
    CheckpointCreated(CheckpointCreatedData),
    CheckpointRestored(CheckpointRestoredData),
    BranchStarted(BranchStartedData),
    BranchCompleted(BranchCompletedData),
    UserInteractionRequested(UserInteractionRequestedData),
    UserInteractionCompleted(UserInteractionCompletedData),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowStartedData {
    pub workflow_id: String,
    pub execution_id: String,
    pub session_id: String,
    pub timestamp: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowCompletedData {
    pub workflow_id: String,
    pub execution_id: String,
    pub session_id: String,
    pub timestamp: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowFailedData {
    pub workflow_id: String,
    pub execution_id: String,
    pub session_id: String,
    pub timestamp: i64,
    pub error: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowPausedData {
    pub workflow_id: String,
    pub execution_id: String,
    pub session_id: String,
    pub timestamp: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NodeStartedData {
    pub node_id: String,
    pub node_type: String,
    pub execution_id: String,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NodeCompletedData {
    pub node_id: String,
    pub execution_id: String,
    pub timestamp: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NodeFailedData {
    pub node_id: String,
    pub execution_id: String,
    pub timestamp: i64,
    pub error: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ToolCalledData {
    pub tool_name: String,
    pub execution_id: String,
    pub timestamp: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arguments: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ToolCompletedData {
    pub tool_name: String,
    pub execution_id: String,
    pub timestamp: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ToolFailedData {
    pub tool_name: String,
    pub execution_id: String,
    pub timestamp: i64,
    pub error: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LlmRequestedData {
    pub profile_id: String,
    pub execution_id: String,
    pub timestamp: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LlmResponseReceivedData {
    pub profile_id: String,
    pub execution_id: String,
    pub timestamp: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CheckpointCreatedData {
    pub checkpoint_id: String,
    pub entity_id: String,
    pub entity_type: String,
    pub timestamp: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CheckpointRestoredData {
    pub checkpoint_id: String,
    pub entity_id: String,
    pub entity_type: String,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BranchStartedData {
    pub branch_id: String,
    pub execution_id: String,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BranchCompletedData {
    pub branch_id: String,
    pub execution_id: String,
    pub timestamp: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UserInteractionRequestedData {
    pub interaction_id: String,
    pub execution_id: String,
    pub timestamp: i64,
    pub prompt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UserInteractionCompletedData {
    pub interaction_id: String,
    pub execution_id: String,
    pub timestamp: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
}
