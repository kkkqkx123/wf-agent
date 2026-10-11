use thiserror::Error;

#[derive(Error, Debug)]
pub enum ToolError {
    #[error("Tool not found: {0}")]
    NotFound(String),

    #[error("Tool execution failed: {tool_id} - {reason}")]
    ExecutionFailed { tool_id: String, reason: String },

    #[error("Tool validation failed: {0}")]
    ValidationFailed(String),

    #[error("Tool timeout: {tool_id} after {timeout_ms}ms")]
    Timeout { tool_id: String, timeout_ms: u64 },

    #[error("Tool cancelled: {tool_id}")]
    Cancelled { tool_id: String },

    #[error("Tool retry exhausted: {tool_id} after {retries} retries")]
    RetryExhausted { tool_id: String, retries: u32 },

    #[error("MCP error: {0}")]
    McpError(String),

    #[error("MCP transport error: {0}")]
    TransportError(String),

    #[error("MCP connection failed: {server} - {reason}")]
    ConnectionFailed { server: String, reason: String },

    #[error("REST tool error: {url} - {status}")]
    RestError { url: String, status: u16 },

    #[error("HTTP error: {0}")]
    HttpError(#[from] reqwest::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Callback not registered: {0}")]
    CallbackNotRegistered(String),

    #[error("Internal error: {0}")]
    Internal(String),

    #[error("Execution error: {0}")]
    ExecutionError(String),

    #[error("Scope violation in tool '{tool_id}': {detail}")]
    ScopeViolation { tool_id: String, detail: String },
}

pub type ToolResult<T> = Result<T, ToolError>;

/// Metadata key marking tool messages that carry a scope violation. Engines
/// set it when converting a [`ToolError::ScopeViolation`] into conversation
/// text so violation rounds stay countable without string sniffing.
pub const SCOPE_VIOLATION_METADATA_KEY: &str = "scope_violation";

/// Stable machine-readable tags heading model-visible error text. Scope
/// rejections and missing files share the same shape (a failed tool call
/// with a path in it), so each class carries its own tag and readers never
/// have to guess from prose. The metadata marker stays the counting signal
/// for engines; the tag is the human and test signal.
pub const SCOPE_VIOLATION_TAG: &str = "scope_violation";
pub const NOT_FOUND_TAG: &str = "not_found";

/// Prefix a model-visible error detail with its class tag.
pub fn tag_detail(tag: &str, detail: String) -> String {
    format!("{tag}: {detail}")
}

impl ToolError {
    pub fn scope_violation(tool_id: impl Into<String>, detail: String) -> Self {
        ToolError::ScopeViolation {
            tool_id: tool_id.into(),
            detail: tag_detail(SCOPE_VIOLATION_TAG, detail),
        }
    }

    pub fn is_scope_violation(&self) -> bool {
        matches!(self, ToolError::ScopeViolation { .. })
    }
}

/// True when a tool message carries the scope violation marker.
pub fn message_marks_scope_violation(
    metadata: Option<&std::collections::HashMap<String, serde_json::Value>>,
) -> bool {
    metadata
        .and_then(|m| m.get(SCOPE_VIOLATION_METADATA_KEY))
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

impl From<wf_shell::ShellError> for ToolError {
    fn from(err: wf_shell::ShellError) -> Self {
        match err {
            wf_shell::ShellError::NotFound(msg) => ToolError::NotFound(msg),
            wf_shell::ShellError::ValidationFailed(msg) => ToolError::ValidationFailed(msg),
            wf_shell::ShellError::Internal(msg) => ToolError::Internal(msg),
            wf_shell::ShellError::ExecutionError(msg) => ToolError::ExecutionError(msg),
            wf_shell::ShellError::Io(e) => ToolError::Io(e),
        }
    }
}
