#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ErrorType {
    ToolError,
    LlmError,
    Timeout,
    Validation,
    Internal,
    Interruption,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ErrorSeverity {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum RecoveryAction {
    Retry,
    Fallback,
    ManualIntervention,
    Abort,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ErrorCause {
    pub reason: String,
    pub handling_attempt: Option<String>,
}
