#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WfError {
    pub code: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<HashMap<String, serde_json::Value>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stack: Option<String>,
}

impl WfError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            details: None,
            stack: None,
        }
    }
}

// ============================================================================
// Specific Error Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ExecutionError {
    NodeExecutionFailed(String),
    WorkflowTimeout(String),
    InvalidTransition(String),
    MaxRetriesExceeded(String),
    BranchExecutionFailed(String),
    JoinTimeout(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ValidationError {
    InvalidNodeType(String),
    MissingRequiredField(String),
    InvalidEdge(String),
    CyclicDependency(String),
    InvalidConfig(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ToolError {
    ToolNotFound(String),
    ToolExecutionFailed(String),
    ToolApprovalDenied(String),
    ToolTimeout(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum StorageError {
    EntityNotFound(String),
    EntityAlreadyExists(String),
    StorageConnectionFailed(String),
    StorageWriteFailed(String),
    StorageReadFailed(String),
}
