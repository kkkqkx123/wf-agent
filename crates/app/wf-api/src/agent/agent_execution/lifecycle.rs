use std::time::Duration;

use wf_tools::callback::AgentLoopOutput;

use crate::infra::context::ApiContext;
use crate::infra::error::ApiError;
use crate::infra::stream::ExecutionEventStream;
use wf_execution_shared::types::execution_entity::ExecutionEntity;

use super::*;

pub async fn run(
    ctx: &ApiContext,
    params: RunAgentLoopParams,
) -> crate::infra::error::ApiResult<AgentLoopOutput> {
    gate_agent_config(ctx, &params.config)?;
    let agent_loop_id = params
        .agent_loop_id
        .clone()
        .unwrap_or_else(|| wf_types::Id::from(wf_common::generate_id()));
    let coordinator = coordinator_for(ctx, &params, Some(agent_loop_id.as_str()))
        .with_agent_loop_id(agent_loop_id.clone());
    let timeout_ms = agent_timeout_ms(&params.config);
    let config = params.config.clone();
    let input = params.input.clone();
    let outcome =
        crate::infra::error::with_timeout(Duration::from_millis(timeout_ms), async move {
            coordinator.execute(config, input).await.map_err(Into::into)
        })
        .await;
    match outcome {
        Ok(output) => {
            // Persist the produced conversation so agent-loop messages are
            // queryable through `message::save` (storage injected via the
            // shared context, mirroring the checkpoint wiring pattern).
            // Messages are scoped to the per-run agent loop id.
            super::checkpoint::persist_conversation(
                ctx,
                &output.agent_loop_id,
                &output.conversation,
            )
            .await;
            Ok(output)
        }
        Err(e) => {
            // A wall-clock timeout drops the coordinator mid-loop: the
            // start record stays `Running`. Mark the live entity failed
            // and persist the terminal record so the agent execution store
            // reflects the failure.
            if let Some(entity) = ctx.agent_loop(&agent_loop_id.to_string()) {
                if let Err(terr) = entity.state.write().await.fail(e.to_string()) {
                    tracing::warn!(
                        error = %terr,
                        "failed to mark live agent entity failed after coordinator error"
                    );
                }
                let record = wf_agent::build_agent_execution(&entity).await;
                ctx.state_manager.persist_agent(&record).await;
            }
            Err(e)
        }
    }
}

/// Run an agent loop in streaming mode. Events (message deltas, tool
/// lifecycle, iteration boundaries, terminal outcome) flow through the
/// returned stream.
pub async fn stream(
    ctx: &ApiContext,
    params: RunAgentLoopParams,
) -> crate::infra::error::ApiResult<ExecutionEventStream> {
    gate_agent_config(ctx, &params.config)?;
    // Resolve the loop id up front (instead of inside the coordinator) so
    // the host-default approval handler can scope its interaction records
    // to the same execution.
    let agent_loop_id = params
        .agent_loop_id
        .clone()
        .unwrap_or_else(|| wf_types::Id::from(wf_common::generate_id()));
    let mut coordinator = coordinator_for(ctx, &params, Some(agent_loop_id.as_str()));
    coordinator = coordinator.with_agent_loop_id(agent_loop_id);
    let stream = coordinator
        .execute_stream(params.config, params.input)
        .await;
    Ok(ExecutionEventStream::from_agent_stream(stream))
}

/// Pause a running agent loop (checked between iterations).
pub async fn pause(ctx: &ApiContext, agent_loop_id: &str) -> crate::infra::error::ApiResult<()> {
    let entity = live_entity(ctx, agent_loop_id)?;
    entity.pause().await?;
    Ok(())
}

/// Resume a paused agent loop.
pub async fn resume(ctx: &ApiContext, agent_loop_id: &str) -> crate::infra::error::ApiResult<()> {
    let entity = live_entity(ctx, agent_loop_id)?;
    if entity.is_completed() || entity.is_failed() || entity.is_cancelled() {
        return Err(ApiError::Validation(format!(
            "agent loop {agent_loop_id} is terminal and cannot resume; restore from checkpoint instead"
        )));
    }
    entity.resume().await?;
    Ok(())
}

/// Cancel (stop) a running agent loop.
pub async fn cancel(ctx: &ApiContext, agent_loop_id: &str) -> crate::infra::error::ApiResult<()> {
    let entity = live_entity(ctx, agent_loop_id)?;
    entity.stop().await?;
    Ok(())
}

/// Resume an agent loop from a checkpoint.
///
/// This drives the engine coordinators (`resume_from_checkpoint_with_mode`)
/// over the checkpoints in `ctx.checkpoint_store` (run boundary / interval /
/// hook opt-in checkpoints as well as manual `agent_checkpoint` snapshots,
/// which share the same store and chain).
///
/// - `in_place = false` (default, branch): continues under a fresh execution
///   id linked to the source through the checkpoint lineage (live source
///   registry first, snapshot-restored manager otherwise); the source chain
///   is never mutated.
/// - `in_place = true`: continues under the source execution id; the source
///   execution must be terminal or paused (a live run is rejected), and a
///   caller-preset loop id conflicting with the source is rejected.
///
/// Ownership is checked up front: the checkpoint must be partitioned under
/// `agent_loop_id` in the checkpoint store.
pub async fn status(
    ctx: &ApiContext,
    agent_loop_id: &str,
) -> crate::infra::error::ApiResult<wf_types::ExecutionStatus> {
    let entity = live_entity(ctx, agent_loop_id)?;
    let status: wf_types::ExecutionStatus = entity.state.read().await.status().into();
    Ok(status)
}
