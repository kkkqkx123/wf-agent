use wf_storage::error::StorageError;

pub type ApiResult<T> = Result<T, ApiError>;

/// Unified error type of the application-facing API layer.
///
/// Maps engine (`wf-workflow` / `wf-agent`) and storage failures onto a
/// small set of stable categories so any transport (server/CLI) can render
/// consistent status codes.
#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("Storage error: {0}")]
    Storage(#[from] StorageError),
    #[error("Not found: {entity_type} [{id}]")]
    NotFound { entity_type: String, id: String },
    #[error("Validation error: {0}")]
    Validation(String),
    #[error("Already exists: {entity_type} [{id}]")]
    AlreadyExists { entity_type: String, id: String },
    #[error("Execution error: {0}")]
    Execution(String),
    #[error("Execution not found: {id}")]
    ExecutionNotFound { id: String },
    #[error("Timeout: {0}")]
    Timeout(String),
    #[error("Conflict: {0}")]
    Conflict(String),
}

impl ApiError {
    /// Convenience constructor for a not-found entity.
    pub fn not_found(entity_type: &str, id: &str) -> Self {
        ApiError::NotFound {
            entity_type: entity_type.to_string(),
            id: id.to_string(),
        }
    }

    /// Convenience constructor for a missing execution handle.
    pub fn execution_not_found(id: &str) -> Self {
        ApiError::ExecutionNotFound { id: id.to_string() }
    }
}

/// Internal shorthand used by the storage CRUD modules.
pub(crate) fn not_found(entity_type: &str, id: &str) -> ApiError {
    ApiError::not_found(entity_type, id)
}

impl From<wf_config::error::ConfigError> for ApiError {
    fn from(e: wf_config::error::ConfigError) -> Self {
        match e {
            wf_config::error::ConfigError::Parse(msg)
            | wf_config::error::ConfigError::Validation(msg) => ApiError::Validation(msg),
            other => ApiError::Execution(other.to_string()),
        }
    }
}

impl From<wf_workflow::error::WorkflowError> for ApiError {
    fn from(e: wf_workflow::error::WorkflowError) -> Self {
        ApiError::Execution(e.to_string())
    }
}

impl From<wf_agent::error::AgentError> for ApiError {
    fn from(e: wf_agent::error::AgentError) -> Self {
        ApiError::Execution(e.to_string())
    }
}

impl From<wf_core::error::CoreError> for ApiError {
    fn from(e: wf_core::error::CoreError) -> Self {
        ApiError::Execution(e.to_string())
    }
}

impl From<wf_core::error::EventError> for ApiError {
    fn from(e: wf_core::error::EventError) -> Self {
        ApiError::Execution(e.to_string())
    }
}

impl From<wf_tools::error::ToolError> for ApiError {
    fn from(e: wf_tools::error::ToolError) -> Self {
        ApiError::Execution(e.to_string())
    }
}
