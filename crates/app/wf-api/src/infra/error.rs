use std::time::Duration;

use wf_storage::error::StorageError;
use wf_types::errors::{ErrorKind, ErrorType};

pub type ApiResult<T> = Result<T, ApiError>;

/// Stable machine-readable category of an engine failure. Transports render
/// status codes from this instead of branching on message text, so the same
/// engine error always surfaces the same HTTP status and code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApiErrorCategory {
    Validation,
    NotFound,
    Conflict,
    Cancelled,
    Timeout,
    BusinessFailure,
    Resource,
    ServiceUnavailable,
    Internal,
}

impl ApiErrorCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Validation => "validation",
            Self::NotFound => "not_found",
            Self::Conflict => "conflict",
            Self::Cancelled => "cancelled",
            Self::Timeout => "timeout",
            Self::BusinessFailure => "business_failure",
            Self::Resource => "resource",
            Self::ServiceUnavailable => "service_unavailable",
            Self::Internal => "internal",
        }
    }

    /// Project the shared error taxonomy onto the transport category.
    /// Actionable kinds pin the category directly; anything else falls back
    /// to the error type, and unknown combinations stay internal so only
    /// genuine server-side failures ever report a 500.
    pub fn from_taxonomy(kind: ErrorKind, error_type: &ErrorType) -> Self {
        if *error_type == ErrorType::Interruption {
            return Self::Cancelled;
        }
        match kind {
            ErrorKind::Validation | ErrorKind::AuthError => Self::Validation,
            ErrorKind::NotFound => Self::NotFound,
            ErrorKind::StateManagement => Self::Conflict,
            ErrorKind::Timeout => Self::Timeout,
            ErrorKind::BusinessLogic | ErrorKind::Tool => Self::BusinessFailure,
            ErrorKind::RateLimited | ErrorKind::Resource => Self::Resource,
            ErrorKind::ServiceUnavailable | ErrorKind::Network => Self::ServiceUnavailable,
            _ => match error_type {
                ErrorType::Timeout => Self::Timeout,
                ErrorType::Validation => Self::Validation,
                ErrorType::RateLimited => Self::Resource,
                ErrorType::ServiceUnavailable => Self::ServiceUnavailable,
                _ => Self::Internal,
            },
        }
    }
}

impl std::fmt::Display for ApiErrorCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

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
        /// Stable category of the failure for transport rendering.
        category: ApiErrorCategory,
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
            category: ApiErrorCategory::Internal,
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
            category: ApiErrorCategory::Internal,
            source: Some(Box::new(err)),
        }
    }

    /// Execution failure with an explicit transport category and no cause.
    pub fn execution_categorized(message: impl Into<String>, category: ApiErrorCategory) -> Self {
        ApiError::Execution {
            message: message.into(),
            category,
            source: None,
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

/// Category of a shared handler-boundary error. A business-failure node
/// failure pins the category directly: its reverse taxonomy projection is
/// `Execution`/`Internal`, which would otherwise read as a generic internal
/// error. Every other variant flows through the shared analysis so records
/// and transports classify identically.
fn shared_error_category(e: &wf_execution_shared::error::ExecutionSharedError) -> ApiErrorCategory {
    use wf_execution_shared::error::ExecutionSharedError;
    use wf_types::workflow::error_branch::NodeErrorCategory;
    match e {
        ExecutionSharedError::NodeFailure { category, .. }
            if *category == NodeErrorCategory::BusinessFailure =>
        {
            ApiErrorCategory::BusinessFailure
        }
        _ => {
            let analysis = wf_agent::error_analysis::shared_error_analysis(e);
            ApiErrorCategory::from_taxonomy(analysis.kind, &analysis.error_type)
        }
    }
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
        use wf_types::workflow::error_branch::NodeErrorCategory;
        use wf_workflow::error::WorkflowError;
        let message = e.to_string();
        let category = match &e {
            WorkflowError::NodeExecutionFailed { .. } => ApiErrorCategory::BusinessFailure,
            WorkflowError::NodeFailure { category, .. }
                if *category == NodeErrorCategory::BusinessFailure =>
            {
                ApiErrorCategory::BusinessFailure
            }
            WorkflowError::SharedError(se) => shared_error_category(se),
            _ => {
                let analysis = wf_workflow::error_analysis::analyze_workflow_error(&e);
                ApiErrorCategory::from_taxonomy(analysis.kind, &analysis.error_type)
            }
        };
        ApiError::Execution {
            message,
            category,
            source: Some(Box::new(e)),
        }
    }
}

impl From<wf_agent::error::AgentError> for ApiError {
    fn from(e: wf_agent::error::AgentError) -> Self {
        use wf_agent::error::AgentError;
        let message = e.to_string();
        let category = match &e {
            AgentError::ErrorPatternTripped(_) => ApiErrorCategory::BusinessFailure,
            AgentError::SharedError(se) => shared_error_category(se),
            _ => {
                let analysis = wf_agent::error_analysis::analyze_error(&e);
                ApiErrorCategory::from_taxonomy(analysis.kind, &analysis.error_type)
            }
        };
        ApiError::Execution {
            message,
            category,
            source: Some(Box::new(e)),
        }
    }
}

impl From<wf_execution_shared::error::ExecutionSharedError> for ApiError {
    fn from(e: wf_execution_shared::error::ExecutionSharedError) -> Self {
        let message = e.to_string();
        let category = shared_error_category(&e);
        ApiError::Execution {
            message,
            category,
            source: Some(Box::new(e)),
        }
    }
}

impl From<wf_core::error::CoreError> for ApiError {
    fn from(e: wf_core::error::CoreError) -> Self {
        use wf_core::error::CoreError;
        let message = e.to_string();
        let category = match &e {
            CoreError::Timeout(_) => ApiErrorCategory::Timeout,
            CoreError::InvalidStateTransition { .. } | CoreError::TaskConflict(_) => {
                ApiErrorCategory::Conflict
            }
            CoreError::InterruptionError(_) => ApiErrorCategory::Cancelled,
            _ => ApiErrorCategory::Internal,
        };
        ApiError::Execution {
            message,
            category,
            source: Some(Box::new(e)),
        }
    }
}

impl From<wf_core::error::EventError> for ApiError {
    fn from(e: wf_core::error::EventError) -> Self {
        ApiError::execution_with_source(e)
    }
}

impl From<wf_tools::error::ToolError> for ApiError {
    fn from(e: wf_tools::error::ToolError) -> Self {
        let message = e.to_string();
        let analysis = wf_agent::error_analysis::tool_error_analysis(&e);
        ApiError::Execution {
            message,
            category: ApiErrorCategory::from_taxonomy(analysis.kind, &analysis.error_type),
            source: Some(Box::new(e)),
        }
    }
}

impl From<wf_llm::error::LlmError> for ApiError {
    fn from(e: wf_llm::error::LlmError) -> Self {
        let message = e.to_string();
        let analysis = wf_agent::error_analysis::llm_error_analysis(&e);
        ApiError::Execution {
            message,
            category: ApiErrorCategory::from_taxonomy(analysis.kind, &analysis.error_type),
            source: Some(Box::new(e)),
        }
    }
}

impl From<serde_json::Error> for ApiError {
    fn from(e: serde_json::Error) -> Self {
        ApiError::Validation(e.to_string())
    }
}
