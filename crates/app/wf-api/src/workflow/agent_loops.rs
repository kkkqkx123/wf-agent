use crate::infra::context::ApiContext;
use wf_execution_shared::types::execution_entity::ExecutionEntity;
use crate::infra::error::{ApiError, ApiResult};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WorkflowAgentLoop {
    pub node_id: String,
    pub agent_loop_id: String,
    pub status: wf_types::ExecutionStatus,
    pub steering_pending: usize,
    pub followup_pending: usize,
    pub append_mode: String,
}

fn mapping(ctx: &ApiContext, execution_id: &str) -> ApiResult<std::collections::BTreeMap<String, String>> {
    let entity = ctx
        .workflow_execution(execution_id)
        .ok_or_else(|| ApiError::execution_not_found(execution_id))?;
    let mut map = std::collections::BTreeMap::new();
    if let Some(entry) = entity.variables().get("__agent_loop_ids") {
        if let Some(obj) = entry.value().as_object() {
            for (k, v) in obj {
                if let Some(id) = v.as_str() {
                    map.insert(k.clone(), id.to_string());
                }
            }
        }
    }
    Ok(map)
}

pub async fn list_agent_loops(
    ctx: &ApiContext,
    execution_id: &str,
) -> ApiResult<Vec<WorkflowAgentLoop>> {
    let map = mapping(ctx, execution_id)?;
    let mut result = Vec::new();
    for (node_id, loop_id) in map {
        if let Some(entity) = ctx.agent_loop(&loop_id) {
            let (steering, followup) = entity.pending_append_counts().await;
            let mode = entity.append_mode().await;
            result.push(WorkflowAgentLoop {
                node_id,
                agent_loop_id: loop_id,
                status: entity.status().into(),
                steering_pending: steering,
                followup_pending: followup,
                append_mode: format!("{:?}", mode).to_lowercase(),
            });
        } else {
            result.push(WorkflowAgentLoop {
                node_id,
                agent_loop_id: loop_id,
                status: wf_types::ExecutionStatus::Completed,
                steering_pending: 0,
                followup_pending: 0,
                append_mode: "followup".to_string(),
            });
        }
    }
    Ok(result)
}

pub async fn append_to_agent_loop(
    ctx: &ApiContext,
    execution_id: &str,
    node_id: &str,
    text: String,
    as_mode: Option<&str>,
) -> ApiResult<()> {
    let map = mapping(ctx, execution_id)?;
    let loop_id = map
        .get(node_id)
        .ok_or_else(|| ApiError::execution_not_found(node_id))?;
    crate::agent::agent_execution::append(ctx, loop_id, text, as_mode).await
}

pub async fn set_agent_loop_append_mode(
    ctx: &ApiContext,
    execution_id: &str,
    node_id: &str,
    mode: &str,
) -> ApiResult<()> {
    let map = mapping(ctx, execution_id)?;
    let loop_id = map
        .get(node_id)
        .ok_or_else(|| ApiError::execution_not_found(node_id))?;
    crate::agent::agent_execution::set_append_mode(ctx, loop_id, mode).await
}

pub async fn convert_pending_to_steering(
    ctx: &ApiContext,
    execution_id: &str,
    node_id: &str,
) -> ApiResult<()> {
    let map = mapping(ctx, execution_id)?;
    let loop_id = map
        .get(node_id)
        .ok_or_else(|| ApiError::execution_not_found(node_id))?;
    crate::agent::agent_execution::convert_pending_to_steering(ctx, loop_id).await
}


