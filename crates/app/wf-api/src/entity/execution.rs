use wf_storage::adapter::base::BaseStorageAdapter;
use wf_storage::adapter::checkpoint::CheckpointStorageAdapter;

use crate::infra::context::ApiContext;
use crate::infra::error::{ApiError, ApiResult};

/// Execution domain an id resolves to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExecutionDomain {
    AgentLoop,
    Workflow,
}

impl ExecutionDomain {
    pub fn as_str(self) -> &'static str {
        match self {
            ExecutionDomain::AgentLoop => "agent_loop",
            ExecutionDomain::Workflow => "workflow",
        }
    }

    pub fn checkpoint_entity_type(self) -> &'static str {
        match self {
            ExecutionDomain::AgentLoop => "agent_loop",
            ExecutionDomain::Workflow => "checkpoint",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "agent" | "agent_loop" | "agent-loop" | "agentloop" => Some(ExecutionDomain::AgentLoop),
            "workflow" | "workflows" => Some(ExecutionDomain::Workflow),
            _ => None,
        }
    }
}

impl std::fmt::Display for ExecutionDomain {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Whether the id exists in the agent-loop domain, probed only when the
/// cheaper layers (live registry, persisted record) found nothing: live
/// registry, persisted record, then the checkpoint record store filtered by
/// the domain's `entity_type` (`agent_loop`).
async fn agent_present(ctx: &ApiContext, id: &str) -> ApiResult<bool> {
    if ctx.agent_loop(id).is_some() {
        return Ok(true);
    }
    if ctx.storage.agent_execution.load(id).await?.is_some() {
        return Ok(true);
    }
    let list = ctx
        .storage
        .checkpoint
        .list_by_entity(id, ExecutionDomain::AgentLoop.checkpoint_entity_type())
        .await?;
    Ok(!list.is_empty())
}

/// Workflow-domain counterpart of [`agent_present`], same cost order. The
/// checkpoint record store is filtered by the workflow `entity_type`
/// (`checkpoint`) so an agent ghost never resolves as a workflow.
async fn workflow_present(ctx: &ApiContext, id: &str) -> ApiResult<bool> {
    if ctx.workflow_execution(id).is_some() {
        return Ok(true);
    }
    if ctx.storage.workflow_execution.load(id).await?.is_some() {
        return Ok(true);
    }
    let list = ctx
        .storage
        .checkpoint
        .list_by_entity(id, ExecutionDomain::Workflow.checkpoint_entity_type())
        .await?;
    Ok(!list.is_empty())
}

/// Resolve a bare execution id to its domain.
///
/// Lookup order is cost-ordered: the live agent loop registry is checked
/// first, then persisted records and checkpoint records for both domains.
/// Both domains hit means the id is ambiguous and a `Conflict` is returned;
/// neither hit means a unified `ExecutionNotFound`.
pub async fn resolve_execution(ctx: &ApiContext, id: &str) -> ApiResult<ExecutionDomain> {
    resolve_execution_with_override(ctx, id, None).await
}

/// Verdict for a resolved (agent, workflow) presence pair under an optional
/// explicit domain override.
fn execution_verdict(
    id: &str,
    agent: bool,
    workflow: bool,
    domain: Option<ExecutionDomain>,
) -> ApiResult<ExecutionDomain> {
    match (agent, workflow, domain) {
        (false, false, _) => Err(ApiError::execution_not_found(id)),
        (true, true, None) => Err(ApiError::Conflict(format!(
            "execution id [{id}] is ambiguous: present as both agent_loop and workflow; specify --domain explicitly"
        ))),
        (true, true, Some(d)) => Ok(d),
        (true, false, None) => Ok(ExecutionDomain::AgentLoop),
        (false, true, None) => Ok(ExecutionDomain::Workflow),
        (true, false, Some(ExecutionDomain::AgentLoop)) => Ok(ExecutionDomain::AgentLoop),
        (false, true, Some(ExecutionDomain::Workflow)) => Ok(ExecutionDomain::Workflow),
        (true, false, Some(other)) => Err(ApiError::Validation(format!(
            "execution id [{id}] belongs to agent_loop, not {}",
            other.as_str()
        ))),
        (false, true, Some(other)) => Err(ApiError::Validation(format!(
            "execution id [{id}] belongs to workflow, not {}",
            other.as_str()
        ))),
    }
}

/// Resolve with an explicit domain override.
///
/// Probing is layered by cost and short-circuits: (1) live registries for
/// both domains are in-memory checks; (2) persisted execution records; (3)
/// checkpoint records, probed only when layers 1-2 left both domains
/// unresolved, since a conclusion already reached for one domain cannot be
/// changed by the other domain's record probe. Both domains hit means the
/// id is ambiguous and a `Conflict` is returned; neither hit means a unified
/// `ExecutionNotFound`.
pub async fn resolve_execution_with_override(
    ctx: &ApiContext,
    id: &str,
    domain: Option<ExecutionDomain>,
) -> ApiResult<ExecutionDomain> {
    // Layer 1: live registries (in-memory).
    let agent_registry = ctx.agent_loop(id).is_some();
    let workflow_registry = ctx.workflow_execution(id).is_some();
    if agent_registry && workflow_registry {
        return execution_verdict(id, true, true, domain);
    }

    // Layer 2: persisted execution records.
    let agent_persisted = !agent_registry && ctx.storage.agent_execution.load(id).await?.is_some();
    let workflow_persisted =
        !workflow_registry && ctx.storage.workflow_execution.load(id).await?.is_some();
    let agent = agent_registry || agent_persisted;
    let workflow = workflow_registry || workflow_persisted;
    if agent || workflow {
        // Exactly one domain answered (or both): layer 3 cannot change the
        // verdict for the answered side, so skip the partition probes.
        return execution_verdict(id, agent, workflow, domain);
    }

    // Layer 3: ghost ids visible only through checkpoint records.
    let agent = agent_present(ctx, id).await?;
    let workflow = workflow_present(ctx, id).await?;
    execution_verdict(id, agent, workflow, domain)
}

/// Assert the id belongs to the endpoint's domain.
///
/// Ambiguity is resolved in favor of the caller-provided domain (the route
/// itself is the explicit override); a genuine mismatch returns a
/// `Validation` error guiding the caller to the correct endpoint, and an
/// unknown id returns `ExecutionNotFound`.
pub async fn ensure_execution_domain(
    ctx: &ApiContext,
    id: &str,
    expected: ExecutionDomain,
) -> ApiResult<ExecutionDomain> {
    resolve_execution_with_override(ctx, id, Some(expected)).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use wf_resource::registry::ResourceRegistries;
    use wf_storage::context::StorageContext;

    fn make_ctx() -> ApiContext {
        ApiContext::new(
            StorageContext::new_memory(),
            Arc::new(ResourceRegistries::new()),
        )
    }

    async fn seed_agent_live(ctx: &ApiContext, id: &str) {
        let entity = Arc::new(wf_agent::entity::AgentLoopEntity::new(wf_types::Id::from(
            id.to_string(),
        )));
        let _ = ctx.agent_loops.register(entity);
    }

    async fn seed_workflow_persisted(ctx: &ApiContext, id: &str) {
        let record = wf_types::WorkflowExecution {
            id: id.to_string(),
            workflow_id: "wf-1".to_string(),
            workflow_version: None,
            status: wf_types::ExecutionStatus::Running,
            current_node_id: None,
            graph: None,
            variables: None,
            input: None,
            output: None,
            node_results: None,
            errors: None,
            error: None,
            started_at: 0,
            completed_at: None,
            execution_type: None,
            fork_join_context: None,
            hierarchy: None,
        };
        ctx.storage.workflow_execution.save(&record).await.unwrap();
    }

    #[tokio::test]
    async fn resolves_agent_only() {
        let ctx = make_ctx();
        seed_agent_live(&ctx, "exec-agent").await;
        assert_eq!(
            resolve_execution(&ctx, "exec-agent").await.unwrap(),
            ExecutionDomain::AgentLoop
        );
    }

    #[tokio::test]
    async fn resolves_workflow_only() {
        let ctx = make_ctx();
        seed_workflow_persisted(&ctx, "exec-wf").await;
        assert_eq!(
            resolve_execution(&ctx, "exec-wf").await.unwrap(),
            ExecutionDomain::Workflow
        );
    }

    #[tokio::test]
    async fn dual_hit_is_ambiguous_without_override() {
        let ctx = make_ctx();
        seed_agent_live(&ctx, "exec-both").await;
        seed_workflow_persisted(&ctx, "exec-both").await;
        let err = resolve_execution(&ctx, "exec-both").await.unwrap_err();
        assert!(matches!(err, ApiError::Conflict(_)));
        assert_eq!(
            resolve_execution_with_override(&ctx, "exec-both", Some(ExecutionDomain::Workflow))
                .await
                .unwrap(),
            ExecutionDomain::Workflow
        );
    }

    #[tokio::test]
    async fn missing_is_not_found() {
        let ctx = make_ctx();
        let err = resolve_execution(&ctx, "missing").await.unwrap_err();
        assert!(matches!(err, ApiError::ExecutionNotFound { .. }));
    }

    #[tokio::test]
    async fn override_mismatch_reports_actual_domain() {
        let ctx = make_ctx();
        seed_agent_live(&ctx, "exec-a").await;
        let err = resolve_execution_with_override(&ctx, "exec-a", Some(ExecutionDomain::Workflow))
            .await
            .unwrap_err();
        match err {
            ApiError::Validation(msg) => assert!(msg.contains("agent_loop"), "{msg}"),
            other => panic!("expected Validation, got {other:?}"),
        }
    }

    fn ghost_checkpoint(id: &str, entity_id: &str, entity_type: &str) -> wf_types::Checkpoint {
        wf_types::Checkpoint {
            id: wf_types::Id::from(id.to_string()),
            entity_type: entity_type.to_string(),
            entity_id: entity_id.to_string(),
            checkpoint_type: wf_types::checkpoint::CheckpointType::Full,
            timestamp: 1000,
            status: wf_types::checkpoint::CheckpointStatus::Active,
            previous_checkpoint_id: None,
            base_checkpoint_id: None,
            chain_root_id: None,
            chain_position: None,
            blob_size: None,
            tags: None,
            custom_fields: None,
        }
    }

    #[tokio::test]
    async fn ghost_workflow_id_resolves_via_checkpoint_partition() {
        let ctx = make_ctx();
        crate::checkpoint::record::save_checkpoint(
            &ctx.storage,
            &ghost_checkpoint("cp-ghost", "exec-ghost", "checkpoint"),
        )
        .await
        .unwrap();
        assert_eq!(
            resolve_execution(&ctx, "exec-ghost").await.unwrap(),
            ExecutionDomain::Workflow
        );
    }

    #[tokio::test]
    async fn ghost_agent_id_resolves_via_checkpoint_partition() {
        let ctx = make_ctx();
        crate::checkpoint::record::save_checkpoint(
            &ctx.storage,
            &ghost_checkpoint("cp-ghost-agent", "loop-ghost", "agent_loop"),
        )
        .await
        .unwrap();
        assert_eq!(
            resolve_execution(&ctx, "loop-ghost").await.unwrap(),
            ExecutionDomain::AgentLoop
        );
    }
}
