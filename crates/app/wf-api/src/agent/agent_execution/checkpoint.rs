use std::time::Duration;

use wf_storage::adapter::base::BaseStorageAdapter;
use wf_tools::callback::AgentLoopOutput;

use crate::infra::context::ApiContext;
use crate::infra::error::ApiError;

use super::*;

pub async fn resume_from_checkpoint(
    ctx: &ApiContext,
    agent_loop_id: &str,
    checkpoint_id: &str,
    params: RunAgentLoopParams,
    in_place: bool,
) -> crate::infra::error::ApiResult<AgentLoopOutput> {
    use wf_checkpoint::CheckpointStateManager;

    gate_agent_config(ctx, &params.config)?;
    let state_manager =
        wf_checkpoint::AgentCheckpointStateManager::new(ctx.checkpoint_store.clone());
    let owned = state_manager
        .list_by_entity(agent_loop_id)
        .await
        .map_err(|e| ApiError::execution(format!("checkpoint lookup failed: {e}")))?
        .iter()
        .any(|meta| meta.id.as_str() == checkpoint_id);
    if !owned {
        return Err(ApiError::Validation(format!(
            "checkpoint {checkpoint_id} does not belong to agent loop {agent_loop_id}"
        )));
    }
    let mut coordinator = coordinator_for(ctx, &params, None);
    if in_place {
        coordinator = coordinator.with_agent_loop_id(wf_types::Id::from(agent_loop_id.to_string()));
    } else if let Some(id) = params.agent_loop_id.clone() {
        coordinator = coordinator.with_agent_loop_id(id);
    }
    let timeout_ms = agent_timeout_ms(&params.config);
    let config = params.config.clone();
    let input = params.input.clone();
    let outcome =
        crate::infra::error::with_timeout(Duration::from_millis(timeout_ms), async move {
            coordinator
                .resume_from_checkpoint_with_mode(checkpoint_id, config, input, in_place)
                .await
                .map_err(Into::into)
        })
        .await;
    match outcome {
        Ok(output) => {
            persist_conversation(ctx, &output.agent_loop_id, &output.conversation).await;
            Ok(output)
        }
        Err(e) => Err(e),
    }
}

/// Automatically resume an agent execution from its latest checkpoint using
/// the declarative loop configuration captured in the snapshot. No caller
/// config or new user message is required; the source execution id is reused
/// in place. Missing checkpoints, terminal snapshots, missing loop configs
/// and live sources surface as validation errors so the recovery
/// orchestrator can skip with a reason instead of backfilling defaults.
pub async fn auto_resume(
    ctx: &ApiContext,
    agent_loop_id: &str,
) -> crate::infra::error::ApiResult<AgentLoopOutput> {
    use wf_checkpoint::CheckpointStateManager;

    let state_manager =
        wf_checkpoint::AgentCheckpointStateManager::new(ctx.checkpoint_store.clone());
    let latest = state_manager
        .get_latest(agent_loop_id)
        .await
        .map_err(|e| ApiError::execution(format!("checkpoint lookup failed: {e}")))?
        .ok_or_else(|| {
            ApiError::Validation(format!(
                "agent execution {agent_loop_id} has no checkpoint; auto-resume requires a checkpoint"
            ))
        })?;
    let coordinator = coordinator(ctx);
    let checkpoint_id = latest.id.clone();
    let timeout_ms = ctx
        .storage
        .agent_execution
        .load(agent_loop_id)
        .await
        .ok()
        .flatten()
        .and_then(|record| record.loop_config)
        .map(|config| agent_timeout_ms(&config))
        .unwrap_or(DEFAULT_AGENT_TIMEOUT_MS);
    let outcome =
        crate::infra::error::with_timeout(Duration::from_millis(timeout_ms), async move {
            coordinator
                .auto_resume_from_checkpoint(&checkpoint_id)
                .await
                .map_err(|e| {
                    let message = e.to_string();
                    let lower = message.to_lowercase();
                    if lower.contains("no loop_config")
                        || lower.contains("terminal")
                        || lower.contains("still live")
                        || lower.contains("invalid")
                        || lower.contains("no checkpoint")
                    {
                        ApiError::Validation(format!(
                            "agent execution {agent_loop_id} not auto-recoverable: {message}"
                        ))
                    } else {
                        ApiError::execution(format!("agent auto-resume failed: {e}"))
                    }
                })
        })
        .await?;
    persist_conversation(ctx, &outcome.agent_loop_id, &outcome.conversation).await;
    Ok(outcome)
}

/// Query the live status of an agent loop execution.
///
/// Returns the typed [`wf_types::ExecutionStatus`] (the persisted status
/// contract) instead of a Debug string, so callers can match without
/// string parsing. A timeout in the engine state reads as `Timeout`.
pub(super) async fn persist_conversation(
    ctx: &ApiContext,
    agent_loop_id: &wf_types::Id,
    conversation: &[wf_types::message::Message],
) {
    let agent_loop_id = agent_loop_id.to_string();
    for message in conversation {
        let record =
            crate::entity::message::record(&agent_loop_id, Some(&agent_loop_id), message.clone());
        if let Err(err) = ctx.storage.message.save(&record).await {
            tracing::warn!(
                target: "wf_api",
                agent_loop_id = %agent_loop_id,
                error = %err,
                "failed to persist agent conversation message"
            );
        }
    }
}

/// Persist the conversation of a stream-mode agent loop once its terminal
/// event has been delivered. Reads the live entity's append-only history and
/// upserts every message by id, so repeated calls are idempotent.
pub async fn persist_stream_conversation(ctx: &ApiContext, agent_loop_id: &str) {
    let id = wf_types::Id::from(agent_loop_id.to_string());
    let Some(entity) = ctx.agent_loops.get(&id) else {
        return;
    };
    let messages = entity.conversation().read().await.state.messages.clone();
    for message in messages {
        let record = crate::entity::message::record(agent_loop_id, Some(agent_loop_id), message);
        if let Err(err) = ctx.storage.message.save(&record).await {
            tracing::warn!(
                target: "wf_api",
                agent_loop_id = %agent_loop_id,
                error = %err,
                "failed to persist stream agent conversation message"
            );
        }
    }
}
