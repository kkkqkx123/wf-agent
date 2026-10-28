use crate::infra::context::ApiContext;
use crate::infra::error::{ApiError, ApiResult};

pub use wf_workflow::entity::{PendingVariableInjection, VariableInjectionPolicy};

/// Buffer an externally submitted variable update on a live workflow
/// execution. The update waits for its application policy instead of
/// touching the running nodes. `policy` accepts `next_boundary` (default)
/// or `on_resume`; `source` labels the calling channel for the audit trail.
pub async fn submit_variable_injection(
    ctx: &ApiContext,
    execution_id: &str,
    variable_name: String,
    value: serde_json::Value,
    source: Option<&str>,
    policy: Option<&str>,
) -> ApiResult<PendingVariableInjection> {
    let entity = ctx
        .workflow_execution(execution_id)
        .ok_or_else(|| ApiError::execution_not_found(execution_id))?;
    let policy = match policy {
        None => None,
        Some(raw) => Some(VariableInjectionPolicy::parse(Some(raw)).ok_or_else(|| {
            ApiError::Validation(format!("unknown variable injection policy: {raw}"))
        })?),
    };
    entity
        .submit_variable_injection(variable_name, value, source, policy)
        .map_err(ApiError::Validation)
}

/// Pending updates of a live workflow execution, oldest first.
pub async fn list_variable_injections(
    ctx: &ApiContext,
    execution_id: &str,
) -> ApiResult<Vec<PendingVariableInjection>> {
    let entity = ctx
        .workflow_execution(execution_id)
        .ok_or_else(|| ApiError::execution_not_found(execution_id))?;
    Ok(entity.list_variable_injections())
}

/// Drop one pending update. Applied values are never removed this way;
/// only the buffered entry goes away.
pub async fn remove_variable_injection(
    ctx: &ApiContext,
    execution_id: &str,
    injection_id: &str,
) -> ApiResult<()> {
    let entity = ctx
        .workflow_execution(execution_id)
        .ok_or_else(|| ApiError::execution_not_found(execution_id))?;
    if entity.remove_variable_injection(injection_id) {
        Ok(())
    } else {
        Err(ApiError::not_found("variable_injection", injection_id))
    }
}
