use crate::errors::WfError;

pub fn configuration_validation_error(
    message: impl Into<String>,
    config_path: Option<String>,
    config_type: Option<String>,
    field: Option<String>,
) -> WfError {
    let mut ctx = std::collections::HashMap::new();
    if let Some(v) = config_path {
        ctx.insert("config_path".into(), serde_json::Value::String(v));
    }
    if let Some(v) = config_type {
        ctx.insert("config_type".into(), serde_json::Value::String(v));
    }
    if let Some(v) = field {
        ctx.insert("field".into(), serde_json::Value::String(v));
    }
    WfError::new(message).with_context(ctx)
}

pub fn runtime_validation_error(
    message: impl Into<String>,
    operation: Option<String>,
    field: Option<String>,
) -> WfError {
    let mut ctx = std::collections::HashMap::new();
    if let Some(v) = operation {
        ctx.insert("operation".into(), serde_json::Value::String(v));
    }
    if let Some(v) = field {
        ctx.insert("field".into(), serde_json::Value::String(v));
    }
    WfError::new(message).with_context(ctx)
}

pub fn expression_security_error(message: impl Into<String>, field: Option<String>) -> WfError {
    let mut ctx = std::collections::HashMap::new();
    ctx.insert(
        "operation".into(),
        serde_json::Value::String("security".into()),
    );
    if let Some(v) = field {
        ctx.insert("field".into(), serde_json::Value::String(v));
    }
    WfError::new(message).with_context(ctx)
}
