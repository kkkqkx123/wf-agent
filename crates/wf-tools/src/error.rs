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

    #[error("Callback already registered")]
    AlreadyRegistered,

    #[error("Internal error: {0}")]
    Internal(String),

    #[error("Execution error: {0}")]
    ExecutionError(String),
}

pub type ToolResult<T> = Result<T, ToolError>;
