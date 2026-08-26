use crate::errors::WfError;

pub fn network_error(message: impl Into<String>) -> WfError {
    WfError::new(message)
}

pub fn http_error(message: impl Into<String>, status_code: u16) -> WfError {
    let mut ctx = std::collections::HashMap::new();
    ctx.insert(
        "status_code".into(),
        serde_json::Value::Number(status_code.into()),
    );
    WfError::new(message).with_context(ctx)
}

pub fn llm_error(
    message: impl Into<String>,
    provider: String,
    model: Option<String>,
    error_type: Option<String>,
    status_code: Option<u16>,
) -> WfError {
    let mut ctx = std::collections::HashMap::new();
    ctx.insert("provider".into(), serde_json::Value::String(provider));
    if let Some(v) = model {
        ctx.insert("model".into(), serde_json::Value::String(v));
    }
    if let Some(ref v) = error_type {
        ctx.insert("type".into(), serde_json::Value::String(v.clone()));
    }
    if let Some(v) = status_code {
        ctx.insert("status_code".into(), serde_json::Value::Number(v.into()));
    }

    WfError::new(message).with_context(ctx)
}

pub fn circuit_breaker_open_error(message: impl Into<String>, state: Option<String>) -> WfError {
    let mut ctx = std::collections::HashMap::new();
    if let Some(v) = state {
        ctx.insert("state".into(), serde_json::Value::String(v));
    }
    WfError::new(message).with_context(ctx)
}
