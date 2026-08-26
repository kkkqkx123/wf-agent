use std::time::Duration;

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
    #[error("Execution error: {message}")]
    Execution {
        message: String,
        /// The typed engine error that caused the failure, retained so callers
        /// can inspect the cause without string parsing.
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },
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

    /// Convenience constructor for a duplicate entity.
    pub fn already_exists(entity_type: &str, id: &str) -> Self {
        ApiError::AlreadyExists {
            entity_type: entity_type.to_string(),
            id: id.to_string(),
        }
    }

    /// Convenience constructor for a missing execution handle.
    pub fn execution_not_found(id: &str) -> Self {
        ApiError::ExecutionNotFound { id: id.to_string() }
    }

    /// Execution failure from a message only (no typed cause available).
    pub fn execution(message: impl Into<String>) -> Self {
        ApiError::Execution {
            message: message.into(),
            source: None,
        }
    }

    /// Execution failure retaining the typed cause as `source`.
    pub fn execution_with_source<E>(err: E) -> Self
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        ApiError::Execution {
            message: err.to_string(),
            source: Some(Box::new(err)),
        }
    }
}

/// Run `future` bounded by `duration`; an elapse maps onto `ApiError::Timeout`.
///
/// Library-level timeout primitive (the design keeps execution "capabilities"
/// such as timeout/cancel as reusable tools instead of a command layer). The
/// default execution timeouts of `workflow_execution::execute` /
/// `agent_execution::run` compose through it.
pub async fn with_timeout<F, T>(duration: Duration, future: F) -> ApiResult<T>
where
    F: std::future::Future<Output = ApiResult<T>>,
{
    match tokio::time::timeout(duration, future).await {
        Ok(result) => result,
        Err(_) => Err(ApiError::Timeout(format!(
            "operation timed out after {}ms",
            duration.as_millis()
        ))),
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
            other => ApiError::execution_with_source(other),
        }
    }
}

impl From<wf_workflow::error::WorkflowError> for ApiError {
    fn from(e: wf_workflow::error::WorkflowError) -> Self {
        ApiError::execution_with_source(e)
    }
}

impl From<wf_agent::error::AgentError> for ApiError {
    fn from(e: wf_agent::error::AgentError) -> Self {
        ApiError::execution_with_source(e)
    }
}

impl From<wf_execution_shared::error::ExecutionSharedError> for ApiError {
    fn from(e: wf_execution_shared::error::ExecutionSharedError) -> Self {
        ApiError::execution_with_source(e)
    }
}

impl From<wf_core::error::CoreError> for ApiError {
    fn from(e: wf_core::error::CoreError) -> Self {
        ApiError::execution_with_source(e)
    }
}

impl From<wf_core::error::EventError> for ApiError {
    fn from(e: wf_core::error::EventError) -> Self {
        ApiError::execution_with_source(e)
    }
}

impl From<wf_tools::error::ToolError> for ApiError {
    fn from(e: wf_tools::error::ToolError) -> Self {
        ApiError::execution_with_source(e)
    }
}

impl From<wf_llm::error::LlmError> for ApiError {
    fn from(e: wf_llm::error::LlmError) -> Self {
        use wf_llm::error::LlmError;
        match e {
            LlmError::ProfileNotFound(id) => ApiError::NotFound {
                entity_type: "profile".into(),
                id,
            },
            LlmError::ConfigError(msg) => ApiError::Validation(msg),
            LlmError::Timeout(ms) => {
                ApiError::Timeout(format!("LLM request timed out after {ms}ms"))
            }
            other => ApiError::execution_with_source(other),
        }
    }
}

impl From<serde_json::Error> for ApiError {
    fn from(e: serde_json::Error) -> Self {
        ApiError::Validation(e.to_string())
    }
}
