use crate::errors::WfError;

pub fn not_found_error(
    message: impl Into<String>,
    resource_type: String,
    resource_id: String,
) -> WfError {
    let mut ctx = std::collections::HashMap::new();
    ctx.insert(
        "resource_type".into(),
        serde_json::Value::String(resource_type),
    );
    ctx.insert("resource_id".into(), serde_json::Value::String(resource_id));
    WfError::new(message).with_context(ctx)
}

pub fn workflow_not_found_error(message: impl Into<String>, workflow_id: String) -> WfError {
    not_found_error(message, "Workflow".into(), workflow_id)
}

pub fn node_not_found_error(message: impl Into<String>, node_id: String) -> WfError {
    not_found_error(message, "Node".into(), node_id)
}

pub fn tool_not_found_error(message: impl Into<String>, tool_id: String) -> WfError {
    not_found_error(message, "Tool".into(), tool_id)
}

pub fn script_not_found_error(message: impl Into<String>, script_name: String) -> WfError {
    not_found_error(message, "Script".into(), script_name)
}

pub fn workflow_execution_not_found_error(
    message: impl Into<String>,
    execution_id: String,
) -> WfError {
    not_found_error(message, "WorkflowExecution".into(), execution_id)
}

pub fn checkpoint_not_found_error(message: impl Into<String>, checkpoint_id: String) -> WfError {
    not_found_error(message, "Checkpoint".into(), checkpoint_id)
}

pub fn trigger_template_not_found_error(
    message: impl Into<String>,
    template_name: String,
) -> WfError {
    not_found_error(message, "TriggerTemplate".into(), template_name)
}

pub fn node_template_not_found_error(message: impl Into<String>, template_name: String) -> WfError {
    not_found_error(message, "NodeTemplate".into(), template_name)
}

pub fn hook_template_not_found_error(message: impl Into<String>, template_name: String) -> WfError {
    not_found_error(message, "HookTemplate".into(), template_name)
}

pub fn agent_loop_not_found_error(message: impl Into<String>, agent_loop_id: String) -> WfError {
    not_found_error(message, "AgentLoop".into(), agent_loop_id)
}
