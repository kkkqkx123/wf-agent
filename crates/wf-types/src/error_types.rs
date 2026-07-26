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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ErrorSeverity {
    #[serde(rename = "info")]
    Info,
    #[serde(rename = "warning")]
    Warning,
    #[serde(rename = "error")]
    Error,
    #[serde(rename = "critical")]
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ErrorContext {
    pub severity: ErrorSeverity,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<HashMap<String, serde_json::Value>>,
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
