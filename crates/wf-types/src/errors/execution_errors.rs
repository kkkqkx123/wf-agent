use crate::errors::WfError;

pub fn business_logic_error(
    message: impl Into<String>,
    business_context: Option<String>,
    rule_name: Option<String>,
) -> WfError {
    let mut ctx = std::collections::HashMap::new();
    if let Some(v) = business_context {
        ctx.insert("business_context".into(), serde_json::Value::String(v));
    }
    if let Some(v) = rule_name {
        ctx.insert("rule_name".into(), serde_json::Value::String(v));
    }
    WfError::new(message).with_context(ctx)
}

pub fn system_execution_error(
    message: impl Into<String>,
    system_component: Option<String>,
    failure_point: Option<String>,
) -> WfError {
    let mut ctx = std::collections::HashMap::new();
    if let Some(v) = system_component {
        ctx.insert("system_component".into(), serde_json::Value::String(v));
    }
    if let Some(v) = failure_point {
        ctx.insert("failure_point".into(), serde_json::Value::String(v));
    }
    WfError::new(message).with_context(ctx)
}

pub fn dependency_injection_error(
    message: impl Into<String>,
    dependency_name: String,
    required_by: Option<String>,
) -> WfError {
    let mut ctx = std::collections::HashMap::new();
    ctx.insert(
        "dependency_name".into(),
        serde_json::Value::String(dependency_name),
    );
    if let Some(v) = required_by {
        ctx.insert("required_by".into(), serde_json::Value::String(v));
    }
    WfError::new(message).with_context(ctx)
}

pub fn state_management_error(
    message: impl Into<String>,
    state_type: String,
    operation: String,
    state_key: Option<String>,
) -> WfError {
    let mut ctx = std::collections::HashMap::new();
    ctx.insert("state_type".into(), serde_json::Value::String(state_type));
    ctx.insert("operation".into(), serde_json::Value::String(operation));
    if let Some(v) = state_key {
        ctx.insert("state_key".into(), serde_json::Value::String(v));
    }
    WfError::new(message).with_context(ctx)
}

pub fn agent_checkpoint_error(
    message: impl Into<String>,
    operation: String,
    checkpoint_id: Option<String>,
    agent_loop_id: Option<String>,
) -> WfError {
    let mut ctx = std::collections::HashMap::new();
    ctx.insert("operation".into(), serde_json::Value::String(operation));
    if let Some(v) = checkpoint_id {
        ctx.insert("checkpoint_id".into(), serde_json::Value::String(v));
    }
    if let Some(v) = agent_loop_id {
        ctx.insert("agent_loop_id".into(), serde_json::Value::String(v));
    }
    WfError::new(message).with_context(ctx)
}

pub fn workflow_checkpoint_error(
    message: impl Into<String>,
    operation: String,
    checkpoint_id: Option<String>,
    execution_id: Option<String>,
) -> WfError {
    let mut ctx = std::collections::HashMap::new();
    ctx.insert("operation".into(), serde_json::Value::String(operation));
    if let Some(v) = checkpoint_id {
        ctx.insert("checkpoint_id".into(), serde_json::Value::String(v));
    }
    if let Some(v) = execution_id {
        ctx.insert("execution_id".into(), serde_json::Value::String(v));
    }
    WfError::new(message).with_context(ctx)
}

pub fn event_system_error(
    message: impl Into<String>,
    operation: String,
    event_type: Option<String>,
) -> WfError {
    let mut ctx = std::collections::HashMap::new();
    ctx.insert("operation".into(), serde_json::Value::String(operation));
    if let Some(v) = event_type {
        ctx.insert("event_type".into(), serde_json::Value::String(v));
    }
    WfError::new(message).with_context(ctx)
}
