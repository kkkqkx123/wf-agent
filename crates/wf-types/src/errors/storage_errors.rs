use crate::errors::ErrorSeverity;
use crate::errors::WfError;

pub fn configuration_error(message: impl Into<String>, config_key: Option<String>) -> WfError {
    let mut ctx = std::collections::HashMap::new();
    if let Some(v) = config_key {
        ctx.insert("config_key".into(), serde_json::Value::String(v));
    }
    WfError::new(message)
        .with_severity(ErrorSeverity::Error)
        .with_context(ctx)
}

pub fn timeout_error(message: impl Into<String>, timeout_ms: u64) -> WfError {
    let mut ctx = std::collections::HashMap::new();
    ctx.insert(
        "timeout".into(),
        serde_json::Value::Number(timeout_ms.into()),
    );
    WfError::new(message)
        .with_severity(ErrorSeverity::Warning)
        .with_context(ctx)
}

pub fn tool_error(
    message: impl Into<String>,
    tool_id: Option<String>,
    tool_type: Option<String>,
) -> WfError {
    let mut ctx = std::collections::HashMap::new();
    if let Some(v) = tool_id {
        ctx.insert("tool_id".into(), serde_json::Value::String(v));
    }
    if let Some(v) = tool_type {
        ctx.insert("tool_type".into(), serde_json::Value::String(v));
    }
    WfError::new(message)
        .with_severity(ErrorSeverity::Warning)
        .with_context(ctx)
}

pub fn script_execution_error(message: impl Into<String>, script_name: Option<String>) -> WfError {
    let mut ctx = std::collections::HashMap::new();
    if let Some(v) = script_name {
        ctx.insert("script_name".into(), serde_json::Value::String(v));
    }
    WfError::new(message).with_context(ctx)
}
